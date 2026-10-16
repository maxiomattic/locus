//! Move in Notes: the destination picker, moves within a notebook, and
//! moves of a note into another notebook (copy, then delete the original).
//!
//! Fixture: [`common::two_books`].
mod common;

use common::*;
use locus_core::view_model::{self, MoveRow};
use locus_core::{Effect, Focus, InputKind, Key, Level, Msg, NbKey, NoteRef, NotesLoc};

/// Into Locus and select its `Top note` (the bottom row).
fn at_top_note(app: &mut App) {
    press(app, Key::Down);
    press(app, Key::Enter);
    press(app, Key::End);
    assert_eq!(selected_title(app), "Top note");
}

fn labels(app: &App) -> Vec<String> {
    picker_rows(app).into_iter().map(|r| r.label).collect()
}

fn picker_rows(app: &App) -> Vec<MoveRow> {
    view_model::move_picker(app)
        .map(|p| p.rows)
        .unwrap_or_default()
}

fn highlighted(app: &App) -> String {
    picker_rows(app)
        .into_iter()
        .find(|r| r.selected)
        .map(|r| r.label)
        .unwrap_or_default()
}

fn top_note() -> NoteRef {
    NoteRef {
        nb: NbKey::of(&nb()),
        id: 1,
    }
}

/// Note 1 of Locus as copied into Journal's `Daily` with id 5.
fn copy_in_daily() -> locus_core::locus_client::notes::Note {
    note_in(5, 21, "Top note", "top body", 10)
}

fn moved_across(deleted: Result<(), String>) -> Msg {
    Msg::NoteMovedAcross {
        from: nb(),
        id: 1,
        to: nb_journal(),
        note: Box::new(copy_in_daily()),
        deleted,
    }
}

fn book_titles(app: &App, key: &NbKey) -> Vec<String> {
    let book = app.notes.book(key).unwrap();
    book.notes.iter().map(|n| n.title.clone()).collect()
}

#[test]
fn the_picker_lists_every_folder_of_every_notebook() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    assert_eq!(app.focus, Focus::Input(InputKind::MoveNote));
    let picker = view_model::move_picker(&app).unwrap();
    assert_eq!(picker.title, "Move \"Top note\"");
    assert_eq!(picker.filter, "");
    assert_eq!(
        labels(&app),
        [
            "Journal › /",
            "Journal › /Daily",
            "Locus › /",
            "Locus › /Alpha",
            "Locus › /beta",
            "Locus › /beta/deep",
        ]
    );
    // The note's own folder is shown but not choosable.
    let enabled: Vec<bool> = picker_rows(&app).iter().map(|r| r.enabled).collect();
    assert_eq!(enabled, [true, true, false, true, true, true]);
    assert_eq!(highlighted(&app), "Journal › /");
    assert!(view_model::input_prompt(&app).is_none());
}

#[test]
fn typing_filters_over_the_whole_label_ignoring_case() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    type_str(&mut app, "BETA");
    assert_eq!(labels(&app), ["Locus › /beta", "Locus › /beta/deep"]);
    assert_eq!(highlighted(&app), "Locus › /beta");

    clear(&mut app);
    type_str(&mut app, "journal");
    assert_eq!(labels(&app), ["Journal › /", "Journal › /Daily"]);

    // Only the current place left: nothing to choose, Enter does nothing.
    clear(&mut app);
    type_str(&mut app, "locus › /");
    assert_eq!(labels(&app)[0], "Locus › /");
    clear(&mut app);
    type_str(&mut app, "nothing like it");
    assert!(labels(&app).is_empty());
    assert!(!view_model::move_picker(&app).unwrap().can_move);
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.focus, Focus::Input(InputKind::MoveNote));
}

fn clear(app: &mut App) {
    send(app, Msg::Text(locus_core::TextEdit::Set(String::new())));
}

#[test]
fn arrows_skip_the_current_folder_and_enter_confirms() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    press(&mut app, Key::Down);
    assert_eq!(highlighted(&app), "Journal › /Daily");
    press(&mut app, Key::Down);
    assert_eq!(highlighted(&app), "Locus › /Alpha");
    press(&mut app, Key::Up);
    assert_eq!(highlighted(&app), "Journal › /Daily");
    press(&mut app, Key::Up);
    press(&mut app, Key::Up);
    assert_eq!(highlighted(&app), "Journal › /");
    for _ in 0..9 {
        press(&mut app, Key::Down);
    }
    assert_eq!(highlighted(&app), "Locus › /beta/deep");

    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::MoveNote {
            nb: nb(),
            id: 1,
            folder: 13,
        }]
    );
    assert_eq!(app.focus, Focus::List);
    assert!(app.notes.rename.is_none());
}

