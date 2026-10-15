mod common;

use common::*;
use locus_core::{Effect, Focus, InputKind, Key, Mods, Msg, Section, keymap, view_model};

#[test]
fn switching_sections_clears_filter_and_focus() {
    let mut app = loaded_notes(vec![note(1, "Milk run", "", 1, 5)]);
    ctrl(&mut app, 'f');
    type_str(&mut app, "milk");
    press(&mut app, Key::Enter);
    assert_eq!(app.filter, "milk");
    send(&mut app, Msg::Switch(Section::Todo));
    assert!(app.filter.is_empty());
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn first_load_creates_notebook_when_none_exist() {
    let mut app = App::new();
    let fx = send(&mut app, Msg::Reload);
    assert_eq!(fx, vec![Effect::LoadNotebooks]);
    let fx = send(&mut app, Msg::NotebooksLoaded { notebooks: vec![] });
    assert_eq!(
        fx,
        vec![Effect::CreateNotebook {
            title: "Locus".into()
        }]
    );
    let fx = send(&mut app, Msg::NotebookCreated { notebook: nb() });
    assert_eq!(
        fx,
        vec![
            Effect::LoadNotes { nb: nb() },
            Effect::LoadFolders { nb: nb() }
        ]
    );
}

#[test]
fn j_k_stop_at_the_ends() {
    let mut app = loaded_notes(vec![
        note(1, "a", "", 1, 3),
        note(2, "b", "", 1, 2),
        note(3, "c", "", 1, 1),
    ]);
    assert_eq!(app.notes.sel, 0);
    press(&mut app, Key::Up);
    assert_eq!(app.notes.sel, 0, "k at the top stays at the top");
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    assert_eq!(app.notes.sel, 2, "j past the bottom stays at the bottom");
    press(&mut app, Key::Up);
    assert_eq!(app.notes.sel, 1);
    press(&mut app, Key::Down);
    assert_eq!(app.notes.sel, 2);
}

#[test]
fn ctrl_d_and_ctrl_u_move_half_a_page() {
    let mut app = loaded_notes((1..=30).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::PageDown);
    assert_eq!(app.notes.sel, 10);
    press(&mut app, Key::PageDown);
    assert_eq!(app.notes.sel, 20);
    press(&mut app, Key::PageUp);
    assert_eq!(app.notes.sel, 10);
    press(&mut app, Key::PageUp);
    press(&mut app, Key::PageUp);
    assert_eq!(app.notes.sel, 0);
}

#[test]
fn notes_are_listed_newest_first() {
    let app = loaded_notes(vec![
        note(1, "old", "", 1, 100),
        note(2, "new", "", 1, 300),
        note(3, "mid", "", 1, 200),
    ]);
    let titles: Vec<_> = view_model::notes_list(&app)
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert_eq!(titles, ["new", "mid", "old"]);
}

#[test]
fn filter_narrows_live_and_escape_clears_in_two_steps() {
    let mut app = loaded_notes(vec![
        note(1, "Groceries", "buy MILK", 1, 3),
        note(2, "Ideas", "rust", 1, 2),
        note(3, "Chores", "", 1, 1),
    ]);
    ctrl(&mut app, 'f');
    assert_eq!(app.focus, Focus::Input(InputKind::Filter));
    // Typed characters go into the filter, not into list commands.
    type_str(&mut app, "mi");
    assert_eq!(app.filter, "mi");
    let titles: Vec<_> = view_model::notes_list(&app)
        .into_iter()
        .map(|r| r.title)
        .collect();
    assert_eq!(
        titles,
        ["Groceries"],
        "filter matches note bodies, case-insensitively"
    );

    // Enter leaves the filter box but keeps the filter.
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::List);
    assert_eq!(app.filter, "mi");

    // First Escape in the list clears the search.
    press(&mut app, Key::Escape);
    assert!(app.filter.is_empty());
    assert_eq!(view_model::notes_list(&app).len(), 3);
}

#[test]
fn escape_in_filter_box_leaves_box_then_clears() {
    let mut app = loaded_notes(vec![note(1, "a", "", 1, 1), note(2, "b", "", 1, 2)]);
    ctrl(&mut app, 'f');
    type_str(&mut app, "b");
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
    assert_eq!(app.filter, "b");
    press(&mut app, Key::Escape);
    assert!(app.filter.is_empty());
}

#[test]
fn delete_asks_first_and_n_cancels() {
    let mut app = loaded_notes(vec![note(1, "Doomed", "", 1, 1)]);
    let fx = press(&mut app, Key::Delete);
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Confirm);
    assert_eq!(
        view_model::confirm_prompt(&app).as_deref(),
        Some("Delete note \"Doomed\"?")
    );
    let fx = press(&mut app, Key::Char('n'));
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::List);
    assert!(app.confirm.is_none());
    assert_eq!(app.notes.note_count(), 1);
}

