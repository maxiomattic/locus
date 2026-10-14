//! Notes section: notebooks and folders to browse, the editor with debounced
//! autosave, conflict handling (keep-both), rename and delete. Todo shares
//! this module's data.
//!
//! Every notebook on the ship is kept in `App::notes.books`. The list shows a
//! location: the notebook list, or the inside of one folder. Note ids are only
//! unique per notebook, so notes are always addressed by (notebook, id).

use locus_client::notes::{Folder, Note, NotebookSummary};

use crate::msg::{Effect, Msg, Nav};
use crate::state::{
    App, Book, Editor, Focus, Input, InputKind, NbKey, NoteRef, NotesLoc, Section, clamp_sel,
    matches_filter,
};
use crate::update::step;

/// Debounce for autosave after the last edit.
pub const SAVE_DEBOUNCE_MS: u32 = 600;
const COPY_SUFFIX: &str = " (conflict copy)";

/// One row of the Notes list: notebooks, or subfolders then notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row<'a> {
    Notebook(&'a Book),
    Folder(&'a Book, &'a Folder),
    Note(&'a Book, &'a Note),
}

/// Rows at the current location after the filter. Notebooks sort by title,
/// then subfolders by name (case-insensitive), then notes newest first. The
/// list index in the view and in `NotesState::sel` refers to this order.
pub fn rows(app: &App) -> Vec<Row<'_>> {
    match &app.notes.loc {
        NotesLoc::Books => {
            let mut books: Vec<&Book> = app
                .notes
                .books
                .iter()
                .filter(|b| matches_filter(&app.filter, &[&b.nb.notebook.title]))
                .collect();
            books.sort_by(|a, b| {
                lower(&a.nb.notebook.title)
                    .cmp(&lower(&b.nb.notebook.title))
                    .then(a.key().cmp(&b.key()))
            });
            books.into_iter().map(Row::Notebook).collect()
        }
        NotesLoc::Folder { nb, folder } => {
            let Some(book) = app.notes.book(nb) else {
                return Vec::new();
            };
            let mut folders: Vec<&Folder> = book
                .folders
                .iter()
                .filter(|f| f.parent_folder_id == Some(*folder) && f.id != *folder)
                .filter(|f| matches_filter(&app.filter, &[&f.name]))
                .collect();
            folders.sort_by(|a, b| lower(&a.name).cmp(&lower(&b.name)).then(a.id.cmp(&b.id)));
            let mut notes: Vec<&Note> = book
                .notes
                .iter()
                .filter(|n| n.folder_id == *folder)
                .filter(|n| matches_filter(&app.filter, &[&n.title, &n.body_md]))
                .collect();
            notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
            folders
                .into_iter()
                .map(|f| Row::Folder(book, f))
                .chain(notes.into_iter().map(|n| Row::Note(book, n)))
                .collect()
        }
    }
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

pub fn selected(app: &App) -> Option<Row<'_>> {
    rows(app).into_iter().nth(app.notes.sel)
}

/// The selected row's note, when it is one.
fn selected_note(app: &App) -> Option<(NoteRef, String)> {
    match selected(app)? {
        Row::Note(book, n) => Some((
            NoteRef {
                nb: book.key(),
                id: n.id,
            },
            n.title.clone(),
        )),
        _ => None,
    }
}

pub fn find<'a>(app: &'a App, nb: &NbKey, id: u64) -> Option<&'a Note> {
    app.notes.book(nb)?.note(id)
}

/// Number of rows the list shows, for selection clamping.
pub fn len(app: &App) -> usize {
    rows(app).len()
}

fn clamp(app: &mut App) {
    app.notes.sel = clamp_sel(app.notes.sel, len(app));
}

/// Folder names from just below the root down to `folder`.
pub fn folder_names(book: &Book, folder: u64) -> Vec<String> {
    let mut names = Vec::new();
    let mut cur = Some(folder);
    // The depth bound guards against a malformed parent cycle.
    for _ in 0..64 {
        let Some(id) = cur else { break };
        if id == book.nb.notebook.root_folder_id {
            break;
        }
        match book.folder(id) {
            Some(f) => {
                names.push(f.name.clone());
                cur = f.parent_folder_id;
            }
            None => break,
        }
    }
    names.reverse();
    names
}

/// `Notebook / Folder / Sub` for a folder, with no root segment.
pub fn path_label(book: &Book, folder: u64) -> String {
    let mut parts = vec![book.nb.notebook.title.clone()];
    parts.extend(folder_names(book, folder));
    parts.join(" / ")
}

