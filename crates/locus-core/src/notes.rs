//! Notes section: notebooks and folders to browse, the editor with debounced
//! autosave, conflict handling (keep-both), rename and delete. Todo shares
//! this module's data.
//!
//! Every notebook on the ship is kept in `App::notes.books`. The list shows a
//! location: the notebook list, or the inside of one folder. Note ids are only
//! unique per notebook, so notes are always addressed by (notebook, id).

use locus_client::notes::{Folder, Note, NoteRevision, NotebookSummary};

use crate::msg::{Effect, Msg, Nav};
use crate::state::{
    App, Book, Editor, Focus, Input, InputKind, NbKey, NoteHistory, NoteRef, NotesLoc, Section,
    clamp_sel, matches_filter,
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
            books.sort_by(|a, b| book_order(a, b));
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

/// Notebooks sort by title (case-insensitive).
fn book_order(a: &Book, b: &Book) -> std::cmp::Ordering {
    lower(&a.nb.notebook.title)
        .cmp(&lower(&b.nb.notebook.title))
        .then(a.key().cmp(&b.key()))
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

/// What a list row shows, independent of where it sorts.
#[derive(PartialEq)]
enum RowId {
    Notebook(NbKey),
    Folder(NbKey, u64),
    Note(NbKey, u64),
}

fn row_id(row: &Row<'_>) -> RowId {
    match row {
        Row::Notebook(b) => RowId::Notebook(b.key()),
        Row::Folder(b, f) => RowId::Folder(b.key(), f.id),
        Row::Note(b, n) => RowId::Note(b.key(), n.id),
    }
}

/// Run `change` and keep the same row selected, wherever it now sorts (a
/// saved note moves to the top). Falls back to clamping when the row is gone.
fn keeping_selection(app: &mut App, change: impl FnOnce(&mut App)) {
    let before = selected(app).map(|r| row_id(&r));
    change(app);
    match before.and_then(|id| rows(app).iter().position(|r| row_id(r) == id)) {
        Some(pos) => app.notes.sel = pos,
        None => clamp(app),
    }
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
    keeping_selection(app, |app| {
        let book = ensure_book(app, nb);
        book.bodies.insert(note.id);
        match book.notes.iter_mut().find(|n| n.id == note.id) {
            Some(slot) => *slot = note,
            None => book.notes.push(note),
        }
    });
}

fn editor_is(app: &App, nb: &NbKey, id: u64) -> bool {
    app.editor.as_ref().is_some_and(|e| e.is(nb, id))
}

// ----- navigation -----

pub fn nav(app: &mut App, fx: &mut Vec<Effect>, nav: Nav) {
    if app.focus == Focus::Detail
        && let Some(h) = app.notes.history.as_mut()
    {
        match nav {
            Nav::Left => close_history(app),
            Nav::Right => {}
            other => {
                let len = h.revs.as_ref().map_or(0, Vec::len);
                h.sel = step(h.sel, len, other);
            }
        }
        return;
    }
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
    app.notes.history = None;
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
    app.info(format!("Created notebook \"{}\"", notebook.notebook.title));
    request_notebook(fx, notebook);
}

pub fn folders_loaded(app: &mut App, nb: NotebookSummary, folders: Vec<Folder>) {
    keeping_selection(app, |app| ensure_book(app, &nb).folders = folders);
}

pub fn notes_loaded(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, notes: Vec<Note>) {
    let key = NbKey::of(&nb);
    keeping_selection(app, |app| {
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
    });
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
    drop_history_if_gone(app);
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

/// `3 notes and 1 folder` for a delete prompt.
fn contents(notes: usize, folders: usize) -> String {
    let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    match (notes, folders) {
        (_, 0) => plural(notes, "note"),
        (0, _) => plural(folders, "folder"),
        _ => format!("{} and {}", plural(notes, "note"), plural(folders, "folder")),
    }
}

/// `folder` and every folder below it.
fn subtree(book: &Book, folder: u64) -> Vec<u64> {
    let mut ids = vec![folder];
    let mut i = 0;
    // Bounded by the folder count, which also guards against parent cycles.
    while i < ids.len() && ids.len() <= book.folders.len() + 1 {
        let parent = ids[i];
        ids.extend(
            book.folders
                .iter()
                .filter(|f| f.parent_folder_id == Some(parent) && f.id != parent)
                .map(|f| f.id),
        );
        i += 1;
    }
    ids
}

/// Notes and folders below `folder`, not counting `folder` itself.
fn subtree_counts(book: &Book, folder: u64) -> (usize, usize) {
    let ids = subtree(book, folder);
    let notes = book
        .notes
        .iter()
        .filter(|n| ids.contains(&n.folder_id))
        .count();
    (notes, ids.len() - 1)
}

/// Delete asks first. Folders and notebooks go with everything in them, so
/// the prompt says how much that is.
pub fn ask_delete(app: &mut App) {
    let (prompt, msg) = match selected(app) {
        Some(Row::Note(b, n)) => (
            format!("Delete note \"{}\"?", n.title),
            Msg::ExecuteDeleteNote(NoteRef {
                nb: b.key(),
                id: n.id,
            }),
        ),
        Some(Row::Folder(b, f)) => {
            let (notes, folders) = subtree_counts(b, f.id);
            let prompt = if notes + folders == 0 {
                format!("Delete empty folder \"{}\"?", f.name)
            } else {
                format!(
                    "Delete folder \"{}\" and the {} in it?",
                    f.name,
                    contents(notes, folders)
                )
            };
            (
                prompt,
                Msg::ExecuteDeleteFolder {
                    nb: b.key(),
                    id: f.id,
                },
            )
        }
        Some(Row::Notebook(b)) => {
            let (notes, folders) = subtree_counts(b, b.nb.notebook.root_folder_id);
            let prompt = if notes + folders == 0 {
                format!("Delete empty notebook \"{}\"?", b.nb.notebook.title)
            } else {
                format!(
                    "Delete notebook \"{}\" and the {} in it?",
                    b.nb.notebook.title,
                    contents(notes, folders)
                )
            };
            (prompt, Msg::ExecuteDeleteNotebook(b.key()))
        }
        None => {
            app.info("Nothing selected");
            return;
        }
    };
    app.ask(prompt, msg);
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, note: NoteRef) {
    let Some(nb) = app.notes.book(&note.nb).map(|b| b.nb.clone()) else {
        return;
    };
    fx.push(Effect::DeleteNote { nb, id: note.id });
    app.info("Deleting note");
}

pub fn execute_delete_folder(app: &mut App, fx: &mut Vec<Effect>, nb: &NbKey, id: u64) {
    let Some(nb) = app.notes.book(nb).map(|b| b.nb.clone()) else {
        return;
    };
    fx.push(Effect::DeleteNoteFolder { nb, id });
    app.info("Deleting folder");
}

pub fn execute_delete_notebook(app: &mut App, fx: &mut Vec<Effect>, nb: &NbKey) {
    let Some(nb) = app.notes.book(nb).map(|b| b.nb.clone()) else {
        return;
    };
    fx.push(Effect::DeleteNotebook { nb });
    app.info("Deleting notebook");
}

/// The open editor's note is gone: close it without saving.
fn drop_editor_if(app: &mut App, gone: impl Fn(&Editor) -> bool) {
    if app.editor.as_ref().is_some_and(gone) {
        app.editor = None;
        if app.focus == Focus::Editor {
            app.focus = Focus::List;
        }
    }
}

/// The server removed the folder's whole subtree. Reload the notebook to
/// learn what went, and leave the folder if the list was inside it.
pub fn folder_deleted(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, id: u64) {
    let key = NbKey::of(&nb);
    if let Some(book) = app.notes.book(&key) {
        let gone_folders = subtree(book, id);
        let gone_notes: Vec<u64> = book
            .notes
            .iter()
            .filter(|n| gone_folders.contains(&n.folder_id))
            .map(|n| n.id)
            .collect();
        // Leave a deleted folder for its nearest surviving ancestor.
        if let NotesLoc::Folder { nb: k, folder } = &app.notes.loc
            && *k == key
            && gone_folders.contains(folder)
        {
            let parent = book
                .folder(id)
                .and_then(|f| f.parent_folder_id)
                .unwrap_or(book.nb.notebook.root_folder_id);
            app.notes.loc = NotesLoc::Folder {
                nb: key.clone(),
                folder: parent,
            };
            app.notes.sel = 0;
        }
        drop_editor_if(app, |e| key.is(&e.nb) && gone_notes.contains(&e.note_id));
        keeping_selection(app, |app| {
            if let Some(book) = app.notes.book_mut(&key) {
                book.folders.retain(|f| !gone_folders.contains(&f.id));
                book.notes.retain(|n| !gone_notes.contains(&n.id));
            }
        });
    }
    drop_history_if_gone(app);
    request_notebook(fx, nb);
    crate::todo::after_notes_changed(app, fx);
    app.info("Folder deleted");
}

pub fn notebook_deleted(app: &mut App, nb: NotebookSummary) {
    let key = NbKey::of(&nb);
    drop_editor_if(app, |e| key.is(&e.nb));
    if matches!(&app.notes.loc, NotesLoc::Folder { nb: k, .. } if *k == key) {
        app.notes.loc = NotesLoc::Books;
        app.notes.sel = 0;
    }
    if app.notes.last_nb.as_ref() == Some(&key) {
        app.notes.last_nb = None;
    }
    keeping_selection(app, |app| app.notes.books.retain(|b| b.key() != key));
    drop_history_if_gone(app);
    app.info("Notebook deleted");
}

/// `/Projects/2026` for a folder, `/` for the notebook's top.
pub fn folder_path(book: &Book, folder: u64) -> String {
    format!("/{}", folder_names(book, folder).join("/"))
}

pub fn note_moved(app: &mut App, nb: NotebookSummary, id: u64, folder: u64) {
    let key = NbKey::of(&nb);
    keeping_selection(app, |app| {
        if let Some(n) = app
            .notes
            .book_mut(&key)
            .and_then(|b| b.notes.iter_mut().find(|n| n.id == id))
        {
            n.folder_id = folder;
        }
    });
    let label = app
        .notes
        .book(&key)
        .map_or_else(String::new, |b| path_label(b, folder));
    app.info(format!("Moved to {label}"));
}

pub fn begin_new_folder(app: &mut App) {
    if !matches!(app.notes.loc, NotesLoc::Folder { .. }) {
        app.info("Open a notebook first");
        return;
    }
    app.input = Input::default();
    app.focus = Focus::Input(InputKind::NewNoteFolder);
}

pub fn submit_new_folder(app: &mut App, fx: &mut Vec<Effect>) {
    let name = app.input.text.trim().to_string();
    if name.is_empty() || name.contains('/') {
        app.error("Folder names cannot be empty or contain /");
        return;
    }
    let NotesLoc::Folder { nb, folder } = app.notes.loc.clone() else {
        app.focus = Focus::List;
        return;
    };
    let Some(nb) = app.notes.book(&nb).map(|b| b.nb.clone()) else {
        app.focus = Focus::List;
        return;
    };
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::CreateNoteFolder {
        nb,
        parent: folder,
        name,
    });
    app.info("Creating folder");
}

pub fn begin_new_notebook(app: &mut App) {
    app.input = Input::default();
    app.focus = Focus::Input(InputKind::NewNotebook);
}

pub fn submit_new_notebook(app: &mut App, fx: &mut Vec<Effect>) {
    let title = app.input.text.trim().to_string();
    if title.is_empty() {
        app.error("Title cannot be empty");
        return;
    }
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::CreateNotebook { title });
    app.info("Creating notebook");
}

pub fn begin_rename(app: &mut App) {
    if let Some(Row::Notebook(b)) = selected(app) {
        let (key, title) = (b.key(), b.nb.notebook.title.clone());
        app.input = Input {
            text: title,
            target: None,
        };
        app.notes.folder_edit = Some((key, 0));
        app.focus = Focus::Input(InputKind::RenameNotebook);
        return;
    }
    if let Some(Row::Folder(b, f)) = selected(app) {
        let (key, id, name) = (b.key(), f.id, f.name.clone());
        app.input = Input {
            text: name,
            target: Some(id),
        };
        app.notes.folder_edit = Some((key, id));
        app.focus = Focus::Input(InputKind::RenameNoteFolder);
        return;
    }
    let Some((note, title)) = selected_note(app) else {
        app.info("Select a note or folder to rename");
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

/// Whether the selected row is a note or a folder, which Rename and Move
/// apply to.
pub fn can_rename(app: &App) -> bool {
    selected(app).is_some()
}

/// Whether Move applies: notes and folders; notebooks have no place.
pub fn can_move(app: &App) -> bool {
    matches!(selected(app), Some(Row::Note(..) | Row::Folder(..)))
}

pub fn submit_notebook_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let title = app.input.text.trim().to_string();
    if title.is_empty() {
        app.error("Title cannot be empty");
        return;
    }
    let nb = app
        .notes
        .folder_edit
        .clone()
        .and_then(|(key, _)| Some(app.notes.book(&key)?.nb.clone()));
    end_folder_prompt(app);
    let Some(nb) = nb else {
        return;
    };
    if nb.notebook.title == title {
        return;
    }
    fx.push(Effect::RenameNotebook { nb, title });
    app.info("Renaming notebook");
}

pub fn notebook_renamed(app: &mut App, nb: &NotebookSummary, title: String) {
    let key = NbKey::of(nb);
    keeping_selection(app, |app| {
        if let Some(b) = app.notes.book_mut(&key) {
            b.nb.notebook.title = title.clone();
        }
        if let Some(ed) = app.editor.as_mut().filter(|e| key.is(&e.nb)) {
            ed.nb.notebook.title = title.clone();
        }
    });
    app.info(format!("Renamed notebook to \"{title}\""));
}

/// The folder a folder prompt is for, with its notebook.
fn folder_edit_target(app: &App) -> Option<(&Book, u64)> {
    let (key, id) = app.notes.folder_edit.clone()?;
    let book = app.notes.book(&key)?;
    book.folder(id).map(|_| (book, id))
}

/// Another folder under `parent` already has `name`: paths would be ambiguous.
fn name_taken(book: &Book, parent: u64, name: &str, except: u64) -> bool {
    book.folders
        .iter()
        .any(|f| f.parent_folder_id == Some(parent) && f.name == name && f.id != except)
}

fn end_folder_prompt(app: &mut App) {
    app.notes.folder_edit = None;
    app.input = Input::default();
    app.focus = Focus::List;
}

pub fn submit_folder_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let name = app.input.text.trim().to_string();
    let Some((book, id)) = folder_edit_target(app) else {
        end_folder_prompt(app);
        return;
    };
    let root = book.nb.notebook.root_folder_id;
    let parent = book.folder(id).and_then(|f| f.parent_folder_id).unwrap_or(root);
    let err = if name.is_empty() || name.contains('/') {
        Some("A folder name cannot be empty or contain /".to_string())
    } else if name_taken(book, parent, &name, id) {
        Some(format!("A folder \"{name}\" is already there"))
    } else {
        None
    };
    if let Some(err) = err {
        app.error(err);
        return;
    }
    let unchanged = book.folder(id).is_some_and(|f| f.name == name);
    let nb = book.nb.clone();
    end_folder_prompt(app);
    if unchanged {
        return;
    }
    fx.push(Effect::UpdateNoteFolder {
        nb,
        id,
        name: Some(name),
        parent: None,
    });
    app.info("Renaming folder");
}

// ----- move -----

/// One destination in the Move picker: a folder of a loaded notebook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dest {
    pub nb: NbKey,
    pub folder: u64,
    /// `Notebook › /folder/path`; the notebook's top is `/`.
    pub label: String,
    /// Where the note or folder is now: shown, but not choosable.
    pub current: bool,
}

