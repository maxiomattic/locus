//! Plain data for the shells to render. Everything a shell needs to draw is
//! computed here, so shells contain no logic. All functions are pure reads of
//! [`App`]; times use the clock from the last `update` call.

use crate::calendar::events_on;
use crate::date::{Date, local_date_of, local_hm_of};
use crate::files::{self, Row as FileItem};
use crate::keymap;
use crate::media;
use crate::messages;
use crate::notes;
use crate::palette;
use crate::photos;
use crate::state::{App, Focus, InputKind, Level, Section};
use crate::todo::{self, GroupKind};
use locus_client::messages::{Message, Status};

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

// ----- sidebar and status -----

/// The window or page title: `Locus`, or `Locus Notes` for a window locked
/// to one section.
pub fn window_title(app: &App) -> String {
    match app.locked {
        Some(s) => format!("Locus {}", s.label()),
        None => "Locus".into(),
    }
}

/// Whether there is a level to go back up to: a folder or notebook in
/// Notes, a subdirectory in Files. Drives the toolbar's Back button.
pub fn can_go_up(app: &App) -> bool {
    match app.section {
        Section::Notes => !matches!(app.notes.loc, crate::state::NotesLoc::Books),
        Section::Files => !app.files.cwd.trim_matches('/').is_empty(),
        _ => false,
    }
}

/// Whether the selection is an item that Delete, Rename or Edit apply to,
/// so shells can disable those buttons otherwise.
pub fn has_item_selected(app: &App) -> bool {
    match app.section {
        Section::Notes => notes_list(app).iter().any(|r| r.selected && !r.is_dir),
        Section::Todo => todo_groups(app)
            .iter()
            .any(|g| g.items.iter().any(|t| t.selected)),
        Section::Calendar => events_on(app, app.calendar.day).len() > app.calendar.sel,
        Section::Photos => photo_grid(app, app.photos.columns.max(1))
            .rows
            .iter()
            .any(|r| r.iter().any(|c| c.selected)),
        // Folders can be renamed and moved too; Delete allows only empty ones.
        Section::Files => file_rows(app).iter().any(|r| r.selected),
        Section::Contacts => contact_detail(app).is_some(),
        Section::Movies | Section::Music => media::selected(app, app.section).is_some(),
        Section::Messages => app.messages.open.is_some(),
    }
}

/// Whether Rename and Move apply: in Notes a note or a folder (notebooks
/// cannot be renamed or moved), elsewhere the selected item.
pub fn can_rename(app: &App) -> bool {
    match app.section {
        Section::Notes => notes::can_rename(app),
        _ => has_item_selected(app),
    }
}

/// Notes: whether Move applies (a note or folder, not a notebook).
pub fn can_move(app: &App) -> bool {
    app.section == Section::Notes && notes::can_move(app)
}

/// Files: any row, folder or file, is selected (Copy path works on both).
pub fn has_file_row_selected(app: &App) -> bool {
    crate::files::selected(app).is_some()
}

/// Whether Delete has something to act on. In Notes that includes folders
/// and notebooks, which go with everything in them; in Files, empty folders.
pub fn can_delete(app: &App) -> bool {
    match app.section {
        Section::Notes => notes_list(app).iter().any(|r| r.selected),
        Section::Files => match crate::files::selected(app) {
            Some(crate::files::Row::Folder { path, .. }) => crate::files::dir_is_empty(app, &path),
            Some(_) => true,
            None => false,
        },
        Section::Messages => messages::thread_has_deletable(app),
        _ => has_item_selected(app),
    }
}

/// One line for the bottom bar: section, status, loading and filter state.
pub fn status_line(app: &App) -> String {
    let mut s = app.section.label().to_string();
    if !app.status.text.is_empty() {
        s.push_str(" | ");
        s.push_str(&app.status.text);
    }
    if app.is_loading(app.section) {
        s.push_str(" | loading...");
    }
    if !app.filter.is_empty() {
        s.push_str(&format!(" | filter: {}", app.filter));
    }
    s
}

pub fn status_level(app: &App) -> Level {
    app.status.level
}

/// The selected calendar day, e.g. `Sun 11 October 2026`.
pub fn selected_day_label(app: &App) -> String {
    long_date_label(app.calendar.day)
}