#[test]
fn a_click_on_the_current_folder_does_not_move() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    send(&mut app, Msg::Select(2));
    assert!(!view_model::move_picker(&app).unwrap().can_move);
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.status.text, "The note is already there");
    assert_eq!(app.focus, Focus::Input(InputKind::MoveNote));
}

#[test]
fn a_double_click_confirms_its_row_and_escape_cancels() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    // A click selects; the list underneath keeps its selection.
    send(&mut app, Msg::Select(3));
    assert_eq!(highlighted(&app), "Locus › /Alpha");
    assert_eq!(selected_title(&app), "Top note");
    let fx = press(&mut app, Key::Escape);
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::List);
    assert!(view_model::move_picker(&app).is_none());

    send(&mut app, Msg::Move);
    let fx = send(&mut app, Msg::Batch(vec![Msg::Select(4), Msg::Submit]));
    assert_eq!(
        fx,
        vec![Effect::MoveNote {
            nb: nb(),
            id: 1,
            folder: 11,
        }]
    );
}

#[test]
fn a_move_within_the_notebook_works_as_before() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    type_str(&mut app, "deep");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::MoveNote {
            nb: nb(),
            id: 1,
            folder: 13,
        }]
    );
    send(
        &mut app,
        Msg::NoteMoved {
            nb: nb(),
            id: 1,
            folder: 13,
        },
    );
    assert_eq!(titles(&app), ["Alpha", "beta", "Newer top"]);
    assert_eq!(app.status.text, "Moved to Locus / beta / deep");
}

#[test]
fn a_folder_moves_only_within_its_notebook_and_not_below_itself() {
    let mut app = two_books();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Down);
    assert_eq!(selected_title(&app), "beta");
    send(&mut app, Msg::Move);
    assert_eq!(app.focus, Focus::Input(InputKind::MoveNoteFolder));
    let picker = view_model::move_picker(&app).unwrap();
    assert_eq!(picker.title, "Move folder \"beta\"");
    // No Journal, no beta, no deep; the top is where beta is now.
    assert_eq!(labels(&app), ["Locus › /", "Locus › /Alpha"]);
    assert!(!picker.rows[0].enabled);
    assert_eq!(highlighted(&app), "Locus › /Alpha");
    press(&mut app, Key::Up);
    assert_eq!(highlighted(&app), "Locus › /Alpha");

    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::UpdateNoteFolder {
            nb: nb(),
            id: 11,
            name: None,
            parent: Some(12),
        }]
    );
    assert_eq!(app.focus, Focus::List);
    assert!(app.notes.folder_edit.is_none());
}

#[test]
fn a_folder_cannot_land_next_to_one_with_its_name() {
    let mut app = two_books();
    send(
        &mut app,
        Msg::FoldersLoaded {
            nb: nb(),
            folders: vec![
                folder(10, "root", None),
                folder(11, "beta", Some(10)),
                folder(12, "Alpha", Some(10)),
                folder(13, "deep", Some(11)),
                folder(14, "beta", Some(12)),
            ],
        },
    );
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Down);
    assert_eq!(selected_title(&app), "beta");
    send(&mut app, Msg::Move);
    type_str(&mut app, "alpha");
    assert_eq!(highlighted(&app), "Locus › /Alpha");
    assert!(press(&mut app, Key::Enter).is_empty());
    assert!(view_model::status_line(&app).contains("already there"));
    assert_eq!(app.focus, Focus::Input(InputKind::MoveNoteFolder));
}

#[test]
fn another_notebook_asks_first_then_moves_across() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(&mut app, Msg::Move);
    type_str(&mut app, "daily");
    let fx = press(&mut app, Key::Enter);
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Confirm);
    let confirm = view_model::confirm_view(&app).unwrap();
    assert_eq!(confirm.prompt, "Move \"Top note\" to Journal?");
    assert_eq!(confirm.detail, "The note's revision history stays behind.");
    assert_eq!(confirm.button, "Move");
    assert!(!confirm.destructive);

    // Cancel changes nothing.
    assert!(press(&mut app, Key::Escape).is_empty());
    assert_eq!(app.focus, Focus::List);

    send(&mut app, Msg::Move);
    type_str(&mut app, "daily");
    press(&mut app, Key::Enter);
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::MoveNoteAcross {
            from: nb(),
            id: 1,
            to: nb_journal(),
            folder: 21,
        }]
    );
    assert_eq!(app.status.text, "Moving note to Journal");
}

