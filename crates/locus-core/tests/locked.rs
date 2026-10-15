//! One codebase, many entry points: a window locked to one section opens
//! other sections in their own windows (`Effect::OpenSection`), carries
//! cross-links as `Target`s, and opens incoming targets once data arrives.

mod common;

use common::*;
use locus_core::locus_client::notes::Note;
use locus_core::{Focus, Target, palette, view_model};

const PLAN: &str = "# Plan\n- [ ] write report\n- [ ] buy bread\n";

/// A window locked to `section` whose notes are loaded.
fn locked(section: Section, notes: Vec<Note>) -> App {
    let mut app = App::new();
    app.focus_section(Some(section));
    send(&mut app, Msg::Switch(section));
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb()],
        },
    );
    send(&mut app, Msg::NotesLoaded { nb: nb(), notes });
    app
}

#[test]
fn focus_section_locks_and_starts_on_the_section() {
    let mut app = App::new();
    assert_eq!(app.locked, None);
    app.focus_section(Some(Section::Calendar));
    assert_eq!(app.locked, Some(Section::Calendar));
    assert_eq!(app.section, Section::Calendar);
    app.focus_section(None);
    assert_eq!(app.locked, None);
}

#[test]
fn enter_on_a_task_in_locked_todo_opens_the_note_in_the_notes_app() {
    let mut app = locked(Section::Todo, vec![note(1, "Plan", PLAN, 1, 1)]);
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::OpenSection(Section::Notes, Some(note_target(1)))]
    );
    assert_eq!(app.section, Section::Todo);
    assert!(app.editor.is_none());
}

#[test]
fn enter_on_a_task_unlocked_still_opens_the_note_in_place() {
    let mut app = loaded_notes(vec![note(1, "Plan", PLAN, 1, 1)]);
    send(&mut app, Msg::Switch(Section::Todo));
    let fx = press(&mut app, Key::Enter);
    assert!(!has_effect(&fx, |e| matches!(e, Effect::OpenSection(..))));
    assert_eq!(app.section, Section::Notes);
    assert_eq!(app.editor.as_ref().map(|e| e.note_id), Some(1));
}

#[test]
fn an_incoming_note_target_waits_for_the_notes_then_opens() {
    let mut app = App::new();
    app.focus_section(Some(Section::Notes));
    app.pending_target = Some(Target::Note(8));
    send(&mut app, Msg::Switch(Section::Notes));
    assert_eq!(
        app.pending_target,
        Some(Target::Note(8)),
        "a plain switch at startup keeps the cross-link"
    );
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb()],
        },
    );
    assert!(app.editor.is_none());
    send(
        &mut app,
        Msg::NotesLoaded {
            nb: nb(),
            notes: vec![note(7, "A", "a", 1, 1), note(8, "B", "b", 1, 2)],
        },
    );
    assert_eq!(app.editor.as_ref().map(|e| e.note_id), Some(8));
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(app.pending_target, None);
}

#[test]
fn open_with_a_target_in_a_loaded_window_applies_at_once() {
    let mut app = locked(
        Section::Notes,
        vec![note(7, "A", "a", 1, 1), note(8, "B", "b", 1, 2)],
    );
    send(
        &mut app,
        Msg::Open {
            section: Section::Notes,
            target: Some(Target::Note(7)),
        },
    );
    assert_eq!(app.editor.as_ref().map(|e| e.note_id), Some(7));

    send(&mut app, Msg::Escape);
    send(
        &mut app,
        Msg::Open {
            section: Section::Notes,
            target: Some(Target::Note(99)),
        },
    );
    assert_eq!(app.status.text, "Note not found");
}

#[test]
fn incoming_file_and_contact_targets_select_after_load() {
    let mut app = App::new();
    app.focus_section(Some(Section::Files));
    send(
        &mut app,
        Msg::Open {
            section: Section::Files,
            target: Some(Target::File(3)),
        },
    );
    assert_eq!(app.pending_target, Some(Target::File(3)));
    send(&mut app, Msg::DirsLoaded { dirs: vec![] });
    send(
        &mut app,
        Msg::FilesLoaded {
            files: vec![file(2, "/", "a.txt", 1), file(3, "/Docs", "b.txt", 1)],
        },
    );
    assert_eq!(app.files.cwd, "/Docs");
    assert_eq!(view_model::file_rows(&app)[app.files.sel].name, "b.txt");

    let mut app = App::new();
    app.focus_section(Some(Section::Contacts));
    app.pending_target = Some(Target::Contact(5));
    send(&mut app, Msg::Switch(Section::Contacts));
    send(
        &mut app,
        Msg::ContactsLoaded {
            contacts: vec![
                contact(4, "Ann", "a@x", "~zod"),
                contact(5, "Bob", "b@x", "~bus"),
            ],
        },
    );
    assert_eq!(app.contacts.sel, 1);
}