/// Whether the Move destination picker is open.
pub fn picking(app: &App) -> bool {
    matches!(
        app.focus,
        Focus::Input(InputKind::MoveNote | InputKind::MoveNoteFolder)
    )
}

/// What the open picker moves.
enum Moving<'a> {
    Note(&'a Book, &'a Note),
    Folder(&'a Book, &'a Folder),
}

fn moving(app: &App) -> Option<Moving<'_>> {
    match app.focus {
        Focus::Input(InputKind::MoveNote) => {
            let r = app.notes.rename.as_ref()?;
            let book = app.notes.book(&r.nb)?;
            Some(Moving::Note(book, book.note(r.id)?))
        }
        Focus::Input(InputKind::MoveNoteFolder) => {
            let (key, id) = app.notes.folder_edit.as_ref()?;
            let book = app.notes.book(key)?;
            Some(Moving::Folder(book, book.folder(*id)?))
        }
        _ => None,
    }
}

/// `folder` and every folder below it, depth first, siblings by name.
fn tree(book: &Book, folder: u64, out: &mut Vec<u64>) {
    // Bounded by the folder count, which also guards against parent cycles.
    if out.len() > book.folders.len() || out.contains(&folder) {
        return;
    }
    out.push(folder);
    let mut kids: Vec<&Folder> = book
        .folders
        .iter()
        .filter(|f| f.parent_folder_id == Some(folder) && f.id != folder)
        .collect();
    kids.sort_by(|a, b| lower(&a.name).cmp(&lower(&b.name)).then(a.id.cmp(&b.id)));
    for kid in kids {
        tree(book, kid.id, out);
    }
}

