//! Todo section: a derived view over task lines in note bodies. Toggling and
//! adding write back to the note they came from, with the same conflict
//! handling as the editor.

use locus_client::notes::Note;
use locus_client::tasks::{add_task, parse_tasks, toggle_task};

use crate::date::Date;
use crate::msg::Effect;
use crate::notes;
use crate::state::{
    App, Focus, Input, InputKind, PendingTask, Section, Target, clamp_sel, matches_filter,
};

pub const INBOX_TITLE: &str = "Inbox";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskEntry {
    pub note_id: u64,
    pub note_title: String,
    /// 0-based line in the note body.
    pub line: usize,
    pub done: bool,
    pub text: String,
    pub due: Option<Date>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Overdue,
    Today,
    Upcoming,
    NoDate,
    Done,
}

impl GroupKind {
    pub fn title(self) -> &'static str {
        match self {
            GroupKind::Overdue => "Overdue",
            GroupKind::Today => "Today",
            GroupKind::Upcoming => "Upcoming",
            GroupKind::NoDate => "No date",
            GroupKind::Done => "Done",
        }
    }
}

/// Every task in the filter, from all note bodies that are loaded.
fn all_tasks(app: &App) -> Vec<TaskEntry> {
    let mut out = Vec::new();
    for note in &app.notes.items {
        for t in parse_tasks(&note.body_md) {
            if !matches_filter(&app.filter, &[&t.text, &note.title]) {
                continue;
            }
            out.push(TaskEntry {
                note_id: note.id,
                note_title: note.title.clone(),
                line: t.line,
                done: t.done,
                text: t.text,
                due: t.due.and_then(|(y, m, d)| Date::new(y, m, d)),
            });
        }
    }
    out
}

/// Groups in display order, each sorted by due date then note and line.
/// The Done group is always present; callers decide whether to show it.
pub fn groups(app: &App, today: Date) -> Vec<(GroupKind, Vec<TaskEntry>)> {
    let mut buckets: [Vec<TaskEntry>; 5] = Default::default();
    for t in all_tasks(app) {
        let kind = if t.done {
            GroupKind::Done
        } else {
            match t.due {
                None => GroupKind::NoDate,
                Some(d) if d < today => GroupKind::Overdue,
                Some(d) if d == today => GroupKind::Today,
                Some(_) => GroupKind::Upcoming,
            }
        };
        buckets[kind as usize].push(t);
    }
    let kinds = [
        GroupKind::Overdue,
        GroupKind::Today,
        GroupKind::Upcoming,
        GroupKind::NoDate,
        GroupKind::Done,
    ];
    kinds
        .into_iter()
        .zip(buckets)
        .map(|(kind, mut tasks)| {
            tasks.sort_by(|a, b| {
                a.due
                    .cmp(&b.due)
                    .then_with(|| a.note_title.cmp(&b.note_title))
                    .then(a.note_id.cmp(&b.note_id))
                    .then(a.line.cmp(&b.line))
            });
            (kind, tasks)
        })
        .collect()
}

/// The navigable task list: groups in order, Done only when shown.
pub fn visible(app: &App) -> Vec<TaskEntry> {
    groups(app, app.now.today)
        .into_iter()
        .filter(|(k, _)| *k != GroupKind::Done || app.todo.show_done)
        .flat_map(|(_, tasks)| tasks)
        .collect()
}

/// Ids of notes whose bodies are not loaded yet. Todo needs every body.
pub fn ensure_bodies(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(nb) = app.notebook.clone() else {
        return;
    };
    for note in &app.notes.items {
        if !app.notes.bodies.contains(&note.id) {
            fx.push(Effect::LoadNote {
                nb: nb.clone(),
                id: note.id,
            });
        }
    }
}

/// Called after any note arrives: keep Todo's bodies complete and finish an
/// add that was waiting for the Inbox body.
pub fn after_notes_changed(app: &mut App, fx: &mut Vec<Effect>) {
    if app.section == Section::Todo {
        ensure_bodies(app, fx);
    }
    let Some(pending) = app.todo.pending_add.clone() else {
        return;
    };
    let inbox = inbox_note(app).map(|n| (n.id, app.notes.bodies.contains(&n.id)));
    match inbox {
        // Inbox body is loaded: append now.
        Some((_, true)) => {}
        // Inbox still loading: wait for it.
        Some((_, false)) => return,
        // Inbox gone: create it.
        None => {}
    }
    app.todo.pending_add = None;
    add_to_inbox(app, fx, pending.text, pending.due);
}

fn inbox_note(app: &App) -> Option<&Note> {
    app.notes.items.iter().find(|n| n.title == INBOX_TITLE)
}