#[test]
fn a_finished_move_shows_the_copy_in_both_lists() {
    let mut app = two_books();
    at_top_note(&mut app);
    let fx = send(&mut app, moved_across(Ok(())));
    assert!(fx.is_empty());
    assert!(app.editor.is_none());
    assert_eq!(
        book_titles(&app, &NbKey::of(&nb())),
        ["In beta", "In deep", "Newer top"]
    );
    assert_eq!(
        book_titles(&app, &NbKey::of(&nb_journal())),
        ["Journal one", "Entry", "Top note"]
    );
    // The list follows the note to its new folder.
    assert_eq!(
        app.notes.loc,
        NotesLoc::Folder {
            nb: NbKey::of(&nb_journal()),
            folder: 21,
        }
    );
    assert_eq!(selected_title(&app), "Top note");
    assert_eq!(app.status.level, Level::Info);
    assert_eq!(app.status.text, "Moved to Journal / Daily");
}

#[test]
fn a_moved_note_that_was_open_opens_as_the_copy() {
    let mut app = two_books();
    at_top_note(&mut app);
    press(&mut app, Key::Enter);
    assert!(
        app.editor
            .as_ref()
            .is_some_and(|e| e.is(&NbKey::of(&nb()), 1))
    );
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
    // Typing that arrives while the move runs is carried to the copy.
    send(&mut app, Msg::EditorChanged("top body, more".into()));

    let fx = send(&mut app, moved_across(Ok(())));
    let ed = app.editor.as_ref().unwrap();
    assert!(ed.is(&NbKey::of(&nb_journal()), 5));
    assert_eq!(ed.body, "top body, more");
    assert_eq!(app.focus, Focus::List);
    assert_eq!(
        save_notes(&fx),
        [&Effect::SaveNote {
            nb: nb_journal(),
            id: 5,
            body: "top body, more".into(),
            expected_revision: 1,
        }]
    );
}

#[test]
fn a_failed_delete_warns_that_the_note_is_in_both() {
    let mut app = two_books();
    at_top_note(&mut app);
    send(
        &mut app,
        moved_across(Err("server returned HTTP 500".into())),
    );
    assert!(book_titles(&app, &NbKey::of(&nb())).contains(&"Top note".to_string()));
    assert!(book_titles(&app, &NbKey::of(&nb_journal())).contains(&"Top note".to_string()));
    assert_eq!(app.status.level, Level::Warn);
    assert_eq!(
        app.status.text,
        locus_core::left_in_both("Top note", "Locus", "Journal", "server returned HTTP 500")
    );
    assert!(app.status.text.contains("both notebooks"));
    assert_eq!(selected_title(&app), "Top note");
}

#[test]
fn a_failed_copy_changes_nothing() {
    let mut app = two_books();
    at_top_note(&mut app);
    let before = app.notes.clone();
    send(
        &mut app,
        Msg::NoteMoveFailed {
            error: "not found".into(),
        },
    );
    assert_eq!(app.notes, before);
    assert_eq!(app.status.level, Level::Error);
    assert_eq!(app.status.text, "Move failed: not found. Nothing changed.");
}

#[test]
fn a_move_across_waits_for_the_editor_to_save() {
    let mut app = two_books();
    at_top_note(&mut app);
    press(&mut app, Key::Enter);
    let execute = Msg::ExecuteMoveNote {
        note: top_note(),
        to: NbKey::of(&nb_journal()),
        folder: 21,
    };
    // Unsaved typing.
    send(&mut app, Msg::EditorChanged("typing".into()));
    let fx = send(&mut app, execute.clone());
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::MoveNoteAcross { .. }))
    );
    assert_eq!(app.status.level, Level::Warn);
    assert!(app.status.text.contains("still saving"));

    // A save on its way.
    let fx = send(&mut app, Msg::SaveNow);
    assert_eq!(save_notes(&fx).len(), 1);
    let fx = send(&mut app, execute.clone());
    assert!(fx.is_empty());
    assert!(app.status.text.contains("still saving"));

    // Saved: the move goes ahead.
    send(
        &mut app,
        Msg::NoteSaved {
            nb: nb(),
            note: note_in(1, 10, "Top note", "typing", 6),
        },
    );
    let fx = send(&mut app, execute);
    assert!(matches!(
        fx.as_slice(),
        [Effect::MoveNoteAcross { id: 1, .. }]
    ));
}