/// The destinations the picker offers, after its filter. A note may go to
/// any folder of any notebook; a folder only within its own notebook, and
/// not into itself or below it.
pub fn move_dests(app: &App) -> Vec<Dest> {
    let Some(what) = moving(app) else {
        return Vec::new();
    };
    let (mut books, here, skip) = match what {
        Moving::Note(book, note) => (
            app.notes.books.iter().collect::<Vec<_>>(),
            (book.key(), note.folder_id),
            Vec::new(),
        ),
        Moving::Folder(book, f) => {
            let parent = f
                .parent_folder_id
                .unwrap_or(book.nb.notebook.root_folder_id);
            (vec![book], (book.key(), parent), subtree(book, f.id))
        }
    };
    books.sort_by(|a, b| book_order(a, b));
    let mut out = Vec::new();
    for book in books {
        let key = book.key();
        let mut ids = Vec::new();
        tree(book, book.nb.notebook.root_folder_id, &mut ids);
        for id in ids.into_iter().filter(|id| !skip.contains(id)) {
            let label = format!("{} › {}", book.nb.notebook.title, folder_path(book, id));
            if !matches_filter(&app.input.text, &[&label]) {
                continue;
            }
            out.push(Dest {
                current: key == here.0 && id == here.1,
                nb: key.clone(),
                folder: id,
                label,
            });
        }
    }
    out
}