/// The line above the list: the notebook list, or the folder's path.
pub fn breadcrumb(app: &App) -> String {
    match &app.notes.loc {
        NotesLoc::Books => "Notebooks".into(),
        NotesLoc::Folder { nb, folder } => app
            .notes
            .book(nb)
            .map_or_else(|| "Notebooks".into(), |b| path_label(b, *folder)),
    }
}

/// The notebook new notes and tasks go to: the one being browsed, else the
/// one opened last, else the first on the ship.
pub fn default_notebook(app: &App) -> Option<&NotebookSummary> {
    let browsing = match &app.notes.loc {
        NotesLoc::Folder { nb, .. } => Some(nb),
        NotesLoc::Books => app.notes.last_nb.as_ref(),
    };
    browsing
        .and_then(|k| app.notes.book(k))
        .or(app.notes.books.first())
        .map(|b| &b.nb)
}

fn ensure_book<'a>(app: &'a mut App, nb: &NotebookSummary) -> &'a mut Book {
    let key = NbKey::of(nb);
    let pos = match app.notes.books.iter().position(|b| b.key() == key) {
        Some(i) => i,
        None => {
            app.notes.books.push(Book::new(nb.clone()));
            app.notes.books.len() - 1
        }
    };
    &mut app.notes.books[pos]
}

fn upsert(app: &mut App, nb: &NotebookSummary, note: Note) {
    let book = ensure_book(app, nb);
    book.bodies.insert(note.id);
    match book.notes.iter_mut().find(|n| n.id == note.id) {
        Some(slot) => *slot = note,
        None => book.notes.push(note),
    }
}

fn editor_is(app: &App, nb: &NbKey, id: u64) -> bool {
    app.editor.as_ref().is_some_and(|e| e.is(nb, id))
}

// ----- navigation -----

pub fn nav(app: &mut App, fx: &mut Vec<Effect>, nav: Nav) {
    match nav {
        Nav::Left => go_up(app),
        Nav::Right => open_selected(app, fx),
        other => {
            let len = len(app);
            app.notes.sel = step(app.notes.sel, len, other);
        }
    }
}

fn enter_folder(app: &mut App, nb: NbKey, folder: u64) {
    app.notes.last_nb = Some(nb.clone());
    app.notes.loc = NotesLoc::Folder { nb, folder };
    app.notes.sel = 0;
    app.filter.clear();
}

