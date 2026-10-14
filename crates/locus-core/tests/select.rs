mod common;

use common::*;
use locus_core::{Msg, Section};

#[test]
fn select_clamps_to_the_notes_list() {
    let mut app = loaded_notes((1..=5).map(|i| note(i, "n", "", 1, i)).collect());
    send(&mut app, Msg::Select(3));
    assert_eq!(app.notes.sel, 3);
    send(&mut app, Msg::Select(99));
    assert_eq!(app.notes.sel, 4);
}

#[test]
fn select_on_empty_list_is_zero() {
    let mut app = loaded_notes(vec![]);
    send(&mut app, Msg::Select(2));
    assert_eq!(app.notes.sel, 0);
}

#[test]
fn select_day_moves_calendar_and_resets_event_cursor() {
    let mut app = loaded_notes(vec![]);
    app.calendar.sel = 2;
    send(&mut app, Msg::SelectDay(date(2026, 10, 20)));
    assert_eq!(app.calendar.day, date(2026, 10, 20));
    assert_eq!(app.calendar.sel, 0);
}

#[test]
fn select_uses_the_current_section() {
    let mut app = loaded_notes((1..=3).map(|i| note(i, "n", "", 1, i)).collect());
    send(&mut app, Msg::Switch(Section::Photos));
    send(&mut app, Msg::Select(1));
    assert_eq!(
        app.photos.sel, 0,
        "no photos loaded, so the cursor stays at 0"
    );
    assert_eq!(app.notes.sel, 0);
}

#[test]
fn editor_focus_only_when_a_note_is_open() {
    let mut app = loaded_notes(vec![note(1, "n", "", 1, 1)]);
    send(&mut app, Msg::EditorFocus);
    assert_eq!(
        app.focus,
        locus_core::Focus::List,
        "no open note, no focus change"
    );
    send(&mut app, Msg::Activate);
    assert_eq!(app.focus, locus_core::Focus::Editor);
    send(&mut app, Msg::Escape);
    assert_eq!(app.focus, locus_core::Focus::List);
    send(&mut app, Msg::EditorFocus);
    assert_eq!(
        app.focus,
        locus_core::Focus::Editor,
        "editor still open after Escape"
    );
}

#[test]
fn select_moves_the_palette_cursor_while_open() {
    let mut app = loaded_notes((1..=3).map(|i| note(i, "n", "", 1, i)).collect());
    send(&mut app, Msg::OpenPalette);
    send(&mut app, Msg::Select(2));
    assert_eq!(app.palette.sel, 2);
    send(&mut app, Msg::Select(50));
    assert_eq!(app.palette.sel, app_palette_len(&app) - 1);
}

fn app_palette_len(app: &locus_core::App) -> usize {
    locus_core::view_model::palette_rows(app).len()
}

#[test]
fn select_from_the_editor_moves_focus_to_the_list() {
    let mut app = loaded_notes(vec![note(1, "a", "", 1, 2), note(2, "b", "", 1, 1)]);
    send(&mut app, Msg::Activate);
    assert_eq!(app.focus, locus_core::Focus::Editor);
    send(&mut app, Msg::Select(1));
    assert_eq!(app.focus, locus_core::Focus::List);
    assert_eq!(app.notes.sel, 1);
    assert!(app.editor.is_some(), "the open note stays open");
}
