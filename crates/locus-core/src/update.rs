//! The pure transition function. [`update`] is the only entry point that
//! changes [`App`]; it dispatches to per-section modules and returns the IO
//! the shell must perform.

use crate::msg::{Effect, Msg, Nav, Now, TextEdit};
use crate::state::{App, Focus, Input, InputKind, Section, clamp_sel};
use crate::{calendar, contacts, files, notes, palette, photos, todo};

/// Apply one message. Returns effects in the order the shell should run them.
pub fn update(app: &mut App, msg: Msg, now: Now) -> Vec<Effect> {
    if !app.clock_seen {
        app.clock_seen = true;
        app.calendar.day = now.today;
    }
    app.now = now;
    let mut fx = Vec::new();
    dispatch(app, msg, now, &mut fx);
    fx
}

fn dispatch(app: &mut App, msg: Msg, now: Now, fx: &mut Vec<Effect>) {
    if !matches!(msg, Msg::GPrefix | Msg::Batch(_)) {
        app.pending_g = false;
    }
    match msg {
        // ----- intents -----
        Msg::Batch(msgs) => {
            for m in msgs {
                dispatch(app, m, now, fx);
            }
        }
        Msg::GPrefix => app.pending_g = true,
        Msg::Switch(s) => switch_section(app, fx, s),
        Msg::Reload => {
            let s = app.section;
            app.info(format!("Reloading {}", s.label()));
            load_section(app, s, fx);
        }
        Msg::Nav(nav) => nav_key(app, nav),
        Msg::Activate => activate(app, fx),
        Msg::Edit => edit(app, fx),
        Msg::New => new_item(app, fx),
        Msg::Delete => delete(app),
        Msg::Rename => rename(app),
        Msg::Yank => files::yank(app, fx),
        Msg::Upload => upload(app, fx),
        Msg::MoveFile => files::begin_move(app),
        Msg::ToggleTask => todo::toggle_selected(app, fx),
        Msg::ToggleDoneTasks => todo::toggle_done_visibility(app),
        Msg::CalendarMode => calendar::toggle_mode(app),
        Msg::ShiftMonth(n) => calendar::shift_month(app, n),
        Msg::Today => calendar::go_today(app, now),
        Msg::FocusFilter => app.focus = Focus::Input(InputKind::Filter),
        Msg::ToggleHelp => {
            app.focus = if app.focus == Focus::Help {
                Focus::List
            } else {
                Focus::Help
            };
        }
        Msg::OpenPalette => palette::open(app),
        Msg::Escape => escape(app, fx),
        Msg::Submit => submit(app, now, fx),
        Msg::SaveForm => {
            if app.focus == Focus::Form {
                submit_form(app, now, fx);
            }
        }
        Msg::NextField => move_field(app, 1),
        Msg::PrevField => move_field(app, -1),
        Msg::Text(edit) => text_edit(app, edit),
        Msg::ConfirmAnswer(yes) => confirm_answer(app, now, fx, yes),
        Msg::SaveNow => notes::save_now(app, fx),
        Msg::EditorChanged(body) => notes::editor_changed(app, body, fx),
        Msg::SaveTimer(token) => notes::save_timer(app, token, fx),
        Msg::Layout { photo_columns } => app.photos.columns = photo_columns.max(1),
        Msg::Goto { section, id } => goto(app, fx, section, id),
        Msg::ExecuteDelete { section, id } => execute_delete(app, fx, section, id),
        Msg::BeginLogin => {
            app.input = Input::default();
            app.focus = Focus::Input(InputKind::Login);
        }

        // ----- IO results -----
        Msg::NotebooksLoaded { notebooks } => notes::notebooks_loaded(app, fx, notebooks),
        Msg::NotebookCreated { notebook } => notes::notebook_created(app, fx, notebook),
        Msg::NotesLoaded { nb, notes } => notes::notes_loaded(app, fx, nb, notes),
        Msg::NoteLoaded { nb, note } => notes::note_loaded(app, fx, nb, note),
        Msg::NoteCreated { nb, note } => notes::note_created(app, fx, nb, note),
        Msg::NoteSaved { nb, note } => notes::note_saved(app, fx, nb, note),
        Msg::NoteRenamed { nb, id, title } => notes::note_renamed(app, nb, id, title),
        Msg::NoteDeleted { nb, id } => notes::note_deleted(app, nb, id),
        Msg::SaveConflict {
            nb,
            id,
            body,
            current,
        } => notes::save_conflict(app, fx, nb, id, body, current),
        Msg::SaveFailed { nb, id, error } => notes::save_failed(app, nb, id, error),
        Msg::EventsLoaded { events } => calendar::events_loaded(app, events),
        Msg::EventSaved { event } => calendar::event_saved(app, event),
        Msg::EventDeleted { id } => calendar::event_deleted(app, id),
        Msg::PhotosLoaded { photos } => photos::loaded(app, photos),
        Msg::ContactsLoaded { contacts } => contacts::loaded(app, contacts),
        Msg::ContactSaved { contact } => contacts::saved(app, contact),
        Msg::ContactDeleted { id } => contacts::deleted(app, id),
        Msg::FilesLoaded { files } => files::files_loaded(app, files),
        Msg::DirsLoaded { dirs } => files::dirs_loaded(app, dirs),
        Msg::DirCreated { path } => files::dir_created(app, path),
        Msg::FilePatched { file } => files::file_patched(app, file),
        Msg::FileDeleted { id } => files::file_deleted(app, id),
        Msg::UploadDone { dir, count } => {
            if count == 0 {
                app.info("Upload cancelled");
            } else {
                app.info(format!("Uploaded {count} file(s) to {dir}"));
                let s = app.section;
                load_section(app, s, fx);
            }
        }
        Msg::LoggedIn => {
            app.loaded = [false; 6];
            app.info("Logged in");
            let s = app.section;
            load_section(app, s, fx);
        }
        Msg::LoginFailed { error } => app.error(format!("Login failed: {error}")),
        Msg::LoadFailed { section, error } => {
            for s in loading_group(section) {
                app.loading[s.index()] = false;
                app.loaded[s.index()] = false;
            }
            app.error(format!("Could not load {}: {error}", section.label()));
        }
        Msg::WriteFailed { section, error } => {
            for s in loading_group(section) {
                app.loading[s.index()] = false;
            }
            app.error(format!("{} failed: {error}", section.label()));
        }
    }
}

