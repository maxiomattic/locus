//! The pure transition function. [`update`] is the only entry point that
//! changes [`App`]; it dispatches to per-section modules and returns the IO
//! the shell must perform.

use crate::msg::{Effect, Msg, Nav, Now, TextEdit};
use crate::state::{App, Focus, Input, InputKind, NbKey, Section, Target, clamp_sel};
use crate::{calendar, contacts, files, media, messages, notes, palette, photos, todo};

/// Apply one message. Returns effects in the order the shell should run them.
pub fn update(app: &mut App, msg: Msg, now: Now) -> Vec<Effect> {
    if !app.clock_seen {
        app.clock_seen = true;
        app.calendar.day = now.today;
    }
    app.now = now;
    let mut fx = Vec::new();
    // A load that finishes may complete a cross-link that was waiting for it.
    let settles = matches!(
        msg,
        Msg::NotesLoaded { .. }
            | Msg::FilesLoaded { .. }
            | Msg::ContactsLoaded { .. }
            | Msg::EventsLoaded { .. }
            | Msg::PhotosLoaded { .. }
            | Msg::ConversationsLoaded { .. }
    );
    dispatch(app, msg, now, &mut fx);
    if settles && let Some(target) = app.pending_target.take() {
        let here = target.section().is_none_or(|s| s == app.section);
        if here {
            apply_target(app, &mut fx, target);
        }
    }
    fx
}