/// Agenda view: the selected day's own line, unless the agenda already
/// starts with that day's heading.
pub fn agenda_day_label(app: &App) -> Option<String> {
    let shown = agenda(app).first().is_some_and(|d| d.is_selected);
    (!shown).then(|| selected_day_label(app))
}

// ----- notes -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub title: String,
    pub subtitle: String,
    pub selected: bool,
    /// A notebook or folder: Enter opens it instead of an editor.
    pub is_dir: bool,
}

pub fn notes_list(app: &App) -> Vec<Row> {
    let tz = app.now.tz_offset_secs;
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    notes::rows(app)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == app.notes.sel;
            match row {
                notes::Row::Notebook(b) => Row {
                    title: b.nb.notebook.title.clone(),
                    subtitle: plural(b.notes.len(), "note", "notes"),
                    selected,
                    is_dir: true,
                },
                notes::Row::Folder(b, f) => {
                    let items = b.notes.iter().filter(|n| n.folder_id == f.id).count()
                        + b.folders
                            .iter()
                            .filter(|c| c.parent_folder_id == Some(f.id))
                            .count();
                    Row {
                        title: f.name.clone(),
                        subtitle: plural(items, "item", "items"),
                        selected,
                        is_dir: true,
                    }
                }
                notes::Row::Note(_, n) => Row {
                    title: n.title.clone(),
                    subtitle: date_label(n.updated_at, tz),
                    selected,
                    is_dir: false,
                },
            }
        })
        .collect()
}

/// The line above the Notes list: `Notebook / Folder / Sub`, or `Notebooks`
/// at the notebook list.
pub fn notes_breadcrumb(app: &App) -> String {
    notes::breadcrumb(app)
}

/// The editor pane, when a note is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorView {
    /// Identifies the note across notebooks (`host/flag_name/id`). Shells
    /// compare it, not `note_id`, to tell when a different note opened.
    pub uid: String,
    pub note_id: u64,
    pub title: String,
    pub body: String,
    pub dirty: bool,
    pub saving: bool,
    pub conflict: bool,
    pub focused: bool,
}

pub fn editor(app: &App) -> Option<EditorView> {
    app.editor.as_ref().map(|e| EditorView {
        uid: e.uid(),
        note_id: e.note_id,
        title: e.title.clone(),
        body: e.body.clone(),
        dirty: e.dirty(),
        saving: e.in_flight,
        conflict: e.conflict,
        focused: app.focus == Focus::Editor,
    })
}

/// Notes: whether History has a note to show (or is open, to close it).
pub fn can_show_history(app: &App) -> bool {
    app.section == Section::Notes && notes::can_show_history(app)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionRow {
    pub label: String,
    /// `2026-10-14 11:14 · ~author`.
    pub detail: String,
    pub selected: bool,
}

/// A note's saved revisions, shown in place of the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryView {
    pub title: String,
    pub loading: bool,
    pub rows: Vec<RevisionRow>,
    /// Title and body of the selected revision.
    pub rev_title: String,
    pub body: String,
    /// A revision is selected, so Restore has something to restore.
    pub can_restore: bool,
}

pub fn note_history(app: &App) -> Option<HistoryView> {
    let h = app.notes.history.as_ref()?;
    let tz = app.now.tz_offset_secs;
    let revs = h.revs.as_deref().unwrap_or_default();
    let rows = revs
        .iter()
        .enumerate()
        .map(|(i, r)| RevisionRow {
            label: format!("Revision {}", r.rev),
            detail: format!(
                "{} {} · {}",
                date_label(r.at, tz),
                hm(local_hm_of(r.at as i64, tz)),
                short_ship(&r.author)
            ),
            selected: i == h.sel,
        })
        .collect();
    let sel = revs.get(h.sel);
    Some(HistoryView {
        title: h.title.clone(),
        loading: h.revs.is_none(),
        rows,
        can_restore: sel.is_some(),
        rev_title: sel.map(|r| r.title.clone()).unwrap_or_default(),
        body: sel.map(|r| r.body_md.clone()).unwrap_or_default(),
    })
}

// ----- todo -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRow {
    pub text: String,
    pub due_label: String,
    pub done: bool,
    pub source_note_title: String,
    pub selected: bool,
    pub overdue: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: &'static str,
    pub count: usize,
    /// True for Done while hidden; `items` is empty then.
    pub collapsed: bool,
    pub items: Vec<TaskRow>,
}