/// The choosable destination nearest `i`, looking down from it first (or up).
fn choosable_near(dests: &[Dest], i: usize, down: bool) -> Option<usize> {
    let i = i.min(dests.len().checked_sub(1)?);
    let below = (i..dests.len()).find(|&j| !dests[j].current);
    let above = (0..=i).rev().find(|&j| !dests[j].current);
    if down {
        below.or(above)
    } else {
        above.or(below)
    }
}

/// Highlight the first choosable destination: when the picker opens and
/// whenever its filter changes.
pub fn reset_move_sel(app: &mut App) {
    app.notes.move_sel = choosable_near(&move_dests(app), 0, true).unwrap_or(0);
}

/// Up and Down in the picker. The current place is skipped.
pub fn move_nav(app: &mut App, nav: Nav) {
    let dests = move_dests(app);
    let to = step(app.notes.move_sel, dests.len(), nav);
    let down = matches!(nav, Nav::Down | Nav::HalfDown | Nav::Bottom);
    if let Some(i) = choosable_near(&dests, to, down) {
        app.notes.move_sel = i;
    }
}

/// A click on destination `i`.
pub fn move_select(app: &mut App, i: usize) {
    app.notes.move_sel = clamp_sel(i, move_dests(app).len());
}

/// Move: open the destination picker for the selected note or folder.
pub fn begin_move(app: &mut App) {
    let (kind, key, id) = match selected(app) {
        Some(Row::Note(b, n)) => (InputKind::MoveNote, b.key(), n.id),
        Some(Row::Folder(b, f)) => (InputKind::MoveNoteFolder, b.key(), f.id),
        _ => {
            app.info("Select a note or folder to move");
            return;
        }
    };
    if kind == InputKind::MoveNote {
        app.notes.rename = Some(NoteRef { nb: key, id });
    } else {
        app.notes.folder_edit = Some((key, id));
    }
    app.input = Input {
        text: String::new(),
        target: Some(id),
    };
    app.focus = Focus::Input(kind);
    reset_move_sel(app);
}