fn dispatch(app: &mut App, msg: Msg, now: Now, fx: &mut Vec<Effect>) {
    match msg {
        // ----- intents -----
        Msg::Batch(msgs) => {
            for m in msgs {
                dispatch(app, m, now, fx);
            }
        }
        Msg::Switch(s) => open(app, fx, s, None),
        Msg::Open { section, target } => open(app, fx, section, target),
        Msg::Reload => {
            let s = app.section;
            app.info(format!("Reloading {}", s.label()));
            load_section(app, s, fx);
        }
        Msg::Nav(nav) => nav_key(app, fx, nav),
        Msg::Activate => activate(app, fx),
        Msg::Edit => edit(app, fx),
        Msg::New => new_item(app, fx),
        Msg::NewFolder => notes::begin_new_folder(app),
        Msg::NewNotebook => notes::begin_new_notebook(app),
        Msg::Delete => delete(app),
        Msg::Rename => rename(app),
        Msg::Yank => files::yank(app, fx),
        Msg::Upload => upload(app, fx),
        Msg::Move => match app.section {
            Section::Files => files::begin_move(app),
            Section::Notes => notes::begin_move(app),
            Section::Photos => photos::begin_move(app),
            Section::Movies | Section::Music => match media::selected(app, app.section) {
                Some(file) => files::begin_move_file(app, &file),
                None => app.info("Nothing selected"),
            },
            _ => app.info("Nothing to move here"),
        },
        Msg::History => match app.section {
            Section::Notes => notes::toggle_history(app, fx),
            _ => app.info("History is available in Notes"),
        },
        Msg::SelectRevision(i) => notes::select_revision(app, fx, i),
        Msg::RestoreRevision => notes::ask_restore(app),
        Msg::ExecuteRestore { note, rev } => notes::execute_restore(app, fx, &note, rev),
        Msg::ExecuteMoveNote { note, to, folder } => {
            notes::execute_move_note(app, fx, &note, &to, folder);
        }
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
        Msg::OpenPalette => palette::open(app, fx),
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
        Msg::TogglePlay => media::toggle(app, fx),
        Msg::NextTrack => media::next(app, fx),
        Msg::PrevTrack => media::previous(app, fx),
        Msg::SeekTo(ms) => media::seek(app, fx, ms),
        Msg::SetVolume(v) => media::set_volume(app, fx, v),
        Msg::StopPlayback => media::stop(app, fx),
        Msg::PlayerTick {
            position_ms,
            duration_ms,
            playing,
        } => media::tick(app, position_ms, duration_ms, playing),
        Msg::PlayerEnded => media::ended(app, fx),
        Msg::PlayerFailed(error) => media::failed(app, error),
        Msg::BeginDraft => messages::begin_draft(app),
        Msg::SendDraft => messages::send(app, fx),
        Msg::Poll => messages::poll(app, fx),
        Msg::ConversationsLoaded { conversations } => {
            messages::conversations_loaded(app, fx, conversations);
        }
        Msg::ThreadLoaded { peer, messages } => messages::thread_loaded(app, peer, messages),
        Msg::MessageSent { peer } => messages::message_sent(app, fx, peer),
        Msg::MessageDeleted { id } => messages::message_deleted(app, fx, id),
        Msg::SendFailed { peer, body, error } => messages::send_failed(app, peer, body, error),
        Msg::ExecuteDeleteMessage { id } => messages::execute_delete(app, fx, id),
        Msg::SetBlocked { peer, blocked } => messages::set_blocked(fx, peer, blocked),
        Msg::BlockChanged { peer, blocked } => messages::block_changed(app, fx, peer, blocked),
        Msg::DirMoved { from, to } => {
            files::dir_moved(app, &from, &to);
            load_section(app, Section::Files, fx);
        }
        Msg::Goto { section, id } => open(app, fx, section, Target::item(section, id)),
        Msg::Select(i) if notes::picking(app) => notes::move_select(app, i),
        Msg::Select(i) if app.section == Section::Messages => {
            if app.focus == Focus::Detail {
                app.messages.thread_sel = clamp_sel(i, app.messages.thread.len());
            } else {
                messages::select(app, fx, i);
            }
        }
        Msg::Select(i) => {
            // Picking a row (a click) while typing in the editor moves focus
            // to the list, so list keys work again.
            if app.focus == Focus::Editor {
                notes::flush_editor(app, fx);
                app.focus = Focus::List;
            }
            // Likewise away from the note history's revision list.
            if app.focus == Focus::Detail && app.section == Section::Notes {
                app.focus = Focus::List;
            }
            select_index(app, i);
            if app.section == Section::Notes {
                notes::close_history_unless_selected(app);
            }
        }
        Msg::EditorFocus => {
            if app.editor.is_some() {
                app.focus = Focus::Editor;
            }
        }
        Msg::SelectDay(day) => {
            app.calendar.day = day;
            app.calendar.sel = 0;
        }
        Msg::ExecuteDelete { section, id } => execute_delete(app, fx, section, id),
        Msg::ExecuteDeleteNote(note) => notes::execute_delete(app, fx, note),
        Msg::ExecuteDeleteFolder { nb, id } => notes::execute_delete_folder(app, fx, &nb, id),
        Msg::ExecuteDeleteNotebook(nb) => notes::execute_delete_notebook(app, fx, &nb),
        Msg::ExecuteDeleteDir { path } => files::execute_delete_dir(app, fx, path),
        Msg::BeginLogin => {
            app.input = Input::default();
            app.focus = Focus::Input(InputKind::Login);
        }

        // ----- IO results -----
        Msg::NotebooksLoaded { notebooks } => notes::notebooks_loaded(app, fx, notebooks),
        Msg::NotebookCreated { notebook } => notes::notebook_created(app, fx, notebook),
        Msg::NotesLoaded { nb, notes } => notes::notes_loaded(app, fx, nb, notes),
        Msg::FoldersLoaded { nb, folders } => notes::folders_loaded(app, nb, folders),
        Msg::NoteFolderCreated { nb, folders } => {
            notes::folders_loaded(app, nb, folders);
            app.info("Created folder");
        }
        Msg::NoteFolderUpdated { nb, folders, moved } => {
            notes::folders_loaded(app, nb, folders);
            app.info(if moved { "Moved folder" } else { "Renamed folder" });
        }
        Msg::NoteLoaded { nb, note } => notes::note_loaded(app, fx, nb, note),
        Msg::NoteCreated { nb, note } => notes::note_created(app, fx, nb, note),
        Msg::NoteSaved { nb, note } => notes::note_saved(app, fx, nb, note),
        Msg::NoteRenamed { nb, id, title } => notes::note_renamed(app, nb, id, title),
        Msg::NoteDeleted { nb, id } => notes::note_deleted(app, nb, id),
        Msg::NoteMoved { nb, id, folder } => notes::note_moved(app, nb, id, folder),
        Msg::NoteMovedAcross {
            from,
            id,
            to,
            note,
            deleted,
        } => notes::note_moved_across(app, fx, from, id, to, *note, deleted),
        Msg::NoteMoveFailed { error } => notes::move_failed(app, error),
        Msg::NoteFolderDeleted { nb, id } => notes::folder_deleted(app, fx, nb, id),
        Msg::NotebookDeleted { nb } => notes::notebook_deleted(app, nb),
        Msg::NotebookRenamed { nb, title } => notes::notebook_renamed(app, &nb, title),
        Msg::NoteRestored { nb, note, rev } => notes::note_restored(app, fx, nb, note, rev),
        Msg::HistoryLoaded { nb, id, revs } => notes::history_loaded(app, nb, id, revs),
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
        Msg::PhotoDeleted { id } => photos::deleted(app, id),
        Msg::PhotoPatched { photo } => photos::patched(app, photo),
        Msg::FilesLoaded { files } => files::files_loaded(app, files),
        Msg::DirsLoaded { dirs } => files::dirs_loaded(app, dirs),
        Msg::DirCreated { path } => files::dir_created(app, path),
        Msg::DirDeleted { path } => files::dir_deleted(app, &path),
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
            app.loaded = [false; Section::COUNT];
            app.info("Logged in");
            let s = app.section;
            load_section(app, s, fx);
        }
        Msg::LoginFailed { error } => app.error(format!("Login failed: {error}")),
        Msg::LoadFailed { section, error } => {
            app.pending_target = None;
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
        Section::Files | Section::Movies | Section::Music => {
            vec![Section::Files, Section::Movies, Section::Music]
        }
        s => vec![s],
    }
}

