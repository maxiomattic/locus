//! Notes section: the note list, the editor with debounced autosave, conflict
//! handling (keep-both), rename and delete. Todo shares this module's data.

use locus_client::notes::{Note, NotebookSummary};

use crate::msg::{Effect, Msg};
use crate::state::{App, Editor, Focus, Input, InputKind, Section, clamp_sel, matches_filter};

/// Debounce for autosave after the last edit.
pub const SAVE_DEBOUNCE_MS: u32 = 600;
const COPY_SUFFIX: &str = " (conflict copy)";

/// Notes in the current filter, newest first. The list index in the view and
/// in `NotesState::sel` refers to this order.
pub fn visible(app: &App) -> Vec<&Note> {
    let mut v: Vec<&Note> = app
        .notes
        .items
        .iter()
        .filter(|n| matches_filter(&app.filter, &[&n.title, &n.body_md]))
        .collect();
    v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    v
}

pub fn find(app: &App, id: u64) -> Option<&Note> {
    app.notes.items.iter().find(|n| n.id == id)
}

fn upsert(app: &mut App, note: Note) {
    app.notes.bodies.insert(note.id);
    match app.notes.items.iter_mut().find(|n| n.id == note.id) {
        Some(slot) => *slot = note,
        None => app.notes.items.push(note),
    }
}

fn editor_is(app: &App, id: u64) -> bool {
    app.editor.as_ref().is_some_and(|e| e.note_id == id)
}

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

