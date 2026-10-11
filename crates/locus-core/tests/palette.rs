mod common;

use common::*;
use locus_core::palette;
use locus_core::{Effect, Focus, Key, Msg, Section, view_model};

#[test]
fn exact_prefix_beats_substring_beats_scattered_match() {
    let prefix = palette::fuzzy_score("cal", "Calendar").unwrap();
    let inner = palette::fuzzy_score("cal", "Go to Calendar").unwrap();
    let scattered = palette::fuzzy_score("cal", "chore a list").unwrap();
    assert!(
        prefix > inner && inner > scattered,
        "{prefix} {inner} {scattered}"
    );
    assert_eq!(palette::fuzzy_score("zq", "Calendar"), None);
    assert_eq!(palette::fuzzy_score("", "anything"), Some(0));
}

#[test]
fn matching_is_case_insensitive_subsequence() {
    assert!(palette::fuzzy_score("NTE", "notes").is_some());
    assert!(palette::fuzzy_score("ntz", "notes").is_none());
}

#[test]
fn ranking_orders_matches_and_keeps_default_order_for_ties() {
    let mut app = loaded_notes(vec![
        note(1, "Weekly review", "", 1, 1),
        note(2, "Reading list", "", 1, 2),
    ]);
    ctrl(&mut app, 'k');
    type_str(&mut app, "re");
    let labels: Vec<_> = palette::ranked(&app).into_iter().map(|i| i.label).collect();
    // Prefix matches (score 1000) tie and keep default order: the command
    // comes before the note. The substring match ("Weekly re...") ranks last.
    assert_eq!(labels, ["Reload section", "Reading list", "Weekly review"]);
}

#[test]
fn empty_query_lists_sections_then_commands_then_content() {
    let mut app = loaded_notes(vec![note(1, "Weekly review", "", 1, 1)]);
    ctrl(&mut app, 'k');
    let rows = view_model::palette_rows(&app);
    assert_eq!(rows[0].label, "Go to Notes");
    assert_eq!(rows[5].label, "Go to Contacts");
    assert!(
        rows.iter()
            .any(|r| r.label == "New event" && r.hint == "command")
    );
    assert_eq!(rows.last().unwrap().label, "Weekly review");
    assert!(rows[0].selected);
}

#[test]
fn enter_runs_the_top_match_as_a_section_switch() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, 'k');
    type_str(&mut app, "calendar");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(app.section, Section::Calendar);
    assert_eq!(app.focus, Focus::List);
    assert_eq!(fx, vec![Effect::LoadEvents]);
}

#[test]
fn new_note_from_palette_switches_and_creates() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, 'k');
    type_str(&mut app, "new note");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(app.section, Section::Notes);
    assert!(matches!(fx.as_slice(), [Effect::CreateNote { title, .. }] if title == "Untitled"));
}

#[test]
fn palette_opens_a_note_in_the_editor() {
    let mut app = loaded_notes(vec![
        note(7, "Reading list", "a", 1, 1),
        note(8, "Other", "", 1, 2),
    ]);
    ctrl(&mut app, 'k');
    type_str(&mut app, "reading");
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(view_model::editor(&app).unwrap().note_id, 7);
}

#[test]
fn arrows_move_the_palette_selection_and_typing_resets_it() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, 'k');
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    assert_eq!(app.palette.sel, 2);
    type_str(&mut app, "c");
    assert_eq!(app.palette.sel, 0);
}

#[test]
fn no_match_enter_is_an_error_message_not_a_panic() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, 'k');
    type_str(&mut app, "qqqqq");
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.status.text, "No matches");
    assert_eq!(app.focus, Focus::Palette);
}

#[test]
fn palette_finds_contacts_and_files_too() {
    let mut app = loaded_notes(vec![]);
    send(
        &mut app,
        Msg::ContactsLoaded {
            contacts: vec![contact(3, "Ada Lovelace", "ada@x.io", "~ada")],
        },
    );
    ctrl(&mut app, 'k');
    type_str(&mut app, "lovelace");
    let rows = view_model::palette_rows(&app);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].hint, "contact");
}