pub fn todo_groups(app: &App) -> Vec<Group> {
    let today = app.now.today;
    let many_books = app.notes.books.len() > 1;
    let mut index = 0usize;
    todo::groups(app, today)
        .into_iter()
        .map(|(kind, tasks)| {
            let hidden = kind == GroupKind::Done && !app.todo.show_done;
            let count = tasks.len();
            let mut items = Vec::new();
            if !hidden {
                for t in tasks {
                    let selected = index == app.todo.sel;
                    index += 1;
                    items.push(TaskRow {
                        due_label: t.due.map(Date::iso).unwrap_or_default(),
                        overdue: kind == GroupKind::Overdue,
                        text: t.text,
                        done: t.done,
                        source_note_title: if many_books {
                            format!("{} / {}", t.notebook_title, t.note_title)
                        } else {
                            t.note_title
                        },
                        selected,
                    });
                }
            }
            Group {
                title: kind.title(),
                count,
                collapsed: hidden,
                items,
            }
        })
        .collect()
}

// ----- calendar -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    pub id: u64,
    /// `09:00-10:30`.
    pub time_label: String,
    pub title: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaDay {
    pub date: Date,
    pub label: String,
    pub is_today: bool,
    pub is_selected: bool,
    pub events: Vec<EventRow>,
}