/// Sections that share one fetch. Notes and Todo both read the note list.
fn loading_group(section: Section) -> Vec<Section> {
    match section {
        Section::Notes | Section::Todo => vec![Section::Notes, Section::Todo],
        s => vec![s],
    }
}

pub(crate) fn switch_section(app: &mut App, fx: &mut Vec<Effect>, section: Section) {
    notes::flush_editor(app, fx);
    app.section = section;
    app.filter.clear();
    app.focus = Focus::List;
    app.confirm = None;
    if !app.loaded[section.index()] {
        load_section(app, section, fx);
    }
    if section == Section::Calendar {
        calendar::clamp(app);
    }
    if section == Section::Todo {
        todo::ensure_bodies(app, fx);
    }
}

/// Request the data a section needs. Notes and Todo fetch the notebook
/// first when none is known yet.
pub(crate) fn load_section(app: &mut App, section: Section, fx: &mut Vec<Effect>) {
    for s in loading_group(section) {
        app.loaded[s.index()] = true;
        app.loading[s.index()] = true;
    }
    match section {
        Section::Notes | Section::Todo => match app.notebook.clone() {
            Some(nb) => fx.push(Effect::LoadNotes { nb }),
            None => fx.push(Effect::LoadNotebooks),
        },
        Section::Calendar => fx.push(Effect::LoadEvents),
        Section::Photos => fx.push(Effect::LoadPhotos),
        Section::Files => {
            fx.push(Effect::LoadFiles);
            fx.push(Effect::LoadDirs);
        }
        Section::Contacts => fx.push(Effect::LoadContacts),
    }
}

/// Move a selection by one list step. Left and Right do not move a plain list.
pub(crate) fn step(sel: usize, len: usize, nav: Nav) -> usize {
    if len == 0 {
        return 0;
    }
    let sel = clamp_sel(sel, len);
    let last = len - 1;
    match nav {
        Nav::Up => sel.saturating_sub(1),
        Nav::Down => (sel + 1).min(last),
        Nav::Top => 0,
        Nav::Bottom => last,
        Nav::HalfUp => sel.saturating_sub(10),
        Nav::HalfDown => (sel + 10).min(last),
        Nav::Left | Nav::Right => sel,
    }
}

