//! Plain data for the shells to render. Everything a shell needs to draw is
//! computed here, so shells contain no logic. All functions are pure reads of
//! [`App`]; times use the clock from the last `update` call.

use crate::calendar::events_on;
use crate::date::{Date, local_date_of, local_hm_of};
use crate::files::{self, Row as FileItem};
use crate::keymap;
use crate::notes;
use crate::palette;
use crate::photos;
use crate::state::{App, Focus, InputKind, Level, Section};
use crate::todo::{self, GroupKind};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarItem {
    pub label: &'static str,
    /// e.g. `g n`.
    pub shortcut: String,
    pub active: bool,
    /// A count worth showing (notes total, tasks due, events today).
    pub badge: Option<usize>,
}

pub fn sidebar(app: &App) -> Vec<SidebarItem> {
    let today = app.now.today;
    Section::ALL
        .into_iter()
        .map(|s| SidebarItem {
            label: s.label(),
            shortcut: format!("g {}", keymap::go_key(s)),
            active: s == app.section,
            badge: match s {
                Section::Notes => Some(app.notes.items.len()),
                Section::Todo => Some(
                    todo::groups(app, today)
                        .iter()
                        .filter(|(k, _)| matches!(k, GroupKind::Overdue | GroupKind::Today))
                        .map(|(_, t)| t.len())
                        .sum(),
                ),
                Section::Calendar => Some(events_on(app, today).len()),
                _ => None,
            },
        })
        .collect()
}

/// The window or page title: `Locus`, or `Locus Notes` for a window locked
/// to one section.
pub fn window_title(app: &App) -> String {
    match app.locked {
        Some(s) => format!("Locus {}", s.label()),
        None => "Locus".into(),
    }
}

/// Whether to draw the section sidebar / nav bar. A locked window is one
/// section's own app, so it has none.
pub fn show_sections(app: &App) -> bool {
    app.locked.is_none()
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

// ----- notes -----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub title: String,
    pub subtitle: String,
    pub selected: bool,
}

pub fn notes_list(app: &App) -> Vec<Row> {
    let tz = app.now.tz_offset_secs;
    notes::visible(app)
        .into_iter()
        .enumerate()
        .map(|(i, n)| Row {
            title: n.title.clone(),
            subtitle: date_label(n.updated_at, tz),
            selected: i == app.notes.sel,
        })
        .collect()
}

/// The editor pane, when a note is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorView {
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
        note_id: e.note_id,
        title: e.title.clone(),
        body: e.body.clone(),
        dirty: e.dirty(),
        saving: e.in_flight,
        conflict: e.conflict,
        focused: app.focus == Focus::Editor,
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
                        source_note_title: t.note_title,
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
                label: field.label,
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
    pub path: String,
    pub hash: Option<String>,
    pub tags: Vec<String>,
    /// `3 / 12`.
    pub position: String,
}

/// The photo shown in the viewer (Detail focus), if any.
pub fn photo_detail(app: &App) -> Option<PhotoDetail> {
    let list = photos::visible(app);
    let p = list.get(app.photos.sel)?;
    Some(PhotoDetail {
        title: p.title.clone(),
        path: p.path.clone(),
        hash: photos::hash_of(&p.path).map(str::to_string),
        tags: p.tags.clone(),
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
pub fn input_prompt(app: &App) -> Option<InputView> {
    let Focus::Input(kind) = app.focus else {
        return None;
    };
    let (label, text) = match kind {
        InputKind::Filter => ("filter", app.filter.clone()),
        InputKind::TaskText => (
            "task text (optional due: YYYY-MM-DD)",
            app.input.text.clone(),
        ),
        InputKind::NewDir => ("folder name", app.input.text.clone()),
        InputKind::RenameNote => ("note title", app.input.text.clone()),
        InputKind::RenameFile => ("file name", app.input.text.clone()),
        InputKind::MoveFile => ("move to directory", app.input.text.clone()),
        InputKind::Login => ("ship web code", app.input.text.clone()),
    };
    Some(InputView {
        label,
        text,
        masked: kind == InputKind::Login,
    })
}

/// Question shown while a confirm prompt has focus.
pub fn confirm_prompt(app: &App) -> Option<String> {
    if app.focus != Focus::Confirm {
        return None;
    }
    app.confirm.as_ref().map(|c| format!("{} (y/n)", c.prompt))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteRow {
    pub label: String,
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
            hint: item.hint,
            selected: i == sel,
        })
        .collect()
}

/// The palette's query text.
pub fn palette_query(app: &App) -> &str {
    &app.palette.query
}

/// `(keys, description)` pairs for the current section. A locked window
/// opens other sections in their own windows, and says so.
pub fn help(app: &App) -> Vec<(String, String)> {
    let locked = app.locked.is_some();
    keymap::bindings(app.section)
        .into_iter()
        .map(|(k, d)| {
            let d = match d {
                "go to Notes/Todo/Calendar/Photos/Files/Contacts" if locked => {
                    "open the Notes/Todo/Calendar/Photos/Files/Contacts app"
                }
                "switch section" if locked => "open another section's app",
                d => d,
            };
            (k.to_string(), d.to_string())
        })
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
