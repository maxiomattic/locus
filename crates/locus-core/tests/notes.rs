mod common;

use common::*;
use locus_core::{Effect, Focus, Key, Msg, view_model};

fn open_first_note(app: &mut locus_core::App) {
    press(app, Key::Enter);
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn enter_opens_editor_and_fetches_body_when_not_loaded() {
    let mut app = loaded_notes(vec![note(5, "Draft", "", 3, 1)]);
    // The list did not carry bodies, so the body is fetched.
    let fx = press(&mut app, Key::Enter);
    assert!(fx.contains(&Effect::LoadNote { nb: nb(), id: 5 }));
    assert_eq!(app.focus, Focus::Editor);
    let ed = view_model::editor(&app).unwrap();
    assert_eq!(ed.title, "Draft");
    assert!(!ed.dirty);
}

#[test]
fn j_while_editing_is_text_not_list_navigation() {
    let mut app = loaded_notes(vec![note(1, "a", "x", 1, 2), note(2, "b", "y", 1, 1)]);
    open_first_note(&mut app);
    let fx = press(&mut app, Key::Char('j'));
    assert!(
        fx.is_empty(),
        "the shell types the j into its editor widget"
    );
    assert_eq!(app.notes.sel, 0, "the list did not move");
    press(&mut app, Key::Char('d'));
    assert_eq!(app.focus, Focus::Editor, "d is text, not delete");
    assert!(app.confirm.is_none());
}

#[test]
fn editor_changes_only_save_after_the_latest_debounce_token() {
    let mut app = loaded_notes(vec![note(1, "Doc", "a", 4, 1)]);
    open_first_note(&mut app);

    let fx1 = send(&mut app, Msg::EditorChanged("ab".into()));
    let fx2 = send(&mut app, Msg::EditorChanged("abc".into()));
    let t1 = match fx1.as_slice() {
        [
            Effect::ScheduleSave {
                after_ms: 600,
                token,
            },
        ] => *token,
        other => panic!("unexpected {other:?}"),
    };
    let t2 = match fx2.as_slice() {
        [
            Effect::ScheduleSave {
                after_ms: 600,
                token,
            },
        ] => *token,
        other => panic!("unexpected {other:?}"),
    };
    assert_ne!(t1, t2);

    // The first timer is stale.
    assert!(send(&mut app, Msg::SaveTimer(t1)).is_empty());
    // The latest timer saves the latest body against the loaded revision.
    let fx = send(&mut app, Msg::SaveTimer(t2));
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 1,
            body: "abc".into(),
            expected_revision: 4,
        }]
    );
}

#[test]
fn timer_after_no_real_change_saves_nothing() {
    let mut app = loaded_notes(vec![note(1, "Doc", "same", 1, 1)]);
    open_first_note(&mut app);
    // Same text as before: no change, no timer.
    assert!(send(&mut app, Msg::EditorChanged("same".into())).is_empty());
}

#[test]
fn ctrl_s_saves_immediately_and_saved_result_clears_dirty() {
    let mut app = loaded_notes(vec![note(1, "Doc", "a", 4, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::EditorChanged("new".into()));
    let fx = ctrl(&mut app, 's');
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 1,
            body: "new".into(),
            expected_revision: 4
        }]
    );
    assert!(view_model::editor(&app).unwrap().saving);

    let fx = send(
        &mut app,
        Msg::NoteSaved {
            nb: nb(),
            note: note(1, "Doc", "new", 5, 9),
        },
    );
    assert!(fx.is_empty());
    let ed = view_model::editor(&app).unwrap();
    assert!(!ed.dirty && !ed.saving);
    assert_eq!(app.status.text, "Saved");
}

#[test]
fn a_save_requested_during_flight_is_sent_after_it_lands() {
    let mut app = loaded_notes(vec![note(1, "Doc", "a", 1, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::EditorChanged("b".into()));
    let first = ctrl(&mut app, 's');
    assert_eq!(first.len(), 1);
    // Typed again while the first save is in flight, then Ctrl+S.
    send(&mut app, Msg::EditorChanged("bc".into()));
    assert!(ctrl(&mut app, 's').is_empty(), "queued, not sent twice");
    let fx = send(
        &mut app,
        Msg::NoteSaved {
            nb: nb(),
            note: note(1, "Doc", "b", 2, 9),
        },
    );
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 1,
            body: "bc".into(),
            expected_revision: 2
        }]
    );
}

#[test]
fn escape_leaves_editor_and_saves_when_dirty() {
    let mut app = loaded_notes(vec![note(1, "Doc", "a", 1, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::EditorChanged("edited".into()));
    let fx = press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 1,
            body: "edited".into(),
            expected_revision: 1
        }]
    );
}