pub(crate) fn switch_section(app: &mut App, fx: &mut Vec<Effect>, section: Section) {
    notes::flush_editor(app, fx);
    if app.section != section {
        // A cross-link still waiting for data is dropped once the user
        // moves on; a switch to the same section (at startup) keeps it.
        app.pending_target = None;
    }
    if section != Section::Notes {
        notes::close_history(app);
    }
    app.section = section;
    app.filter.clear();
    app.focus = Focus::List;
    app.confirm = None;
    // An open form belongs to the section being left; leaving cancels it,
    // like Escape, rather than leaving a card that no longer takes input.
    app.form = None;
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
        // The notebook list first; each notebook's folders and notes follow.
        Section::Notes | Section::Todo => fx.push(Effect::LoadNotebooks),
        Section::Calendar => fx.push(Effect::LoadEvents),
        Section::Photos => fx.push(Effect::LoadPhotos),
        Section::Files | Section::Movies | Section::Music => {
            fx.push(Effect::LoadFiles);
            fx.push(Effect::LoadDirs);
        }
        Section::Contacts => fx.push(Effect::LoadContacts),
        Section::Messages => fx.push(Effect::LoadConversations),
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

fn nav_key(app: &mut App, fx: &mut Vec<Effect>, nav: Nav) {
    match app.focus {
        Focus::Palette => {
            palette::move_sel(app, nav);
            return;
        }
        Focus::Input(InputKind::MoveNote | InputKind::MoveNoteFolder) => {
            notes::move_nav(app, nav);
            return;
        }
        Focus::List | Focus::Detail => {}
        _ => return,
    }
    match app.section {
        Section::Notes => notes::nav(app, fx, nav),
        Section::Todo => {
            let len = todo::visible(app).len();
            app.todo.sel = step(app.todo.sel, len, nav);
        }
        Section::Calendar => calendar::nav(app, nav),
        Section::Photos => photos::nav(app, fx, nav),
        Section::Files => files::nav(app, nav),
        Section::Contacts => {
            let len = contacts::visible(app).len();
            app.contacts.sel = step(app.contacts.sel, len, nav);
        }
        Section::Movies | Section::Music => media::nav(app, app.section, nav),
        Section::Messages => {
            if app.focus == Focus::Detail {
                messages::thread_nav(app, nav);
            } else {
                messages::nav(app, fx, nav);
            }
        }
    }
}

fn activate(app: &mut App, fx: &mut Vec<Effect>) {
    match app.section {
        Section::Notes => notes::open_selected(app, fx),
        Section::Todo => todo::jump_to_source(app, fx),
        Section::Calendar => calendar::edit_selected(app),
        Section::Photos => photos::open_viewer(app, fx),
        Section::Files => files::activate(app, fx),
        Section::Contacts => contacts::open_detail(app),
        Section::Movies => media::activate(app, fx, Section::Movies),
        Section::Music => media::activate(app, fx, Section::Music),
        Section::Messages => messages::activate(app, fx),
    }
}

fn edit(app: &mut App, fx: &mut Vec<Effect>) {
    match app.section {
        Section::Notes => notes::edit_selected(app, fx),
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
        Section::Photos => app.info("Use Upload to add photos"),
        Section::Movies | Section::Music => app.info("Use Upload to add files"),
        Section::Messages => messages::begin_new(app),
    }
}

fn delete(app: &mut App) {
    match app.section {
        Section::Notes => notes::ask_delete(app),
        Section::Calendar => calendar::ask_delete(app),
        Section::Files => files::ask_delete(app),
        Section::Contacts => contacts::ask_delete(app),
        Section::Photos => photos::ask_delete(app),
        Section::Movies | Section::Music => match media::selected(app, app.section) {
            Some(file) => files::ask_delete_file(app, &file),
            None => app.info("Nothing selected"),
        },
        Section::Messages => messages::ask_delete(app),
        _ => app.info("Nothing to delete here"),
    }
}

fn rename(app: &mut App) {
    match app.section {
        Section::Notes => notes::begin_rename(app),
        Section::Files => files::begin_rename(app),
        Section::Photos => photos::begin_rename(app),
        Section::Movies | Section::Music => match media::selected(app, app.section) {
            Some(file) => files::begin_rename_file(app, &file),
            None => app.info("Nothing selected"),
        },
        _ => app.info("Nothing to rename here"),
    }
}

fn upload(app: &mut App, fx: &mut Vec<Effect>) {
    let dir = match app.section {
        Section::Photos => "/Photos".to_string(),
        Section::Files => app.files.cwd.clone(),
        Section::Movies => media::MOVIES_DIR.to_string(),
        Section::Music => media::MUSIC_DIR.to_string(),
        _ => {
            app.info("Upload is available in Photos, Files, Movies and Music");
            return;
        }
    };
    app.info(format!("Choose files to upload to {dir}"));
    fx.push(Effect::UploadFiles { dir });
}

fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, section: Section, id: u64) {
    match section {
        Section::Notes | Section::Todo | Section::Messages => {}
        Section::Calendar => calendar::execute_delete(app, fx, id),
        Section::Files | Section::Movies | Section::Music => files::execute_delete(app, fx, id),
        Section::Contacts => contacts::execute_delete(app, fx, id),
        Section::Photos => photos::execute_delete(app, fx, id),
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
            app.focus = after_confirm(app);
        }
        Focus::Form => {
            app.form = None;
            app.focus = Focus::List;
        }
        // Leave the prompt but keep the filter; a second Escape clears it.
        Focus::Input(InputKind::Filter) => app.focus = Focus::List,
        Focus::Input(InputKind::MessageDraft) => app.focus = Focus::List,
        Focus::Input(_) => {
            app.input = Input::default();
            app.files.dir_edit = None;
            app.notes.folder_edit = None;
            app.notes.rename = None;
            app.focus = Focus::List;
        }
        Focus::Editor => {
            notes::flush_editor(app, fx);
            app.focus = Focus::List;
        }
        Focus::Detail if app.section == Section::Notes => {
            notes::close_history(app);
            app.focus = Focus::List;
        }
        Focus::Detail => app.focus = Focus::List,
        Focus::List => {
            if !app.filter.is_empty() {
                app.filter.clear();
                reset_sel(app);
            } else if app.section == Section::Notes {
                notes::close_history(app);
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
        Section::Movies | Section::Music => media::reset_sel(app),
        Section::Messages => app.messages.sel = 0,
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
        InputKind::NewNoteFolder => notes::submit_new_folder(app, fx),
        InputKind::NewNotebook => notes::submit_new_notebook(app, fx),
        InputKind::RenameNote => notes::submit_rename(app, fx),
        InputKind::RenameFile => files::submit_rename(app, fx),
        InputKind::MoveFile => files::submit_move(app, fx),
        InputKind::RenamePhoto => photos::submit_rename(app, fx),
        InputKind::MovePhoto => photos::submit_move(app, fx),
        InputKind::RenameDir => files::submit_rename_dir(app, fx),
        InputKind::MoveDir => files::submit_move_dir(app, fx),
        InputKind::MessageDraft => messages::send(app, fx),
        InputKind::NewPeer => messages::submit_new_peer(app, fx),
        InputKind::MoveNote | InputKind::MoveNoteFolder => notes::submit_move(app, fx),
        InputKind::RenameNoteFolder => notes::submit_folder_rename(app, fx),
        InputKind::RenameNotebook => notes::submit_notebook_rename(app, fx),
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
        Focus::Input(InputKind::MessageDraft) => &mut app.messages.draft,
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
    if notes::picking(app) {
        notes::reset_move_sel(app);
    }
}

fn confirm_answer(app: &mut App, now: Now, fx: &mut Vec<Effect>, yes: bool) {
    if app.focus != Focus::Confirm {
        return;
    }
    app.focus = after_confirm(app);
    let Some(confirm) = app.confirm.take() else {
        return;
    };
    if yes {
        dispatch(app, confirm.on_yes, now, fx);
    } else {
        app.info("Cancelled");
    }
}

/// Where focus goes when a confirm prompt closes: back to the revision
/// list if the note history is open, else the list.
fn after_confirm(app: &App) -> Focus {
    if app.section == Section::Notes && app.notes.history.is_some() {
        Focus::Detail
    } else {
        Focus::List
    }
}

/// Open `section`, then `target` in it. A window locked to another section
/// hands both to the shell, which opens or focuses that section's window.
pub(crate) fn open(app: &mut App, fx: &mut Vec<Effect>, section: Section, target: Option<Target>) {
    if let Some(locked) = app.locked
        && locked.app() != section.app()
    {
        app.info(format!("Opening {}", section.label()));
        fx.push(Effect::OpenSection(section, target));
        return;
    }
    switch_section(app, fx, section);
    if let Some(target) = target {
        app.pending_target = None;
        apply_target(app, fx, target);
    }
}

/// Show `target` in the current section. When the data it needs has not
/// arrived yet, it waits in `pending_target` until the section's load ends.
fn apply_target(app: &mut App, fx: &mut Vec<Effect>, target: Target) {
    if target.section().is_some_and(|s| s != app.section) {
        return;
    }
    let waiting = |app: &App, s: Section| !app.loaded[s.index()] || app.is_loading(s);
    match target.clone() {
        Target::Note(id) => {
            // A bare id means a note of the default notebook.
            let key = notes::default_notebook(app).map(NbKey::of);
            open_note(app, fx, key, id, &target, waiting(app, Section::Notes));
        }
        Target::NoteAt(r) => {
            open_note(
                app,
                fx,
                Some(r.nb.clone()),
                r.id,
                &target,
                waiting(app, Section::Notes),
            );
        }
        Target::File(id) => {
            if app.files.files.iter().any(|f| f.id == id) || !waiting(app, Section::Files) {
                files::select_file(app, id);
            } else {
                app.pending_target = Some(target);
            }
        }
        Target::Contact(id) => {
            if app.contacts.items.iter().any(|c| c.id == id) {
                contacts::select(app, id);
            } else if waiting(app, Section::Contacts) {
                app.pending_target = Some(target);
            } else {
                app.info("Contact not found");
            }
        }
        Target::New => {
            // A new note needs the notebook, which arrives with the notes.
            if matches!(app.section, Section::Notes | Section::Todo)
                && notes::default_notebook(app).is_none()
            {
                app.pending_target = Some(target);
            } else {
                new_item(app, fx);
            }
        }
        Target::Upload => upload(app, fx),
    }
}

/// Show a note by link, or keep the link waiting while notes are loading.
fn open_note(
    app: &mut App,
    fx: &mut Vec<Effect>,
    key: Option<NbKey>,
    id: u64,
    target: &Target,
    waiting: bool,
) {
    match key {
        Some(key) if notes::find(app, &key, id).is_some() => notes::goto_note(app, fx, &key, id),
        _ if waiting => app.pending_target = Some(target.clone()),
        _ => app.info("Note not found"),
    }
}

/// Set the current section's cursor to `i`, clamped to the rows it shows.
fn select_index(app: &mut App, i: usize) {
    if app.focus == Focus::Palette {
        let len = palette::ranked(app).len();
        app.palette.sel = clamp_sel(i, len);
        return;
    }
    match app.section {
        Section::Notes => {
            let len = notes::len(app);
            app.notes.sel = clamp_sel(i, len);
        }
        Section::Todo => {
            let len = todo::visible(app).len();
            app.todo.sel = clamp_sel(i, len);
        }
        Section::Calendar => {
            let len = calendar::events_on(app, app.calendar.day).len();
            app.calendar.sel = clamp_sel(i, len);
        }
        Section::Photos => {
            let len = photos::visible(app).len();
            app.photos.sel = clamp_sel(i, len);
        }
        Section::Files => {
            let len = files::rows(app).len();
            app.files.sel = clamp_sel(i, len);
        }
        Section::Contacts => {
            let len = contacts::visible(app).len();
            app.contacts.sel = clamp_sel(i, len);
        }
        Section::Movies | Section::Music => media::select(app, app.section, i),
        Section::Messages => app.messages.sel = clamp_sel(i, app.messages.conversations.len()),
    }
}