/// Open the note with `id` in the editor, focusing it.
pub fn open_editor(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    let Some(nb) = app.notebook.clone() else {
        app.error("No notebook yet");
        return;
    };
    if editor_is(app, id) {
        app.focus = Focus::Editor;
        return;
    }
    let Some(note) = find(app, id).cloned() else {
        app.error("Note not found");
        return;
    };
    retire_editor(app, fx);
    if !app.notes.bodies.contains(&id) {
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

pub fn open_selected(app: &mut App, fx: &mut Vec<Effect>) {
    let id = visible(app).get(app.notes.sel).map(|n| n.id);
    match id {
        Some(id) => open_editor(app, fx, id),
        None => app.info("No note selected"),
    }
}

pub fn select_and_open(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    let pos = visible(app).iter().position(|n| n.id == id);
    match pos {
        Some(pos) => {
            app.notes.sel = pos;
            open_editor(app, fx, id);
        }
        None => app.info("Note is hidden by the filter"),
    }
}

pub fn new_note(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(nb) = app.notebook.clone() else {
        app.error("No notebook yet");
        return;
    };
    flush_editor(app, fx);
    app.notes.open_on_create = true;
    let folder = nb.notebook.root_folder_id;
    fx.push(Effect::CreateNote {
        nb,
        folder,
        title: "Untitled".into(),
        body: String::new(),
    });
    app.info("Creating note");
}

pub fn notebooks_loaded(app: &mut App, fx: &mut Vec<Effect>, notebooks: Vec<NotebookSummary>) {
    let current = app.notebook.as_ref();
    let chosen = current
        .and_then(|cur| {
            notebooks
                .iter()
                .find(|n| n.host == cur.host && n.flag_name == cur.flag_name)
        })
        .or(notebooks.first())
        .cloned();
    match chosen {
        Some(nb) => {
            app.notebook = Some(nb.clone());
            fx.push(Effect::LoadNotes { nb });
        }
        None => {
            // First run on a fresh ship: create the default notebook.
            app.info("Creating your first notebook");
            fx.push(Effect::CreateNotebook {
                title: "Locus".into(),
            });
        }
    }
}

pub fn notebook_created(app: &mut App, fx: &mut Vec<Effect>, notebook: NotebookSummary) {
    app.notebook = Some(notebook.clone());
    fx.push(Effect::LoadNotes { nb: notebook });
}

pub fn notes_loaded(app: &mut App, fx: &mut Vec<Effect>, nb: NotebookSummary, notes: Vec<Note>) {
    app.notebook = Some(nb);
    app.notes.bodies = notes
        .iter()
        .filter(|n| !n.body_md.is_empty())
        .map(|n| n.id)
        .collect();
    app.notes.items = notes;
    app.notes.sel = clamp_sel(app.notes.sel, visible(app).len());
    for s in [Section::Notes, Section::Todo] {
        app.loading[s.index()] = false;
        app.loaded[s.index()] = true;
    }
    let open_id = app.editor.as_ref().map(|e| e.note_id);
    if let (Some(ed), Some(note)) = (
        app.editor.as_mut(),
        open_id.and_then(|id| app.notes.items.iter().find(|n| n.id == id)),
    ) {
        ed.title = note.title.clone();
        adopt_if_clean(ed, note);
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

pub fn note_loaded(app: &mut App, fx: &mut Vec<Effect>, _nb: NotebookSummary, note: Note) {
    let id = note.id;
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
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
    upsert(app, note);
    crate::todo::after_notes_changed(app, fx);
}

pub fn note_created(app: &mut App, fx: &mut Vec<Effect>, _nb: NotebookSummary, note: Note) {
    let is_copy = note.title.ends_with(COPY_SUFFIX);
    let title = note.title.clone();
    let id = note.id;
    upsert(app, note);
    if app.notes.open_on_create {
        app.notes.open_on_create = false;
        app.notes.sel = visible(app).iter().position(|n| n.id == id).unwrap_or(0);
        open_editor(app, fx, id);
        app.info("Created note");
    } else if title == crate::todo::INBOX_TITLE {
        app.info("Added task to Inbox");
    } else if !is_copy {
        app.info("Created note");
    }
}

pub fn note_saved(app: &mut App, fx: &mut Vec<Effect>, _nb: NotebookSummary, note: Note) {
    let id = note.id;
    let mut clean = true;
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
        ed.in_flight = false;
        ed.last_saved = note.body_md.clone();
        ed.base_revision = note.revision;
        ed.title = note.title.clone();
        request_save(ed, fx);
        clean = !ed.dirty();
    }
    upsert(app, note);
    if clean {
        app.info("Saved");
    }
}

pub fn note_renamed(app: &mut App, _nb: NotebookSummary, id: u64, title: String) {
    if let Some(n) = app.notes.items.iter_mut().find(|n| n.id == id) {
        n.title = title.clone();
    }
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
        ed.title = title;
    }
    app.info("Renamed");
}

pub fn note_deleted(app: &mut App, _nb: NotebookSummary, id: u64) {
    app.notes.items.retain(|n| n.id != id);
    app.notes.bodies.remove(&id);
    if editor_is(app, id) {
        app.editor = None;
        if app.focus == Focus::Editor {
            app.focus = Focus::List;
        }
    }
    app.notes.sel = clamp_sel(app.notes.sel, visible(app).len());
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
    let title = find(app, id)
        .map(|n| n.title.clone())
        .or_else(|| app.editor.as_ref().map(|e| e.title.clone()))
        .unwrap_or_else(|| "Untitled".into());
    let copy_title = format!("{title}{COPY_SUFFIX}");
    let folder = nb.notebook.root_folder_id;
    fx.push(Effect::CreateNote {
        nb: nb.clone(),
        folder,
        title: copy_title.clone(),
        body,
    });
    fx.push(Effect::LoadNote { nb, id });
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
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

pub fn save_failed(app: &mut App, _nb: NotebookSummary, id: u64, error: String) {
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
        ed.in_flight = false;
        ed.save_queued = false;
    }
    for s in [Section::Notes, Section::Todo] {
        app.loading[s.index()] = false;
    }
    app.error(format!("Save failed: {error}. Your text is kept."));
}

pub fn ask_delete(app: &mut App) {
    let Some(note) = visible(app)
        .get(app.notes.sel)
        .map(|n| (n.id, n.title.clone()))
    else {
        app.info("No note selected");
        return;
    };
    app.ask(
        format!("Delete note \"{}\"?", note.1),
        Msg::ExecuteDelete {
            section: Section::Notes,
            id: note.0,
        },
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    let Some(nb) = app.notebook.clone() else {
        return;
    };
    fx.push(Effect::DeleteNote { nb, id });
    app.info("Deleting note");
}

pub fn begin_rename(app: &mut App) {
    let Some((id, title)) = visible(app)
        .get(app.notes.sel)
        .map(|n| (n.id, n.title.clone()))
    else {
        app.info("No note selected");
        return;
    };
    app.input = Input {
        text: title,
        target: Some(id),
    };
    app.focus = Focus::Input(InputKind::RenameNote);
}

pub fn submit_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let title = app.input.text.trim().to_string();
    if title.is_empty() {
        app.error("Title cannot be empty");
        return;
    }
    let (Some(id), Some(nb)) = (app.input.target, app.notebook.clone()) else {
        app.focus = Focus::List;
        return;
    };
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::RenameNote { nb, id, title });
}