fn end_move(app: &mut App) {
    app.notes.rename = None;
    app.notes.folder_edit = None;
    app.input = Input::default();
    app.focus = Focus::List;
}

/// Enter or the picker's Move button: move to the highlighted destination.
/// Within a notebook this is a plain move; into another notebook it asks
/// first, since the note is copied and its history stays behind.
pub fn submit_move(app: &mut App, fx: &mut Vec<Effect>) {
    enum Act {
        Folder(NotebookSummary, u64),
        Note(NotebookSummary, u64),
        Ask(String, Box<Msg>),
    }
    let Some(what) = moving(app) else {
        end_move(app);
        return;
    };
    let dests = move_dests(app);
    let Some(dest) = dests.get(app.notes.move_sel) else {
        app.info("No folder matches");
        return;
    };
    let act = match what {
        Moving::Folder(..) if dest.current => {
            app.info("The folder is already there");
            return;
        }
        Moving::Note(..) if dest.current => {
            app.info("The note is already there");
            return;
        }
        Moving::Folder(book, f) => {
            if name_taken(book, dest.folder, &f.name, f.id) {
                let err = format!("A folder \"{}\" is already there", f.name);
                app.error(err);
                return;
            }
            Act::Folder(book.nb.clone(), f.id)
        }
        Moving::Note(book, n) if book.key() == dest.nb => Act::Note(book.nb.clone(), n.id),
        Moving::Note(book, n) => {
            let to = app
                .notes
                .book(&dest.nb)
                .map_or_else(String::new, |b| b.nb.notebook.title.clone());
            Act::Ask(
                format!("Move \"{}\" to {to}?", n.title),
                Box::new(Msg::ExecuteMoveNote {
                    note: NoteRef {
                        nb: book.key(),
                        id: n.id,
                    },
                    to: dest.nb.clone(),
                    folder: dest.folder,
                }),
            )
        }
    };
    let folder = dest.folder;
    end_move(app);
    match act {
        Act::Folder(nb, id) => {
            fx.push(Effect::UpdateNoteFolder {
                nb,
                id,
                name: None,
                parent: Some(folder),
            });
            app.info("Moving folder");
        }
        Act::Note(nb, id) => {
            fx.push(Effect::MoveNote { nb, id, folder });
            app.info("Moving note");
        }
        Act::Ask(prompt, msg) => app.ask(prompt, *msg),
    }
}