#[test]
fn a_new_target_starts_a_new_item() {
    let mut app = App::new();
    app.focus_section(Some(Section::Calendar));
    send(
        &mut app,
        Msg::Open {
            section: Section::Calendar,
            target: Some(Target::New),
        },
    );
    assert_eq!(app.focus, Focus::Form);

    // A new note waits for the notebook.
    let mut app = App::new();
    app.focus_section(Some(Section::Notes));
    app.pending_target = Some(Target::New);
    send(&mut app, Msg::Switch(Section::Notes));
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb()],
        },
    );
    let fx = send(
        &mut app,
        Msg::NotesLoaded {
            nb: nb(),
            notes: vec![],
        },
    );
    assert!(has_effect(&fx, |e| matches!(e, Effect::CreateNote { .. })));
}

#[test]
fn a_target_for_another_section_is_dropped_once_the_user_moves_on() {
    let mut app = loaded_notes(vec![]);
    send(&mut app, Msg::Switch(Section::Contacts));
    send(
        &mut app,
        Msg::Open {
            section: Section::Contacts,
            target: Some(Target::Contact(1)),
        },
    );
    send(&mut app, Msg::Switch(Section::Calendar));
    assert_eq!(app.pending_target, None);
}

#[test]
fn palette_in_a_locked_window_fetches_what_it_searches() {
    let mut app = locked(Section::Calendar, vec![]);
    let fx = ctrl(&mut app, 'k');
    assert_eq!(app.focus, Focus::Palette);
    assert!(has_effect(&fx, |e| matches!(e, Effect::LoadFiles)));
    assert!(has_effect(&fx, |e| matches!(e, Effect::LoadContacts)));
    // The fixture already delivered the notes, so only the rest load.
    assert!(!has_effect(&fx, |e| matches!(e, Effect::LoadNotes { .. })));

    // The unlocked palette keeps its old behaviour: no fetches.
    let mut app = loaded_notes(vec![]);
    let fx = ctrl(&mut app, 'k');
    assert!(fx.is_empty());
}

#[test]
fn palette_hits_in_a_locked_window_open_in_their_own_app() {
    let mut app = locked(Section::Calendar, vec![note(7, "Reading list", "a", 1, 1)]);
    send(
        &mut app,
        Msg::ContactsLoaded {
            contacts: vec![contact(5, "Bob Jones", "b@x", "~bus")],
        },
    );

    ctrl(&mut app, 'k');
    type_str(&mut app, "reading");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::OpenSection(Section::Notes, Some(note_target(7)))]
    );
    assert_eq!(app.section, Section::Calendar);
    assert_eq!(app.focus, Focus::List);

    ctrl(&mut app, 'k');
    type_str(&mut app, "bob");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::OpenSection(
            Section::Contacts,
            Some(Target::Contact(5))
        )]
    );

    ctrl(&mut app, 'k');
    type_str(&mut app, "new note");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::OpenSection(Section::Notes, Some(Target::New))]
    );

    // A command for the window's own section runs in place.
    ctrl(&mut app, 'k');
    type_str(&mut app, "new event");
    let fx = press(&mut app, Key::Enter);
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Form);
}

#[test]
fn palette_marks_entries_that_open_another_app() {
    let mut app = locked(Section::Notes, vec![]);
    ctrl(&mut app, 'k');
    let items = palette::items(&app);
    assert_eq!(items[0].hint, "section", "Notes is this window");
    assert_eq!(items[1].hint, "app");
}

#[test]
fn a_locked_window_is_titled_after_its_app() {
    let app = loaded_notes(vec![]);
    assert_eq!(view_model::window_title(&app), "Locus");

    let app = locked(Section::Notes, vec![]);
    assert_eq!(view_model::window_title(&app), "Locus Notes");
}

#[test]
fn targets_and_slugs_round_trip() {
    for t in [
        Target::Note(12),
        note_target(12),
        Target::File(3),
        Target::Contact(9),
        Target::New,
        Target::Upload,
    ] {
        assert_eq!(Target::parse(&t.to_string()), Some(t));
    }
    assert_eq!(Target::parse("note:x"), None);
    assert_eq!(Target::parse("note:~zod/notes/x"), None);
    assert_eq!(Target::parse("note:~zod/12"), None);
    assert_eq!(Target::parse("note://12"), None);
    assert_eq!(Target::parse("note:~zod/a/b/12"), None);
    assert_eq!(note_target(12).to_string(), "note:~zod/notes/12");
    assert_eq!(Target::Note(12).to_string(), "note:12");
    assert_eq!(Target::parse("event:1"), None);
    for s in Section::ALL {
        assert_eq!(Section::from_slug(s.slug()), Some(s));
    }
    assert_eq!(Section::from_slug("Notes"), Some(Section::Notes));
    assert_eq!(Section::from_slug("mail"), None);
}