/// Select the first row for which `pred` holds (the row 0 if none).
fn select_where(app: &mut App, pred: impl Fn(&Row<'_>) -> bool) {
    app.notes.sel = rows(app).iter().position(pred).unwrap_or(0);
}

fn go_up(app: &mut App) {
    let NotesLoc::Folder { nb, folder } = app.notes.loc.clone() else {
        return;
    };
    let Some(book) = app.notes.book(&nb) else {
        app.notes.loc = NotesLoc::Books;
        app.notes.sel = 0;
        return;
    };
    let root = book.nb.notebook.root_folder_id;
    let parent = if folder == root {
        None
    } else {
        // A folder whose parent is unknown hangs off the root.
        Some(
            book.folder(folder)
                .and_then(|f| f.parent_folder_id)
                .unwrap_or(root),
        )
    };
    app.filter.clear();
    match parent {
        None => {
            app.notes.loc = NotesLoc::Books;
            select_where(app, |r| matches!(r, Row::Notebook(b) if b.key() == nb));
        }
        Some(parent) => {
            app.notes.loc = NotesLoc::Folder {
                nb: nb.clone(),
                folder: parent,
            };
            select_where(app, |r| matches!(r, Row::Folder(_, f) if f.id == folder));
        }
    }
}

/// Enter on the selected row: open a notebook, a folder or a note.
pub fn open_selected(app: &mut App, fx: &mut Vec<Effect>) {
    enum Act {
        Enter(NbKey, u64),
        Open(NbKey, u64),
    }
    let act = match selected(app) {
        Some(Row::Notebook(b)) => Act::Enter(b.key(), b.nb.notebook.root_folder_id),
        Some(Row::Folder(b, f)) => Act::Enter(b.key(), f.id),
        Some(Row::Note(b, n)) => Act::Open(b.key(), n.id),
        None => {
            app.info("Nothing selected");
            return;
        }
    };
    match act {
        Act::Enter(nb, folder) => enter_folder(app, nb, folder),
        Act::Open(nb, id) => {
            app.notes.last_nb = Some(nb.clone());
            open_editor(app, fx, &nb, id);
        }
    }
}

/// `i` / `e`: edit the selected note. Containers are not editable.
pub fn edit_selected(app: &mut App, fx: &mut Vec<Effect>) {
    match selected(app) {
        Some(Row::Note(..)) => open_selected(app, fx),
        Some(_) => app.info("Select a note to edit"),
        None => app.info("No note selected"),
    }
}

/// Move the list to the folder of note `id` of `nb`, select it and open it.
pub fn goto_note(app: &mut App, fx: &mut Vec<Effect>, nb: &NbKey, id: u64) {
    let Some(folder) = find(app, nb, id).map(|n| n.folder_id) else {
        app.error("Note not found");
        return;
    };
    enter_folder(app, nb.clone(), folder);
    select_where(app, |r| matches!(r, Row::Note(_, n) if n.id == id));
    open_editor(app, fx, nb, id);
}

// ----- editor -----

/// Send a save for the editor if it has unsaved text. While a save is in
/// flight the request is queued and sent when that save is acknowledged.
fn request_save(ed: &mut Editor, fx: &mut Vec<Effect>) {
    if !ed.dirty() {
        ed.save_queued = false;
        return;
    }
    if ed.in_flight {
        ed.save_queued = true;
        return;
    }
    ed.in_flight = true;
    ed.save_queued = false;
    fx.push(Effect::SaveNote {
        nb: ed.nb.clone(),
        id: ed.note_id,
        body: ed.body.clone(),
        expected_revision: ed.base_revision,
    });
}

/// Save the open editor if dirty. Used when leaving the editor or section.
pub fn flush_editor(app: &mut App, fx: &mut Vec<Effect>) {
    if let Some(ed) = app.editor.as_mut() {
        request_save(ed, fx);
    }
}

pub fn save_now(app: &mut App, fx: &mut Vec<Effect>) {
    flush_editor(app, fx);
}

pub fn editor_changed(app: &mut App, body: String, fx: &mut Vec<Effect>) {
    let Some(ed) = app.editor.as_mut() else {
        return;
    };
    if ed.body == body {
        return;
    }
    ed.body = body;
    app.notes.token_seq += 1;
    ed.token = app.notes.token_seq;
    let token = ed.token;
    fx.push(Effect::ScheduleSave {
        after_ms: SAVE_DEBOUNCE_MS,
        token,
    });
}

pub fn save_timer(app: &mut App, token: u64, fx: &mut Vec<Effect>) {
    if let Some(ed) = app.editor.as_mut().filter(|e| e.token == token) {
        request_save(ed, fx);
    }
}

/// Take the open editor out of the way before another note opens. Unsaved
/// text is sent right away: a queued save would be lost with the editor.
fn retire_editor(app: &mut App, fx: &mut Vec<Effect>) {
    if let Some(old) = app.editor.take().filter(Editor::dirty) {
        fx.push(Effect::SaveNote {
            nb: old.nb,
            id: old.note_id,
            body: old.body,
            expected_revision: old.base_revision,
        });
    }
}

/// Open note `id` of notebook `key` in the editor, focusing it.
pub fn open_editor(app: &mut App, fx: &mut Vec<Effect>, key: &NbKey, id: u64) {
    if editor_is(app, key, id) {
        app.focus = Focus::Editor;
        return;
    }
    let Some((nb, note, loaded)) = app
        .notes
        .book(key)
        .and_then(|b| Some((b.nb.clone(), b.note(id)?.clone(), b.bodies.contains(&id))))
    else {
        app.error("Note not found");
        return;
    };
    retire_editor(app, fx);
    if !loaded {
        fx.push(Effect::LoadNote { nb: nb.clone(), id });
    }
    app.editor = Some(Editor {
        nb,
        note_id: id,
        title: note.title.clone(),
        body: note.body_md.clone(),
        last_saved: note.body_md,
        base_revision: note.revision,
        token: 0,
        in_flight: false,
        save_queued: false,
        conflict: false,
    });
    app.focus = Focus::Editor;
}

/// `n`: create a note in the folder being browsed. From the notebook list
/// it goes to the root of the default notebook.
pub fn new_note(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(nb) = default_notebook(app).cloned() else {
        app.error("No notebook yet");
        return;
    };
    let folder = match &app.notes.loc {
        NotesLoc::Folder { nb: k, folder } if k.is(&nb) => *folder,
        _ => nb.notebook.root_folder_id,
    };
    flush_editor(app, fx);
    app.notes.open_on_create = true;
    fx.push(Effect::CreateNote {
        nb,
        folder,
        title: "Untitled".into(),
        body: String::new(),
    });
    app.info("Creating note");
}

// ----- IO results -----

fn request_notebook(fx: &mut Vec<Effect>, nb: NotebookSummary) {
    fx.push(Effect::LoadNotes { nb: nb.clone() });
    fx.push(Effect::LoadFolders { nb });
}

pub fn notebooks_loaded(app: &mut App, fx: &mut Vec<Effect>, notebooks: Vec<NotebookSummary>) {
    if notebooks.is_empty() {
        // First run on a fresh ship: create the default notebook.
        app.info("Creating your first notebook");
        fx.push(Effect::CreateNotebook {
            title: "Locus".into(),
        });
        return;
    }
    let old = std::mem::take(&mut app.notes.books);
    app.notes.books = notebooks
        .iter()
        .map(|nb| {
            let key = NbKey::of(nb);
            let mut book = old
                .iter()
                .find(|b| b.key() == key)
                .cloned()
                .unwrap_or_else(|| Book::new(nb.clone()));
            book.nb = nb.clone();
            book.notes_loaded = false;
            book
        })
        .collect();
    if let NotesLoc::Folder { nb, .. } = &app.notes.loc
        && app.notes.book(nb).is_none()
    {
        app.notes.loc = NotesLoc::Books;
    }
    if !app.notes.loc_init {
        app.notes.loc_init = true;
        // A single notebook has no list to choose from.
        if let [only] = notebooks.as_slice() {
            enter_folder(app, NbKey::of(only), only.notebook.root_folder_id);
        }
    }
    clamp(app);
    for nb in notebooks {
        request_notebook(fx, nb);
    }
}

pub fn notebook_created(app: &mut App, fx: &mut Vec<Effect>, notebook: NotebookSummary) {
    ensure_book(app, &notebook);
    app.notes.loc_init = true;
    enter_folder(app, NbKey::of(&notebook), notebook.notebook.root_folder_id);
    request_notebook(fx, notebook);
}

pub fn folders_loaded(app: &mut App, nb: NotebookSummary, folders: Vec<Folder>) {
    ensure_book(app, &nb).folders = folders;
    clamp(app);
}

pub fn notes_loaded(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, notes: Vec<Note>) {
    let key = NbKey::of(&nb);
    let book = ensure_book(app, &nb);
    book.nb = nb;
    book.bodies = notes
        .iter()
        .filter(|n| !n.body_md.is_empty())
        .map(|n| n.id)
        .collect();
    book.requested.clear();
    book.notes = notes;
    book.notes_loaded = true;
    clamp(app);
    if app.notes.books.iter().all(|b| b.notes_loaded) {
        for s in [Section::Notes, Section::Todo] {
            app.loading[s.index()] = false;
            app.loaded[s.index()] = true;
        }
    }
    if let Some(ed) = app.editor.as_mut().filter(|e| key.is(&e.nb)) {
        let found = app
            .notes
            .books
            .iter()
            .find(|b| b.key() == key)
            .and_then(|b| b.note(ed.note_id));
        if let Some(note) = found {
            ed.title = note.title.clone();
            adopt_if_clean(ed, note);
        }
    }
    crate::todo::after_notes_changed(app, fx);
}

/// Take the server's copy into the editor when the user has nothing unsaved
/// and no save is in flight. A body the server did not send is never adopted.
fn adopt_if_clean(ed: &mut Editor, note: &Note) {
    if ed.dirty() || ed.in_flight || ed.conflict || note.body_md.is_empty() {
        return;
    }
    ed.body = note.body_md.clone();
    ed.last_saved = note.body_md.clone();
    ed.base_revision = note.revision;
}

pub fn note_loaded(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, note: Note) {
    let key = NbKey::of(&nb);
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, note.id)) {
        ed.title = note.title.clone();
        if ed.conflict {
            // Our conflict was resolved: the server copy is the new base, and
            // the user's text stays in the editor as unsaved changes.
            ed.last_saved = note.body_md.clone();
            ed.base_revision = note.revision;
            ed.conflict = false;
        } else {
            adopt_if_clean(ed, &note);
        }
    }
    upsert(app, &nb, note);
    crate::todo::after_notes_changed(app, fx);
}