#[test]
fn save_conflict_keeps_both_versions() {
    let mut app = loaded_notes(vec![note(1, "Plan", "mine", 4, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::EditorChanged("mine, edited".into()));
    let fx = ctrl(&mut app, 's');
    assert_eq!(fx.len(), 1);

    let fx = send(
        &mut app,
        Msg::SaveConflict {
            nb: nb(),
            id: 1,
            body: "mine, edited".into(),
            current: Some(6),
        },
    );
    assert_eq!(
        fx,
        vec![
            Effect::CreateNote {
                nb: nb(),
                folder: 10,
                title: "Plan (conflict copy)".into(),
                body: "mine, edited".into(),
            },
            Effect::LoadNote { nb: nb(), id: 1 },
        ]
    );
    assert_eq!(app.status.level, locus_core::Level::Warn);
    assert!(app.status.text.contains("Plan (conflict copy)"));

    // The user's text stays in the editor.
    let ed = view_model::editor(&app).unwrap();
    assert_eq!(ed.body, "mine, edited");
    assert!(ed.conflict);

    // The server copy comes back and becomes the new base.
    let fx = send(
        &mut app,
        Msg::NoteLoaded {
            nb: nb(),
            note: note(1, "Plan", "theirs", 6, 2),
        },
    );
    assert!(fx.is_empty());
    let ed = view_model::editor(&app).unwrap();
    assert_eq!(ed.body, "mine, edited", "the user's text is not replaced");
    assert!(
        ed.dirty,
        "it differs from the server copy, so it is unsaved"
    );
    assert!(!ed.conflict);

    // The next edit saves against the reloaded revision.
    let fx = send(&mut app, Msg::EditorChanged("mine, final".into()));
    let token = match fx.as_slice() {
        [Effect::ScheduleSave { token, .. }] => *token,
        other => panic!("{other:?}"),
    };
    let fx = send(&mut app, Msg::SaveTimer(token));
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 1,
            body: "mine, final".into(),
            expected_revision: 6
        }]
    );
}

#[test]
fn save_failure_reports_error_and_keeps_text() {
    let mut app = loaded_notes(vec![note(1, "Doc", "a", 1, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::EditorChanged("b".into()));
    ctrl(&mut app, 's');
    send(
        &mut app,
        Msg::SaveFailed {
            nb: nb(),
            id: 1,
            error: "network error".into(),
        },
    );
    assert_eq!(app.status.level, locus_core::Level::Error);
    let ed = view_model::editor(&app).unwrap();
    assert_eq!(ed.body, "b");
    assert!(ed.dirty && !ed.saving);
    // A later Ctrl+S tries again.
    assert_eq!(ctrl(&mut app, 's').len(), 1);
}

#[test]
fn n_creates_untitled_and_opens_it_in_the_editor() {
    let mut app = loaded_notes(vec![note(1, "Old", "", 1, 1)]);
    let fx = press(&mut app, Key::Char('n'));
    assert_eq!(
        fx,
        vec![Effect::CreateNote {
            nb: nb(),
            folder: 10,
            title: "Untitled".into(),
            body: String::new()
        }]
    );
    let created = note(2, "Untitled", "", 1, 9);
    let fx = send(
        &mut app,
        Msg::NoteCreated {
            nb: nb(),
            note: created,
        },
    );
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(view_model::editor(&app).unwrap().note_id, 2);
}

#[test]
fn rename_prefills_and_submits_trimmed_title() {
    let mut app = loaded_notes(vec![note(3, "Old name", "", 1, 1)]);
    press(&mut app, Key::Char('r'));
    assert_eq!(app.input.text, "Old name");
    // Clear with backspaces, then type.
    for _ in 0.."Old name".len() {
        press(&mut app, Key::Backspace);
    }
    type_str(&mut app, "  New  ");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::RenameNote {
            nb: nb(),
            id: 3,
            title: "New".into()
        }]
    );
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn rename_to_blank_is_rejected_and_stays_open() {
    let mut app = loaded_notes(vec![note(3, "x", "", 1, 1)]);
    press(&mut app, Key::Char('r'));
    press(&mut app, Key::Backspace);
    let fx = press(&mut app, Key::Enter);
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Input(locus_core::InputKind::RenameNote));
    assert_eq!(app.status.level, locus_core::Level::Error);
}

#[test]
fn note_deleted_closes_its_editor() {
    let mut app = loaded_notes(vec![note(1, "a", "", 1, 1)]);
    open_first_note(&mut app);
    send(&mut app, Msg::NoteDeleted { nb: nb(), id: 1 });
    assert!(app.editor.is_none());
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn bad_data_never_panics() {
    let mut app = loaded_notes(vec![]);
    assert!(send(&mut app, Msg::EditorChanged("x".into())).is_empty());
    assert!(send(&mut app, Msg::SaveTimer(99)).is_empty());
    assert!(
        send(
            &mut app,
            Msg::NoteSaved {
                nb: nb(),
                note: note(42, "?", "", 1, 1)
            }
        )
        .is_empty()
    );
    assert!(
        send(
            &mut app,
            Msg::Goto {
                section: locus_core::Section::Notes,
                id: 404
            }
        )
        .is_empty()
    );
    assert_eq!(
        send(
            &mut app,
            Msg::ExecuteDelete {
                section: locus_core::Section::Files,
                id: 1
            }
        ),
        vec![Effect::DeleteFile { id: 1 }]
    );
    send(&mut app, Msg::ConfirmAnswer(true));
    assert_eq!(app.focus, Focus::List);
}