fn nav_key(app: &mut App, nav: Nav) {
    match app.focus {
        Focus::Palette => {
            palette::move_sel(app, nav);
            return;
        }
        Focus::List | Focus::Detail => {}
        _ => return,
    }
    match app.section {
        Section::Notes => {
            let len = notes::visible(app).len();
            app.notes.sel = step(app.notes.sel, len, nav);
        }
        Section::Todo => {
            let len = todo::visible(app).len();
            app.todo.sel = step(app.todo.sel, len, nav);
        }
        Section::Calendar => calendar::nav(app, nav),
        Section::Photos => photos::nav(app, nav),
        Section::Files => files::nav(app, nav),
        Section::Contacts => {
            let len = contacts::visible(app).len();
            app.contacts.sel = step(app.contacts.sel, len, nav);
        }
    }
}

fn activate(app: &mut App, fx: &mut Vec<Effect>) {
    match app.section {
        Section::Notes => notes::open_selected(app, fx),
        Section::Todo => todo::jump_to_source(app, fx),
        Section::Calendar => calendar::edit_selected(app),
        Section::Photos => photos::open_viewer(app),
        Section::Files => files::activate(app, fx),
        Section::Contacts => contacts::open_detail(app),
    }
}

fn edit(app: &mut App, fx: &mut Vec<Effect>) {
    match app.section {
        Section::Notes => notes::open_selected(app, fx),
        Section::Calendar => calendar::edit_selected(app),
        Section::Contacts => contacts::edit_selected(app),
        _ => app.info("Nothing to edit here"),
    }
}

fn new_item(app: &mut App, fx: &mut Vec<Effect>) {
    match app.section {
        Section::Notes => notes::new_note(app, fx),
        Section::Todo => todo::begin_add(app),
        Section::Calendar => calendar::new_event(app),
        Section::Files => files::begin_new_dir(app),
        Section::Contacts => contacts::new_contact(app),
        Section::Photos => app.info("Use u to upload photos"),
    }
}

fn delete(app: &mut App) {
    match app.section {
        Section::Notes => notes::ask_delete(app),
        Section::Calendar => calendar::ask_delete(app),
        Section::Files => files::ask_delete(app),
        Section::Contacts => contacts::ask_delete(app),
        _ => app.info("Nothing to delete here"),
    }
}

fn rename(app: &mut App) {
    match app.section {
        Section::Notes => notes::begin_rename(app),
        Section::Files => files::begin_rename(app),
        _ => app.info("Nothing to rename here"),
    }
}

fn upload(app: &mut App, fx: &mut Vec<Effect>) {
    let dir = match app.section {
        Section::Photos => "/Photos".to_string(),
        Section::Files => app.files.cwd.clone(),
        _ => {
            app.info("Upload is available in Photos and Files");
            return;
        }
    };
    app.info(format!("Choose files to upload to {dir}"));
    fx.push(Effect::UploadFiles { dir });
}

fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, section: Section, id: u64) {
    match section {
        Section::Notes => notes::execute_delete(app, fx, id),
        Section::Calendar => calendar::execute_delete(app, fx, id),
        Section::Files => files::execute_delete(app, fx, id),
        Section::Contacts => contacts::execute_delete(app, fx, id),
        Section::Todo | Section::Photos => {}
    }
}

fn escape(app: &mut App, fx: &mut Vec<Effect>) {
    match app.focus {
        Focus::Help => app.focus = Focus::List,
        Focus::Palette => {
            app.palette = Default::default();
            app.focus = Focus::List;
        }
        Focus::Confirm => {
            app.confirm = None;
            app.focus = Focus::List;
        }
        Focus::Form => {
            app.form = None;
            app.focus = Focus::List;
        }
        // Leave the prompt but keep the filter; a second Escape clears it.
        Focus::Input(InputKind::Filter) => app.focus = Focus::List,
        Focus::Input(_) => {
            app.input = Input::default();
            app.focus = Focus::List;
        }
        Focus::Editor => {
            notes::flush_editor(app, fx);
            app.focus = Focus::List;
        }
        Focus::Detail => app.focus = Focus::List,
        Focus::List => {
            if !app.filter.is_empty() {
                app.filter.clear();
                reset_sel(app);
            }
        }
    }
}