pub fn note_created(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, note: Note) {
    let is_copy = note.title.ends_with(COPY_SUFFIX);
    let title = note.title.clone();
    let id = note.id;
    upsert(app, &nb, note);
    if app.notes.open_on_create {
        app.notes.open_on_create = false;
        goto_note(app, fx, &NbKey::of(&nb), id);
        app.info("Created note");
    } else if title == crate::todo::INBOX_TITLE {
        app.info("Added task to Inbox");
    } else if !is_copy {
        app.info("Created note");
    }
}

pub fn note_saved(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, note: Note) {
    let key = NbKey::of(&nb);
    let mut clean = true;
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, note.id)) {
        ed.in_flight = false;
        ed.last_saved = note.body_md.clone();
        ed.base_revision = note.revision;
        ed.title = note.title.clone();
        request_save(ed, fx);
        clean = !ed.dirty();
    }
    upsert(app, &nb, note);
    if clean {
        app.info("Saved");
    }
}

pub fn note_renamed(app: &mut App, nb: NotebookSummary, id: u64, title: String) {
    let key = NbKey::of(&nb);
    if let Some(n) = app
        .notes
        .book_mut(&key)
        .and_then(|b| b.notes.iter_mut().find(|n| n.id == id))
    {
        n.title = title.clone();
    }
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, id)) {
        ed.title = title;
    }
    app.info("Renamed");
}