pub fn toggle_selected(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(task) = visible(app).get(app.todo.sel).cloned() else {
        app.info("No task selected");
        return;
    };
    let Some(nb) = app.notebook.clone() else {
        return;
    };
    let Some(note) = notes::find(app, task.note_id).cloned() else {
        return;
    };
    let editing = app
        .editor
        .as_ref()
        .filter(|e| e.note_id == note.id)
        .map(|e| (e.body.clone(), e.base_revision));
    let (base, revision) = editing.unwrap_or((note.body_md.clone(), note.revision));
    let body = toggle_task(&base, task.line);
    if body == base {
        app.info("That line is not a task");
        return;
    }
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == note.id) {
        ed.body = body.clone();
        ed.in_flight = true;
        ed.save_queued = false;
    }
    fx.push(Effect::SaveNote {
        nb,
        id: note.id,
        body,
        expected_revision: revision,
    });
}

pub fn toggle_done_visibility(app: &mut App) {
    app.todo.show_done = !app.todo.show_done;
    app.todo.sel = clamp_sel(app.todo.sel, visible(app).len());
    app.info(if app.todo.show_done {
        "Showing Done"
    } else {
        "Hiding Done"
    });
}

pub fn begin_add(app: &mut App) {
    if app.notebook.is_none() {
        app.error("No notebook yet");
        return;
    }
    app.input = Input::default();
    app.focus = Focus::Input(InputKind::TaskText);
}

/// Parse `text [due]` where due is `YYYY-MM-DD` or `due:YYYY-MM-DD` at the end.
pub fn parse_task_input(raw: &str) -> Result<(String, Option<Date>), String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("Task text is required".into());
    }
    let (head, last) = match t.rsplit_once(char::is_whitespace) {
        Some((h, l)) => (h.trim_end(), l),
        None => ("", t),
    };
    let has_prefix = last.starts_with("due:");
    let date_part = last.strip_prefix("due:").unwrap_or(last);
    match Date::parse(date_part) {
        Some(due) if !head.is_empty() => Ok((head.to_string(), Some(due))),
        Some(_) if has_prefix => Err("Task text is required".into()),
        _ if has_prefix => Err("Due date must be YYYY-MM-DD".into()),
        _ => Ok((t.to_string(), None)),
    }
}

pub fn submit_add(app: &mut App, fx: &mut Vec<Effect>) {
    match parse_task_input(&app.input.text) {
        Ok((text, due)) => {
            app.input = Input::default();
            app.focus = Focus::List;
            add_to_inbox(app, fx, text, due);
        }
        Err(e) => app.error(e),
    }
}

/// Append a task to the Inbox note, creating it when missing. If the Inbox
/// body is not loaded yet, the add waits for it.
fn add_to_inbox(app: &mut App, fx: &mut Vec<Effect>, text: String, due: Option<Date>) {
    let Some(nb) = app.notebook.clone() else {
        app.error("No notebook yet");
        return;
    };
    let due_tuple = due.map(|d| (d.y, d.m, d.d));
    let Some(inbox) = inbox_note(app).map(|n| (n.id, n.body_md.clone(), n.revision)) else {
        let folder = nb.notebook.root_folder_id;
        fx.push(Effect::CreateNote {
            nb,
            folder,
            title: INBOX_TITLE.into(),
            body: add_task("", &text, due_tuple),
        });
        return;
    };
    let (id, cached_body, cached_rev) = inbox;
    if !app.notes.bodies.contains(&id) {
        app.todo.pending_add = Some(PendingTask { text, due });
        fx.push(Effect::LoadNote { nb, id });
        return;
    }
    let editing = app
        .editor
        .as_ref()
        .filter(|e| e.note_id == id)
        .map(|e| (e.body.clone(), e.base_revision));
    let (base, revision) = editing.unwrap_or((cached_body, cached_rev));
    let body = add_task(&base, &text, due_tuple);
    if let Some(ed) = app.editor.as_mut().filter(|e| e.note_id == id) {
        ed.body = body.clone();
        ed.in_flight = true;
        ed.save_queued = false;
    }
    fx.push(Effect::SaveNote {
        nb,
        id,
        body,
        expected_revision: revision,
    });
}

/// Enter on a task: open the note it came from.
pub fn jump_to_source(app: &mut App, fx: &mut Vec<Effect>) {
    let Some(task) = visible(app).get(app.todo.sel).cloned() else {
        app.info("No task selected");
        return;
    };
    crate::update::open(app, fx, Section::Notes, Some(Target::Note(task.note_id)));
}