/// The move into another notebook was confirmed: copy the note there, then
/// delete the original. Refused while the editor still has text on its way
/// for the note, which the copy would miss.
pub fn execute_move_note(
    app: &mut App,
    fx: &mut Vec<Effect>,
    note: &NoteRef,
    to: &NbKey,
    folder: u64,
) {
    let from = app
        .notes
        .book(&note.nb)
        .filter(|b| b.note(note.id).is_some());
    let (Some(from), Some(to)) = (from, app.notes.book(to)) else {
        app.error("Note not found");
        return;
    };
    let (from, to) = (from.nb.clone(), to.nb.clone());
    if app
        .editor
        .as_ref()
        .is_some_and(|e| e.is(&note.nb, note.id) && (e.dirty() || e.in_flight))
    {
        app.warn("The note is still saving. Try Move again in a moment.");
        return;
    }
    app.info(format!("Moving note to {}", to.notebook.title));
    fx.push(Effect::MoveNoteAcross {
        from,
        id: note.id,
        to,
        folder,
    });
}

/// The warning when a note was copied into another notebook but the
/// original could not be deleted. `locus notes mv` prints the same.
pub fn left_in_both(title: &str, from: &str, to: &str, error: &str) -> String {
    format!(
        "\"{title}\" was copied to {to}, but deleting it from {from} failed ({error}). \
         The note is now in both notebooks."
    )
}