pub fn note_deleted(app: &mut App, nb: NotebookSummary, id: u64) {
    let key = NbKey::of(&nb);
    if let Some(book) = app.notes.book_mut(&key) {
        book.notes.retain(|n| n.id != id);
        book.bodies.remove(&id);
    }
    if editor_is(app, &key, id) {
        app.editor = None;
        if app.focus == Focus::Editor {
            app.focus = Focus::List;
        }
    }
    clamp(app);
    app.info("Note deleted");
}

/// The server rejected a write as stale. The user's text is preserved as a
/// conflict copy, and the original is reloaded from the server.
pub fn save_conflict(
    app: &mut App,
    fx: &mut Vec<Effect>,
    nb: NotebookSummary,
    id: u64,
    body: String,
    current: Option<u64>,
) {
    let key = NbKey::of(&nb);
    let known = find(app, &key, id).map(|n| (n.title.clone(), n.folder_id));
    let title = known
        .as_ref()
        .map(|(t, _)| t.clone())
        .or_else(|| app.editor.as_ref().map(|e| e.title.clone()))
        .unwrap_or_else(|| "Untitled".into());
    let copy_title = format!("{title}{COPY_SUFFIX}");
    // The copy sits next to the original.
    let folder = known.map_or(nb.notebook.root_folder_id, |(_, f)| f);
    fx.push(Effect::CreateNote {
        nb: nb.clone(),
        folder,
        title: copy_title.clone(),
        body,
    });
    fx.push(Effect::LoadNote { nb, id });
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, id)) {
        ed.in_flight = false;
        ed.save_queued = false;
        ed.conflict = true;
        if let Some(rev) = current {
            ed.base_revision = rev;
        }
    }
    app.warn(format!(
        "Conflict: \"{title}\" changed elsewhere. Your text is kept as \"{copy_title}\"."
    ));
}

pub fn save_failed(app: &mut App, nb: NotebookSummary, id: u64, error: String) {
    let key = NbKey::of(&nb);
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, id)) {
        ed.in_flight = false;
        ed.save_queued = false;
    }
    for s in [Section::Notes, Section::Todo] {
        app.loading[s.index()] = false;
    }
    app.error(format!("Save failed: {error}. Your text is kept."));
}

pub fn ask_delete(app: &mut App) {
    let Some((note, title)) = selected_note(app) else {
        app.info("Select a note to delete");
        return;
    };
    app.ask(
        format!("Delete note \"{title}\"?"),
        Msg::ExecuteDeleteNote(note),
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, note: NoteRef) {
    let Some(nb) = app.notes.book(&note.nb).map(|b| b.nb.clone()) else {
        return;
    };
    fx.push(Effect::DeleteNote { nb, id: note.id });
    app.info("Deleting note");
}

pub fn begin_rename(app: &mut App) {
    let Some((note, title)) = selected_note(app) else {
        app.info("Select a note to rename");
        return;
    };
    app.input = Input {
        text: title,
        target: Some(note.id),
    };
    app.notes.rename = Some(note);
    app.focus = Focus::Input(InputKind::RenameNote);
}

pub fn submit_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let title = app.input.text.trim().to_string();
    if title.is_empty() {
        app.error("Title cannot be empty");
        return;
    }
    let target = app
        .notes
        .rename
        .take()
        .and_then(|r| Some((app.notes.book(&r.nb)?.nb.clone(), r.id)));
    let Some((nb, id)) = target else {
        app.focus = Focus::List;
        return;
    };
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::RenameNote { nb, id, title });
}
