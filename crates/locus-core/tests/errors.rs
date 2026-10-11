mod common;

use common::*;
use locus_core::{Effect, Focus, Level, Msg, Section, view_model};

#[test]
fn load_failure_clears_loading_and_reports_error() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(Section::Contacts));
    assert!(app.is_loading(Section::Contacts));
    send(
        &mut app,
        Msg::LoadFailed {
            section: Section::Contacts,
            error: "HTTP 401".into(),
        },
    );
    assert!(!app.is_loading(Section::Contacts));
    assert_eq!(app.status.level, Level::Error);
    assert!(app.status.text.contains("HTTP 401"));
    // The next visit tries again.
    assert_eq!(send(&mut app, Msg::Switch(Section::Notes)).len(), 1);
    assert_eq!(
        send(&mut app, Msg::Switch(Section::Contacts)),
        vec![Effect::LoadContacts]
    );
}

#[test]
fn notes_failure_clears_both_notes_and_todo_loading() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Reload);
    send(
        &mut app,
        Msg::LoadFailed {
            section: Section::Todo,
            error: "offline".into(),
        },
    );
    assert!(!app.is_loading(Section::Notes));
    assert!(!app.is_loading(Section::Todo));
}

#[test]
fn write_failures_set_error_status_for_their_section() {
    let mut app = loaded_notes(vec![]);
    send(
        &mut app,
        Msg::WriteFailed {
            section: Section::Files,
            error: "conflict".into(),
        },
    );
    assert_eq!(app.status.level, Level::Error);
    assert_eq!(app.status.text, "Files failed: conflict");
}

#[test]
fn login_flow_sends_the_trimmed_code_and_reloads() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::BeginLogin);
    assert!(matches!(app.focus, Focus::Input(_)));
    let view = view_model::input_prompt(&app).unwrap();
    assert!(view.masked, "the login code must not be echoed");
    type_str(&mut app, "  lidlut-tabwed  ");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::Login {
            code: "lidlut-tabwed".into()
        }]
    );
    assert!(app.input.text.is_empty(), "the code is not kept in state");
    let fx = send(&mut app, Msg::LoggedIn);
    assert_eq!(fx, vec![Effect::LoadNotebooks]);
}

#[test]
fn login_failure_is_reported() {
    let mut app = locus_core::App::new();
    send(
        &mut app,
        Msg::LoginFailed {
            error: "bad code".into(),
        },
    );
    assert_eq!(app.status.level, Level::Error);
}

#[test]
fn upload_cancelled_is_not_an_error() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Switch(Section::Photos));
    let fx = send(
        &mut app,
        Msg::UploadDone {
            dir: "/Photos".into(),
            count: 0,
        },
    );
    assert!(fx.is_empty());
    assert_eq!(app.status.level, Level::Info);
}

#[test]
fn status_line_shows_section_status_loading_and_filter() {
    let mut app = locus_core::App::new();
    send(&mut app, Msg::Reload);
    let line = view_model::status_line(&app);
    assert!(line.starts_with("Notes |"));
    assert!(line.ends_with("loading..."));
    app.filter = "x".into();
    assert!(view_model::status_line(&app).contains("filter: x"));
}

#[test]
fn sidebar_marks_only_the_active_section() {
    let app = loaded_notes(vec![]);
    let items = view_model::sidebar(&app);
    assert_eq!(items.len(), 6);
    assert_eq!(items.iter().filter(|i| i.active).count(), 1);
    assert!(items[0].active);
    assert_eq!(items[5].shortcut, "g o");
}

#[test]
fn a_flood_of_unknown_messages_leaves_state_valid() {
    let mut app = loaded_notes(vec![note(1, "a", "- [ ] t", 1, 1)]);
    send(&mut app, Msg::Nav(locus_core::Nav::Bottom));
    send(&mut app, Msg::ToggleTask);
    send(&mut app, Msg::Escape);
    send(&mut app, Msg::Submit);
    send(&mut app, Msg::NextField);
    send(
        &mut app,
        Msg::Text(locus_core::TextEdit::Insert("zz".into())),
    );
    send(&mut app, Msg::Text(locus_core::TextEdit::Backspace));
    send(&mut app, Msg::PhotosLoaded { photos: vec![] });
    send(
        &mut app,
        Msg::Goto {
            section: Section::Files,
            id: 99,
        },
    );
    send(
        &mut app,
        Msg::Goto {
            section: Section::Contacts,
            id: 99,
        },
    );
    send(&mut app, Msg::Batch(vec![Msg::Escape, Msg::Escape]));
    assert_eq!(app.focus, Focus::List);
    assert!(app.form.is_none());
    assert_eq!(app.notes.items.len(), 1, "no data was dropped");
}