/// Note `id` of `from` now exists in `to` as `note`, and is gone from
/// `from` unless `deleted` failed. The copy is selected, and takes the
/// original's place in the editor.
pub fn note_moved_across(
    app: &mut App,
    fx: &mut Vec<Effect>,
    from: NotebookSummary,
    id: u64,
    to: NotebookSummary,
    note: Note,
    deleted: Result<(), String>,
) {
    let (from_key, to_key) = (NbKey::of(&from), NbKey::of(&to));
    let (new_id, folder, title) = (note.id, note.folder_id, note.title.clone());
    let old_editor = app.editor.take_if(|e| e.is(&from_key, id));
    upsert(app, &to, note);
    if deleted.is_ok()
        && let Some(book) = app.notes.book_mut(&from_key)
    {
        book.notes.retain(|n| n.id != id);
        book.bodies.remove(&id);
    }
    drop_history_if_gone(app);
    enter_folder(app, to_key.clone(), folder);
    select_where(app, |r| matches!(r, Row::Note(_, n) if n.id == new_id));
    if let Some(old) = old_editor {
        let focus = app.focus;
        open_editor(app, fx, &to_key, new_id);
        // Text typed while the move ran goes on in the copy.
        if let Some(ed) = app.editor.as_mut().filter(|_| old.dirty()) {
            ed.body = old.body;
            request_save(ed, fx);
        }
        app.focus = focus;
    }
    crate::todo::after_notes_changed(app, fx);
    match deleted {
        Ok(()) => {
            let label = app
                .notes
                .book(&to_key)
                .map_or_else(String::new, |b| path_label(b, folder));
            app.info(format!("Moved to {label}"));
        }
        Err(e) => {
            let msg = left_in_both(&title, &from.notebook.title, &to.notebook.title, &e);
            app.warn(msg);
        }
    }
}

pub fn move_failed(app: &mut App, error: String) {
    app.error(format!("Move failed: {error}. Nothing changed."));
}

// ----- history -----

/// The note History applies to: the open note while typing in it, else the
/// selected note, else the open note.
fn history_target(app: &App) -> Option<(NoteRef, String)> {
    let open = app.editor.as_ref().map(|e| {
        (
            NoteRef {
                nb: NbKey::of(&e.nb),
                id: e.note_id,
            },
            e.title.clone(),
        )
    });
    if app.focus == Focus::Editor && open.is_some() {
        return open;
    }
    selected_note(app).or(open)
}

/// Whether History has a note to show.
pub fn can_show_history(app: &App) -> bool {
    app.notes.history.is_some() || history_target(app).is_some()
}

/// History: show the saved revisions of a note in place of the editor, or
/// hide them again.
pub fn toggle_history(app: &mut App, fx: &mut Vec<Effect>) {
    if app.notes.history.is_some() {
        close_history(app);
        return;
    }
    let Some((note, title)) = history_target(app) else {
        app.info("Select a note to see its history");
        return;
    };
    let Some(nb) = app.notes.book(&note.nb).map(|b| b.nb.clone()) else {
        app.error("Note not found");
        return;
    };
    flush_editor(app, fx);
    fx.push(Effect::LoadHistory { nb, id: note.id });
    app.notes.history = Some(NoteHistory {
        note,
        title,
        revs: None,
        sel: 0,
    });
    app.focus = Focus::Detail;
    app.info("Loading history");
}