#[test]
fn delete_then_y_emits_delete_effect() {
    let mut app = loaded_notes(vec![note(7, "Doomed", "", 1, 1)]);
    press(&mut app, Key::Delete);
    let fx = press(&mut app, Key::Char('y'));
    assert_eq!(fx, vec![Effect::DeleteNote { nb: nb(), id: 7 }]);
    let fx = send(&mut app, Msg::NoteDeleted { nb: nb(), id: 7 });
    assert!(fx.is_empty());
    assert!((app.notes.note_count() == 0));
}

#[test]
fn escape_closes_confirm_without_deleting() {
    let mut app = loaded_notes(vec![note(1, "x", "", 1, 1)]);
    press(&mut app, Key::Delete);
    assert!(press(&mut app, Key::Escape).is_empty());
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn help_overlay_toggles_and_lists_section_bindings() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::F(1));
    assert_eq!(app.focus, Focus::Help);
    let help = view_model::help(&app);
    assert!(help.iter().any(|(k, _)| k == "Ctrl+K"));
    assert!(help.iter().any(|(k, _)| k == "Delete"));
    assert!(help.iter().any(|(k, _)| k == "F2"));
    press(&mut app, Key::F(1));
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn keys_are_ignored_while_help_is_open() {
    let mut app = loaded_notes((1..=3).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::F(1));
    press(&mut app, Key::Down);
    assert_eq!(app.notes.sel, 0);
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn ctrl_k_opens_palette_and_escape_closes_it() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, 'k');
    assert_eq!(app.focus, Focus::Palette);
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn letters_filter_the_list_and_never_run_commands() {
    let mut app = loaded_notes(vec![note(1, "Milk run", "", 1, 5), note(2, "Taxes", "", 1, 4)]);
    for c in ['n', 'd', 'j', 'q'] {
        assert_eq!(
            keymap(&app, Key::Char(c), Mods::NONE),
            Some(Msg::Batch(vec![
                Msg::FocusFilter,
                Msg::Text(locus_core::TextEdit::Insert(c.to_string()))
            ]))
        );
    }
    let fx = press(&mut app, Key::Char('m'));
    assert!(fx.is_empty(), "typing creates and deletes nothing");
    type_str(&mut app, "ilk");
    assert_eq!(app.filter, "milk");
    assert_eq!(view_model::notes_list(&app).len(), 1);
}

#[test]
fn ctrl_and_alt_combos_without_a_meaning_are_unbound() {
    let app = loaded_notes(vec![]);
    let ctrl_alt = Mods {
        ctrl: true,
        alt: true,
        ..Mods::default()
    };
    assert_eq!(keymap(&app, Key::Char('1'), ctrl_alt), None);
    assert_eq!(keymap(&app, Key::Char('2'), Mods::CTRL), None);
    assert_eq!(keymap(&app, Key::Char('n'), Mods::CTRL), Some(Msg::New));
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    assert_eq!(keymap(&app, Key::Char('n'), alt), Some(Msg::New), "browsers keep Ctrl+N");
}

#[test]
fn list_keys_do_nothing_on_an_empty_list() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::Down);
    press(&mut app, Key::End);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Delete);
    assert_eq!(app.notes.sel, 0);
    assert_eq!(
        app.focus,
        Focus::List,
        "delete on an empty list does not open a prompt"
    );
    assert!(app.editor.is_none());
}

#[test]
fn ctrl_r_reloads_the_current_section() {
    let mut app = loaded_notes(vec![]);
    send(&mut app, Msg::Switch(Section::Calendar));
    assert_eq!(ctrl(&mut app, 'r'), vec![Effect::LoadEvents]);
    assert!(app.is_loading(Section::Calendar));
}

#[test]
fn page_keys_and_home_end_map_to_list_movement() {
    let mut app = loaded_notes((1..=30).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::End);
    assert_eq!(app.notes.sel, 29);
    press(&mut app, Key::PageUp);
    assert_eq!(app.notes.sel, 19);
    press(&mut app, Key::Home);
    assert_eq!(app.notes.sel, 0);
    press(&mut app, Key::PageDown);
    assert_eq!(app.notes.sel, 10);
}

#[test]
fn switching_sections_cancels_an_open_form() {
    let mut app = loaded_notes(vec![]);
    send(&mut app, Msg::Switch(Section::Contacts));
    ctrl(&mut app, 'n');
    assert_eq!(app.focus, Focus::Form);
    send(&mut app, Msg::Switch(Section::Notes));
    assert!(app.form.is_none());
    send(&mut app, Msg::Switch(Section::Contacts));
    assert!(view_model::form(&app).is_none(), "no stale form card");
}