/// Reset the current section's selection to the top (after the filter changes).
pub(crate) fn reset_sel(app: &mut App) {
    match app.section {
        Section::Notes => app.notes.sel = 0,
        Section::Todo => app.todo.sel = 0,
        Section::Calendar => app.calendar.sel = 0,
        Section::Photos => app.photos.sel = 0,
        Section::Files => app.files.sel = 0,
        Section::Contacts => app.contacts.sel = 0,
    }
}

fn submit(app: &mut App, now: Now, fx: &mut Vec<Effect>) {
    match app.focus {
        Focus::Input(kind) => submit_input(app, fx, kind),
        Focus::Form => {
            let last = app
                .form
                .as_ref()
                .is_none_or(|f| f.active + 1 >= f.fields.len());
            if last {
                submit_form(app, now, fx);
            } else {
                move_field(app, 1);
            }
        }
        Focus::Palette => {
            if let Some(msg) = palette::selected_action(app) {
                app.palette = Default::default();
                app.focus = Focus::List;
                dispatch(app, msg, now, fx);
            } else {
                app.info("No matches");
            }
        }
        _ => {}
    }
}

fn submit_input(app: &mut App, fx: &mut Vec<Effect>, kind: InputKind) {
    match kind {
        InputKind::Filter => {
            app.focus = Focus::List;
            reset_sel(app);
        }
        InputKind::TaskText => todo::submit_add(app, fx),
        InputKind::NewDir => files::submit_new_dir(app, fx),
        InputKind::RenameNote => notes::submit_rename(app, fx),
        InputKind::RenameFile => files::submit_rename(app, fx),
        InputKind::MoveFile => files::submit_move(app, fx),
        InputKind::Login => {
            let code = std::mem::take(&mut app.input.text);
            app.input = Input::default();
            app.focus = Focus::List;
            app.info("Logging in");
            fx.push(Effect::Login {
                code: code.trim().to_string(),
            });
        }
    }
}

fn submit_form(app: &mut App, now: Now, fx: &mut Vec<Effect>) {
    let is_event = matches!(
        app.form.as_ref().map(|f| &f.kind),
        Some(crate::state::FormKind::NewEvent | crate::state::FormKind::EditEvent { .. })
    );
    if is_event {
        calendar::submit_form(app, now, fx);
    } else {
        contacts::submit_form(app, fx);
    }
}

fn move_field(app: &mut App, delta: i32) {
    if let Some(form) = app.form.as_mut() {
        let len = form.fields.len() as i32;
        if len > 0 {
            form.active = (form.active as i32 + delta).rem_euclid(len) as usize;
        }
    }
}

fn text_edit(app: &mut App, edit: TextEdit) {
    let buf: &mut String = match app.focus {
        Focus::Input(InputKind::Filter) => {
            reset_sel(app);
            &mut app.filter
        }
        Focus::Input(_) => &mut app.input.text,
        Focus::Palette => {
            app.palette.sel = 0;
            &mut app.palette.query
        }
        Focus::Form => match app.form.as_mut() {
            Some(form) if !form.fields.is_empty() => {
                let i = form.active.min(form.fields.len() - 1);
                &mut form.fields[i].value
            }
            _ => return,
        },
        _ => return,
    };
    match edit {
        TextEdit::Insert(s) => buf.extend(s.chars().filter(|c| !c.is_control())),
        TextEdit::Backspace => {
            buf.pop();
        }
        TextEdit::Set(s) => {
            buf.clear();
            buf.extend(s.chars().filter(|c| !c.is_control()));
        }
    }
}

fn confirm_answer(app: &mut App, now: Now, fx: &mut Vec<Effect>, yes: bool) {
    if app.focus != Focus::Confirm {
        return;
    }
    app.focus = Focus::List;
    let Some(confirm) = app.confirm.take() else {
        return;
    };
    if yes {
        dispatch(app, confirm.on_yes, now, fx);
    } else {
        app.info("Cancelled");
    }
}

/// Open `section` and select the item with `id` (used by the palette).
fn goto(app: &mut App, fx: &mut Vec<Effect>, section: Section, id: u64) {
    switch_section(app, fx, section);
    match section {
        Section::Notes => notes::select_and_open(app, fx, id),
        Section::Files => files::select_file(app, id),
        Section::Contacts => contacts::select(app, id),
        _ => {}
    }
}