/// Agenda: the next 30 days starting at the selected day, days with events only.
pub fn agenda(app: &App) -> Vec<AgendaDay> {
    let tz = app.now.tz_offset_secs;
    let mut out = Vec::new();
    for offset in 0..30 {
        let date = app.calendar.day.add_days(offset);
        let evs = events_on(app, date);
        if evs.is_empty() {
            continue;
        }
        let is_selected_day = date == app.calendar.day;
        let events = evs
            .iter()
            .enumerate()
            .map(|(i, e)| EventRow {
                id: e.id,
                time_label: format!(
                    "{}-{}",
                    hm(local_hm_of(e.start as i64, tz)),
                    hm(local_hm_of(e.end as i64, tz))
                ),
                title: e.title.clone(),
                selected: is_selected_day && i == app.calendar.sel,
            })
            .collect();
        out.push(AgendaDay {
            date,
            label: long_date_label(date),
            is_today: date == app.now.today,
            is_selected: is_selected_day,
            events,
        });
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthCell {
    pub date: Date,
    pub day: u8,
    pub in_month: bool,
    pub is_today: bool,
    pub is_selected: bool,
    pub event_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthView {
    pub title: String,
    /// Six Monday-first weeks, always 42 cells.
    pub weeks: [[MonthCell; 7]; 6],
}

pub fn month_grid(app: &App) -> MonthView {
    let shown = app.calendar.day;
    let first = shown.first_of_month();
    let start = first.add_days(-i64::from(first.weekday_mon0()));
    let weeks = std::array::from_fn(|w| {
        std::array::from_fn(|d| {
            let date = start.add_days((w * 7 + d) as i64);
            MonthCell {
                date,
                day: date.d,
                in_month: date.m == shown.m && date.y == shown.y,
                is_today: date == app.now.today,
                is_selected: date == shown,
                event_count: events_on(app, date).len(),
            }
        })
    });
    MonthView {
        title: format!("{} {}", MONTHS[usize::from(shown.m) - 1], shown.y),
        weeks,
    }
}

/// The event form, when one is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormView {
    pub title: &'static str,
    pub fields: Vec<FieldView>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldView {
    pub label: &'static str,
    pub value: String,
    pub active: bool,
}

/// How a form field's key reads on screen: `title` → `Title`.
fn field_label(key: &'static str) -> &'static str {
    match key {
        "title" => "Title",
        "date" => "Date",
        "start" => "Start",
        "end" => "End",
        "notes" => "Notes",
        "name" => "Name",
        "email" => "Email",
        "ship" => "Ship",
        other => other,
    }
}

pub fn form(app: &App) -> Option<FormView> {
    let f = app.form.as_ref()?;
    let title = match f.kind {
        crate::state::FormKind::NewEvent => "New event",
        crate::state::FormKind::EditEvent { .. } => "Edit event",
        crate::state::FormKind::NewContact => "New contact",
        crate::state::FormKind::EditContact { .. } => "Edit contact",
    };
    Some(FormView {
        title,
        fields: f
            .fields
            .iter()
            .enumerate()
            .map(|(i, field)| FieldView {
                label: field_label(field.label),
                value: field.value.clone(),
                active: i == f.active,
            })
            .collect(),
        error: f.error.clone(),
    })
}

// ----- photos -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoCell {
    pub title: String,
    /// A video: the grid shows a play mark, not a thumbnail.
    pub is_video: bool,
    /// Content hash to fetch a thumbnail by, when the path is `sha256:<hex>`.
    pub hash: Option<String>,
    /// Catalog path otherwise.
    pub path: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoGrid {
    pub columns: usize,
    pub rows: Vec<Vec<PhotoCell>>,
    /// Index into the flattened grid.
    pub selected: usize,
}

pub fn photo_grid(app: &App, columns: usize) -> PhotoGrid {
    let columns = columns.max(1);
    let cells: Vec<PhotoCell> = photos::visible(app)
        .into_iter()
        .enumerate()
        .map(|(i, p)| PhotoCell {
            title: p.title.clone(),
            is_video: photos::is_video(p),
            hash: photos::hash_of(&p.path).map(str::to_string),
            path: p.path.clone(),
            selected: i == app.photos.sel,
        })
        .collect();
    let rows = cells.chunks(columns).map(<[PhotoCell]>::to_vec).collect();
    PhotoGrid {
        columns,
        rows,
        selected: app.photos.sel,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoDetail {
    pub title: String,
    /// The viewer plays this instead of showing an image.
    pub is_video: bool,
    pub path: String,
    pub hash: Option<String>,
    pub tags: Vec<String>,
    pub album: String,
    /// The line under the photo: `Album: /Trips · beach · 2026`.
    pub meta: String,
    /// `3 / 12`.
    pub position: String,
}

/// The photo shown in the viewer (Detail focus), if any.
pub fn photo_detail(app: &App) -> Option<PhotoDetail> {
    let list = photos::visible(app);
    let p = list.get(app.photos.sel)?;
    let hash = photos::hash_of(&p.path).map(str::to_string);
    let album = p.album.trim_matches('/');
    let mut meta = vec![if album.is_empty() {
        "No album".to_string()
    } else {
        format!("Album: /{album}")
    }];
    // A stored photo's path is only its content hash; other paths say where it is.
    if hash.is_none() && !p.path.is_empty() {
        meta.push(p.path.clone());
    }
    meta.extend(p.tags.iter().cloned());
    Some(PhotoDetail {
        title: p.title.clone(),
        is_video: photos::is_video(p),
        path: p.path.clone(),
        hash,
        tags: p.tags.clone(),
        album: p.album.clone(),
        meta: meta.join("  ·  "),
        position: format!("{} / {}", app.photos.sel + 1, list.len()),
    })
}

// ----- files -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// Empty for folders.
    pub size_label: String,
    /// Empty for folders.
    pub modified_label: String,
    pub selected: bool,
}

pub fn file_rows(app: &App) -> Vec<FileRow> {
    let tz = app.now.tz_offset_secs;
    files::rows(app)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let selected = i == app.files.sel;
            let path = files::row_path(&row);
            match row {
                FileItem::Folder { name, .. } => FileRow {
                    name,
                    path,
                    is_dir: true,
                    size_label: String::new(),
                    modified_label: String::new(),
                    selected,
                },
                FileItem::File(f) => FileRow {
                    name: f.name,
                    path,
                    is_dir: false,
                    size_label: size_label(f.size),
                    modified_label: date_label(f.modified, tz),
                    selected,
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub path: String,
}

/// `/`, then one crumb per path segment, each with its absolute path.
pub fn breadcrumb(app: &App) -> Vec<Crumb> {
    let mut out = vec![Crumb {
        label: "/".into(),
        path: "/".into(),
    }];
    let mut path = String::new();
    for seg in app.files.cwd.split('/').filter(|s| !s.is_empty()) {
        path.push('/');
        path.push_str(seg);
        out.push(Crumb {
            label: seg.to_string(),
            path: path.clone(),
        });
    }
    out
}

// ----- contacts -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactRow {
    pub name: String,
    /// Email and ship, joined with a separator; empty when neither is set.
    pub subtitle: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactDetail {
    pub name: String,
    pub email: String,
    pub ship: String,
}

pub fn contact_rows(app: &App) -> Vec<ContactRow> {
    crate::contacts::visible(app)
        .into_iter()
        .enumerate()
        .map(|(i, c)| ContactRow {
            name: c.name.clone(),
            subtitle: [c.email.as_str(), c.ship.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            selected: i == app.contacts.sel,
        })
        .collect()
}

pub fn contact_detail(app: &App) -> Option<ContactDetail> {
    let c = crate::contacts::visible(app)
        .get(app.contacts.sel)
        .copied()?;
    Some(ContactDetail {
        name: c.name.clone(),
        email: c.email.clone(),
        ship: c.ship.clone(),
    })
}

// ----- prompts and palette -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputView {
    pub label: &'static str,
    pub text: String,
    /// True for the login code, which shells should not echo.
    pub masked: bool,
}

/// The single-line prompt (filter, task, rename, ...), when one has focus.
/// `None` for the Move picker, which [`move_picker`] describes.
pub fn input_prompt(app: &App) -> Option<InputView> {
    let Focus::Input(kind) = app.focus else {
        return None;
    };
    if notes::picking(app) {
        return None;
    }
    let (label, text) = match kind {
        InputKind::Filter => ("filter", app.filter.clone()),
        InputKind::TaskText => (
            "task text (optional due: YYYY-MM-DD)",
            app.input.text.clone(),
        ),
        InputKind::NewDir | InputKind::NewNoteFolder => ("folder name", app.input.text.clone()),
        InputKind::NewNotebook => ("notebook title", app.input.text.clone()),
        InputKind::RenameNote => ("note title", app.input.text.clone()),
        InputKind::RenameFile => ("file name", app.input.text.clone()),
        InputKind::MoveFile => ("move to directory", app.input.text.clone()),
        InputKind::RenamePhoto => ("photo title", app.input.text.clone()),
        InputKind::MovePhoto => ("move to album", app.input.text.clone()),
        InputKind::RenameNoteFolder => ("folder name", app.input.text.clone()),
        InputKind::RenameNotebook => ("notebook title", app.input.text.clone()),
        InputKind::RenameDir => ("folder name", app.input.text.clone()),
        InputKind::MoveDir => ("move folder to (full path)", app.input.text.clone()),
        InputKind::MessageDraft => ("message", app.messages.draft.clone()),
        InputKind::NewPeer => ("ship to message, like ~zod", app.input.text.clone()),
        InputKind::NewGroup => (
            "group title, then its members, like Team ~nec ~dev",
            app.input.text.clone(),
        ),
        InputKind::MoveNote | InputKind::MoveNoteFolder => return None,
        InputKind::Login => ("ship web code", app.input.text.clone()),
    };
    Some(InputView {
        label,
        text,
        masked: kind == InputKind::Login,
    })
}

/// The Move dialog: a filter field over the places a note or folder can go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePicker {
    /// `Move "Title"` or `Move folder "Name"`.
    pub title: String,
    pub filter: String,
    pub rows: Vec<MoveRow>,
    /// Whether the highlighted row can be chosen (drives the Move button).
    pub can_move: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveRow {
    /// `Notebook › /folder/path`.
    pub label: String,
    /// False for the current place: drawn dimmed, not clickable.
    pub enabled: bool,
    pub selected: bool,
}

/// The Move destination picker, while it is open.
pub fn move_picker(app: &App) -> Option<MovePicker> {
    let title = match app.focus {
        Focus::Input(InputKind::MoveNote) => {
            let r = app.notes.rename.as_ref()?;
            format!("Move \"{}\"", notes::find(app, &r.nb, r.id)?.title)
        }
        Focus::Input(InputKind::MoveNoteFolder) => {
            let (key, id) = app.notes.folder_edit.as_ref()?;
            format!("Move folder \"{}\"", app.notes.book(key)?.folder(*id)?.name)
        }
        _ => return None,
    };
    let rows: Vec<MoveRow> = notes::move_dests(app)
        .into_iter()
        .enumerate()
        .map(|(i, d)| MoveRow {
            label: d.label,
            enabled: !d.current,
            selected: i == app.notes.move_sel,
        })
        .collect();
    Some(MovePicker {
        title,
        filter: app.input.text.clone(),
        can_move: rows.iter().any(|r| r.selected && r.enabled),
        rows,
    })
}

/// Question shown while a confirm prompt has focus.
pub fn confirm_prompt(app: &App) -> Option<String> {
    if app.focus != Focus::Confirm {
        return None;
    }
    app.confirm.as_ref().map(|c| c.prompt.clone())
}

/// The confirm dialog: the question, the line under it, and the button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmView {
    pub prompt: String,
    pub detail: &'static str,
    pub button: &'static str,
    /// Draw the button as destructive (deletes).
    pub destructive: bool,
}

pub fn confirm_view(app: &App) -> Option<ConfirmView> {
    use crate::msg::Msg;
    let prompt = confirm_prompt(app)?;
    Some(match app.confirm.as_ref().map(|c| &c.on_yes) {
        Some(Msg::ExecuteRestore { .. }) => ConfirmView {
            prompt,
            detail: "The text it has now stays in the history.",
            button: "Restore",
            destructive: false,
        },
        Some(Msg::ExecuteMoveNote { .. }) => ConfirmView {
            prompt,
            detail: "The note's revision history stays behind.",
            button: "Move",
            destructive: false,
        },
        _ => ConfirmView {
            prompt,
            detail: "This cannot be undone.",
            button: "Delete",
            destructive: true,
        },
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteRow {
    pub label: String,
    /// Secondary text, e.g. `Notebook / Folder` for a note; often empty.
    pub detail: String,
    pub hint: &'static str,
    pub selected: bool,
}

pub fn palette_rows(app: &App) -> Vec<PaletteRow> {
    let list = palette::ranked(app);
    let sel = app.palette.sel.min(list.len().saturating_sub(1));
    list.into_iter()
        .enumerate()
        .map(|(i, item)| PaletteRow {
            label: item.label,
            detail: item.detail,
            hint: item.hint,
            selected: i == sel,
        })
        .collect()
}

/// The palette's query text.
pub fn palette_query(app: &App) -> &str {
    &app.palette.query
}

/// `(keys, description)` pairs for the current section.
pub fn help(app: &App) -> Vec<(String, String)> {
    keymap::bindings(app.section)
        .into_iter()
        .map(|(k, d)| (k.to_string(), d.to_string()))
        .collect()
}

// ----- formatting -----

fn hm((h, m): (u8, u8)) -> String {
    format!("{h:02}:{m:02}")
}

fn long_date_label(d: Date) -> String {
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    format!(
        "{} {} {} {}",
        WEEKDAYS[usize::from(d.weekday_mon0())],
        d.d,
        MONTHS[usize::from(d.m) - 1],
        d.y
    )
}

/// Local calendar date of a unix instant, or empty for 0 (never written).
fn date_label(unix: u64, tz: i32) -> String {
    if unix == 0 {
        return String::new();
    }
    local_date_of(unix as i64, tz).iso()
}

/// A ship name short enough for one line: a comet's eight words become
/// `~first_last`, as Urbit apps show them. Shorter names are kept.
pub fn short_ship(ship: &str) -> String {
    let words: Vec<&str> = ship
        .trim_start_matches('~')
        .split('-')
        .filter(|w| !w.is_empty())
        .collect();
    match (words.first(), words.last()) {
        (Some(first), Some(last)) if words.len() > 4 => format!("~{first}_{last}"),
        _ => ship.to_string(),
    }
}

pub fn size_label(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// ----- movies and music -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRow {
    pub id: u64,
    /// The movie's or track's title.
    pub title: String,
    /// Movies: the folder. Music: the track number's album, or empty.
    pub subtitle: String,
    /// Music only: the album heading shown above the first track of an album.
    pub heading: Option<String>,
    pub size: String,
    pub selected: bool,
    /// This row is what the player holds.
    pub playing: bool,
}

/// The rows of the Movies or Music list, after the filter.
pub fn media_rows(app: &App, section: Section) -> Vec<MediaRow> {
    let playing = app.player.as_ref().map(|p| p.file.id);
    let sel = media::selected(app, section).map(|f| f.id);
    let mut out = Vec::new();
    let mut last_album: Option<(String, String)> = None;
    for f in media::rows(app, section) {
        let (title, subtitle, heading) = match section {
            Section::Music => {
                let album_key = media::artist_album(&f.dir);
                let album_text = album_line(&album_key.0, &album_key.1);
                let heading = if last_album.as_ref() != Some(&album_key) {
                    Some(album_text.clone())
                } else {
                    None
                };
                last_album = Some(album_key);
                (media::track_title(&f.name), album_text, heading)
            }
            _ => (media::movie_title(&f.name), f.dir.clone(), None),
        };
        out.push(MediaRow {
            id: f.id,
            title,
            subtitle,
            heading,
            size: size_label(f.size),
            selected: sel == Some(f.id),
            playing: playing == Some(f.id),
        });
    }
    out
}

/// `artist · album` for a music folder; `Unsorted` for files outside `/Music`.
fn album_line(artist: &str, album: &str) -> String {
    match (artist.is_empty(), album.is_empty()) {
        (true, _) => "Unsorted".to_string(),
        (false, true) => artist.to_string(),
        (false, false) => format!("{artist} · {album}"),
    }
}

/// `m:ss`, or `h:mm:ss` from an hour on.
pub fn clock(ms: u64) -> String {
    let total = ms / 1000;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerView {
    pub title: String,
    /// Movies: the folder. Music: `artist · album`.
    pub subtitle: String,
    pub is_video: bool,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub position: String,
    pub duration: String,
    /// Position over length, 0.0 to 1.0.
    pub fraction: f32,
    pub volume: u8,
    pub error: Option<String>,
    /// Next and Previous have somewhere to go in the list the player came from.
    pub can_next: bool,
    pub can_prev: bool,
}

/// The player, when something is loaded (whatever app is showing).
pub fn player(app: &App) -> Option<PlayerView> {
    let p = app.player.as_ref()?;
    let subtitle = match p.from {
        Section::Music => {
            let (artist, album) = media::artist_album(&p.file.dir);
            album_line(&artist, &album)
        }
        _ => p.file.dir.clone(),
    };
    let fraction = if p.duration_ms > 0 {
        (p.position_ms as f32 / p.duration_ms as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let list = media::rows(app, p.from);
    let index = list.iter().position(|f| f.id == p.file.id);
    let len = list.len();
    Some(PlayerView {
        title: match p.from {
            Section::Music => media::track_title(&p.file.name),
            _ => media::movie_title(&p.file.name),
        },
        subtitle,
        is_video: p.is_video,
        playing: p.playing,
        position_ms: p.position_ms,
        duration_ms: p.duration_ms,
        position: clock(p.position_ms),
        duration: if p.duration_ms > 0 {
            clock(p.duration_ms)
        } else {
            "--:--".into()
        },
        fraction,
        volume: p.volume,
        error: p.error.clone(),
        can_next: index.is_some_and(|i| i + 1 < len),
        can_prev: index.is_some_and(|i| i > 0 || p.position_ms > 3_000),
    })
}

// ----- messages -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationView {
    pub peer: String,
    /// What the row shows: the ship, or the title of a group.
    pub label: String,
    /// True for a group's row.
    pub group: bool,
    pub selected: bool,
    pub open: bool,
    pub unread: u64,
    /// The last message, cut to one line.
    pub preview: String,
    /// The last message's local date, or empty.
    pub when: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMessage {
    pub id: u64,
    pub outgoing: bool,
    pub body: String,
    /// Local time, `HH:MM` on today, else the date.
    pub when: String,
    /// `Sending`, `Delivered`, `Failed`, or empty for received messages.
    pub status: &'static str,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagesView {
    pub conversations: Vec<ConversationView>,
    pub open: Option<String>,
    pub thread: Vec<ThreadMessage>,
    pub draft: String,
    /// Sending is allowed: a conversation is open and the draft is not empty.
    pub can_send: bool,
    pub unread_total: u64,
    /// The open conversation's title (a group) or ship, for its header.
    pub open_label: Option<String>,
    /// The open conversation is a group. A group cannot be blocked.
    pub open_group: bool,
    /// The open conversation's ship is blocked: its messages are refused.
    pub open_blocked: bool,
    /// What the thread pane says when no conversation is open.
    pub hint: &'static str,
}

fn one_line(body: &str, max: usize) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let cut: String = flat.chars().take(max).collect();
        format!("{cut}…")
    }
}

fn message_when(unix: u64, app: &App) -> String {
    if unix == 0 {
        return String::new();
    }
    let date = local_date_of(unix as i64, app.now.tz_offset_secs);
    if date == app.now.today {
        hm(local_hm_of(unix as i64, app.now.tz_offset_secs))
    } else {
        date.iso()
    }
}

fn status_label(m: &Message) -> &'static str {
    if !m.outgoing {
        return "";
    }
    match m.status {
        Status::Pending => "Sending",
        Status::Delivered => "Delivered",
        Status::Failed => "Failed",
        Status::Received => "",
    }
}

/// The Messages app: conversations, the open thread and the draft.
pub fn messages_view(app: &App) -> MessagesView {
    let m = &app.messages;
    let conversations = m
        .conversations
        .iter()
        .enumerate()
        .map(|(i, c)| ConversationView {
            peer: c.peer.clone(),
            label: c.title.clone().unwrap_or_else(|| c.peer.clone()),
            group: locus_client::messages::group_id(&c.peer).is_some(),
            selected: i == m.sel,
            open: m.open.as_deref() == Some(c.peer.as_str()),
            unread: c.unread,
            preview: c
                .last
                .as_ref()
                .map(|l| one_line(&l.body, 60))
                .unwrap_or_default(),
            when: c
                .last
                .as_ref()
                .map(|l| message_when(l.sent_unix(), app))
                .unwrap_or_default(),
        })
        .collect();
    let thread = m
        .thread
        .iter()
        .enumerate()
        .map(|(i, t)| ThreadMessage {
            id: t.id,
            outgoing: t.outgoing,
            // In a group, a message from someone else shows who wrote it.
            body: if t.group.is_some() && !t.outgoing {
                format!("{}: {}", t.peer, t.body)
            } else {
                t.body.clone()
            },
            when: message_when(t.sent_unix(), app),
            status: status_label(t),
            selected: i == m.thread_sel,
        })
        .collect();
    let open_row = m
        .open
        .as_deref()
        .and_then(|p| m.conversations.iter().find(|c| c.peer == p));
    let open_label = open_row.map(|c| c.title.clone().unwrap_or_else(|| c.peer.clone()));
    let open_group = m
        .open
        .as_deref()
        .is_some_and(|p| locus_client::messages::group_id(p).is_some());
    let hint = if m.conversations.is_empty() && m.open.is_none() {
        "No conversations yet. Start one with New."
    } else {
        "Choose a conversation on the left."
    };
    MessagesView {
        conversations,
        open: m.open.clone(),
        open_label,
        open_group,
        thread,
        draft: m.draft.clone(),
        can_send: m.open.is_some() && !m.draft.trim().is_empty(),
        unread_total: messages::unread_total(app),
        open_blocked: m
            .open
            .as_deref()
            .is_some_and(|peer| messages::is_blocked(app, peer)),
        hint,
    }
}

// ----- virtual lists -----

/// The rows of a list a shell builds when it draws only what is on screen.
/// Rows `first..last` are built; `above` and `below` are the heights of the
/// rows left out before and after them, for the spacers that stand in for
/// them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowWindow {
    pub first: usize,
    pub last: usize,
    pub above: f32,
    pub below: f32,
}

/// The on-screen window of a list whose rows have mixed heights, such as
/// Todo (group headings and tasks) or the agenda (days and events).
/// `heights` are the rows' heights in pixels, stacked with no gap between
/// them; `offset` and `view_h` are the scrollable's scroll offset and
/// height. `overscan` more rows are kept beyond each edge. For rows of one
/// height this is the same window the shells' fixed-height lists draw.
pub fn row_window(heights: &[f32], offset: f32, view_h: f32, overscan: usize) -> RowWindow {
    let offset = offset.max(0.0);
    let end = offset + view_h.max(0.0);
    let mut first = None;
    let mut last = heights.len();
    let mut top = 0.0;
    for (i, &h) in heights.iter().enumerate() {
        if first.is_none() && top + h > offset {
            first = Some(i);
        }
        if top >= end {
            last = i;
            break;
        }
        top += h;
    }
    let last = (last + overscan).min(heights.len());
    let first = first
        .unwrap_or(heights.len())
        .saturating_sub(overscan)
        .min(last);
    RowWindow {
        first,
        last,
        above: heights[..first].iter().sum(),
        below: heights[last..].iter().sum(),
    }
}