/// A click picked another row: the history of a different note goes away.
pub fn close_history_unless_selected(app: &mut App) {
    let keep = match (app.notes.history.as_ref(), selected_note(app)) {
        (Some(h), Some((note, _))) => h.note == note,
        (Some(_), None) => false,
        (None, _) => return,
    };
    if !keep {
        close_history(app);
    }
}

pub fn close_history(app: &mut App) {
    if app.notes.history.take().is_some() && app.focus == Focus::Detail {
        app.focus = Focus::List;
    }
}

/// Restore: make the selected revision the note's text again, after asking.
pub fn ask_restore(app: &mut App) {
    let Some(h) = app.notes.history.as_ref() else {
        return;
    };
    let Some(rev) = h.revs.as_ref().and_then(|r| r.get(h.sel)).map(|r| r.rev) else {
        app.info("Select a revision to restore");
        return;
    };
    let prompt = format!("Restore \"{}\" to revision {rev}?", h.title);
    let note = h.note.clone();
    app.ask(prompt, Msg::ExecuteRestore { note, rev });
}

pub fn execute_restore(app: &mut App, fx: &mut Vec<Effect>, note: &NoteRef, rev: u64) {
    let Some(nb) = app.notes.book(&note.nb).map(|b| b.nb.clone()) else {
        return;
    };
    // A save still on its way could land after the restore and undo it, or
    // be refused and lose the typing. Restore once the note is saved.
    if app
        .editor
        .as_ref()
        .is_some_and(|e| e.is(&note.nb, note.id) && (e.dirty() || e.in_flight))
    {
        app.warn("The note is still saving. Try Restore again in a moment.");
        return;
    }
    fx.push(Effect::RestoreNote { nb, id: note.id, rev });
    app.info("Restoring");
}

/// The server took the old text back: show the note again, not the history.
pub fn note_restored(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, note: Note, rev: u64) {
    let (key, id) = (NbKey::of(&nb), note.id);
    if let Some(ed) = app.editor.as_mut().filter(|e| e.is(&key, id)) {
        // The restore is the newest text; the editor was saved before it.
        ed.body = note.body_md.clone();
        ed.last_saved = note.body_md.clone();
        ed.base_revision = note.revision;
        ed.conflict = false;
    }
    note_loaded(app, fx, nb, note);
    if app.notes.history.as_ref().is_some_and(|h| h.note.nb == key && h.note.id == id) {
        close_history(app);
        // Show what the note says now.
        open_editor(app, fx, &key, id);
    }
    app.info(format!("Restored revision {rev}"));
}

/// A click on revision `i`.
pub fn select_revision(app: &mut App, fx: &mut Vec<Effect>, i: usize) {
    if app.focus == Focus::Editor {
        flush_editor(app, fx);
    }
    if let Some(h) = app.notes.history.as_mut() {
        h.sel = clamp_sel(i, h.revs.as_ref().map_or(0, Vec::len));
        app.focus = Focus::Detail;
    }
}

pub fn history_loaded(
    app: &mut App,
    nb: NotebookSummary,
    id: u64,
    revs: Result<Vec<NoteRevision>, String>,
) {
    let key = NbKey::of(&nb);
    let Some(h) = app
        .notes
        .history
        .as_mut()
        .filter(|h| h.note.nb == key && h.note.id == id)
    else {
        return;
    };
    match revs {
        Ok(revs) => {
            let n = revs.len();
            h.revs = Some(revs);
            h.sel = 0;
            app.info(match n {
                0 => "No saved revisions".to_string(),
                1 => "1 saved revision".to_string(),
                n => format!("{n} saved revisions"),
            });
        }
        Err(error) => {
            close_history(app);
            app.error(format!("Could not load history: {error}"));
        }
    }
}

/// Close the history when its note no longer exists.
fn drop_history_if_gone(app: &mut App) {
    let gone = app
        .notes
        .history
        .as_ref()
        .is_some_and(|h| find(app, &h.note.nb, h.note.id).is_none());
    if gone {
        close_history(app);
    }
}
