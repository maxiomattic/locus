mod common;

use common::*;
use locus_core::{Effect, Focus, InputKind, Key, Mods, Msg, Section, keymap, view_model};

#[test]
fn ctrl_digits_switch_sections_and_first_visit_loads() {
    let mut app = loaded_notes(vec![]);
    assert_eq!(app.section, Section::Notes);

    let fx = ctrl(&mut app, '2');
    assert_eq!(app.section, Section::Todo);
    assert!(fx.is_empty(), "Todo reuses the notes already loaded");

    let fx = ctrl(&mut app, '3');
    assert_eq!(app.section, Section::Calendar);
    assert_eq!(fx, vec![Effect::LoadEvents]);

    ctrl(&mut app, '3');
    assert_eq!(
        app.section,
        Section::Calendar,
        "second visit does not refetch"
    );
    assert_eq!(ctrl(&mut app, '3'), vec![]);

    assert_eq!(ctrl(&mut app, '4'), vec![Effect::LoadPhotos]);
    assert_eq!(
        ctrl(&mut app, '5'),
        vec![Effect::LoadFiles, Effect::LoadDirs]
    );
    assert_eq!(ctrl(&mut app, '6'), vec![Effect::LoadContacts]);
    assert_eq!(app.section, Section::Contacts);
    // Ctrl+7 and friends are not bound.
    assert_eq!(ctrl(&mut app, '7'), vec![]);
    assert_eq!(app.section, Section::Contacts);
}

#[test]
fn switching_sections_clears_filter_and_focus() {
    let mut app = loaded_notes(vec![note(1, "Milk run", "", 1, 5)]);
    press(&mut app, Key::Char('/'));
    type_str(&mut app, "milk");
    press(&mut app, Key::Enter);
    assert_eq!(app.filter, "milk");
    ctrl(&mut app, '2');
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
    press(&mut app, Key::Char('k'));
    assert_eq!(app.notes.sel, 0, "k at the top stays at the top");
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Char('j'));
    assert_eq!(app.notes.sel, 2, "j past the bottom stays at the bottom");
    press(&mut app, Key::Up);
    assert_eq!(app.notes.sel, 1);
    press(&mut app, Key::Down);
    assert_eq!(app.notes.sel, 2);
}

#[test]
fn gg_goes_top_and_capital_g_goes_bottom() {
    let notes: Vec<_> = (1..=25)
        .map(|i| note(i, &format!("n{i}"), "", 1, i))
        .collect();
    let mut app = loaded_notes(notes);
    press(&mut app, Key::Char('G'));
    assert_eq!(app.notes.sel, 24);
    press(&mut app, Key::Char('g'));
    assert!(app.pending_g);
    assert_eq!(app.notes.sel, 24, "a single g does nothing yet");
    press(&mut app, Key::Char('g'));
    assert!(!app.pending_g);
    assert_eq!(app.notes.sel, 0);
}

#[test]
fn a_g_followed_by_another_key_is_cancelled() {
    let mut app = loaded_notes((1..=5).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('k'));
    assert!(!app.pending_g);
    assert_eq!(app.notes.sel, 3);
    press(&mut app, Key::Char('g'));
    assert_eq!(app.notes.sel, 3, "the g after k starts a new prefix");
}

#[test]
fn ctrl_d_and_ctrl_u_move_half_a_page() {
    let mut app = loaded_notes((1..=30).map(|i| note(i, "n", "", 1, i)).collect());
    ctrl(&mut app, 'd');
    assert_eq!(app.notes.sel, 10);
    ctrl(&mut app, 'd');
    assert_eq!(app.notes.sel, 20);
    ctrl(&mut app, 'u');
    assert_eq!(app.notes.sel, 10);
    ctrl(&mut app, 'u');
    ctrl(&mut app, 'u');
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
    press(&mut app, Key::Char('/'));
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
    press(&mut app, Key::Char('/'));
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
    let fx = press(&mut app, Key::Char('d'));
    assert!(fx.is_empty());
    assert_eq!(app.focus, Focus::Confirm);
    assert_eq!(
        view_model::confirm_prompt(&app).as_deref(),
        Some("Delete note \"Doomed\"? (y/n)")
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
    press(&mut app, Key::Char('d'));
    let fx = press(&mut app, Key::Char('y'));
    assert_eq!(fx, vec![Effect::DeleteNote { nb: nb(), id: 7 }]);
    let fx = send(&mut app, Msg::NoteDeleted { nb: nb(), id: 7 });
    assert!(fx.is_empty());
    assert!((app.notes.note_count() == 0));
}

#[test]
fn escape_closes_confirm_without_deleting() {
    let mut app = loaded_notes(vec![note(1, "x", "", 1, 1)]);
    press(&mut app, Key::Char('d'));
    assert!(press(&mut app, Key::Escape).is_empty());
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn help_overlay_toggles_and_lists_section_bindings() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::Char('?'));
    assert_eq!(app.focus, Focus::Help);
    let help = view_model::help(&app);
    assert!(help.iter().any(|(k, _)| k == ": or Ctrl+K"));
    assert!(help.iter().any(|(k, _)| k == "g n/t/c/p/f/o"));
    assert!(help.iter().any(|(k, _)| k == "Enter / l"));
    press(&mut app, Key::Char('?'));
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn keys_are_ignored_while_help_is_open() {
    let mut app = loaded_notes((1..=3).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::Char('?'));
    press(&mut app, Key::Char('j'));
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
fn keymap_is_none_for_unbound_keys_and_alt_combos() {
    let app = loaded_notes(vec![]);
    assert_eq!(keymap(&app, Key::Char('q'), Mods::NONE), None);
    assert_eq!(
        keymap(
            &app,
            Key::Char('1'),
            Mods {
                ctrl: true,
                alt: true,
                ..Mods::default()
            }
        ),
        None
    );
    assert_eq!(
        keymap(&app, Key::Char('2'), Mods::CTRL),
        Some(Msg::Switch(Section::Todo))
    );
}

#[test]
fn list_keys_do_nothing_on_an_empty_list() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Char('G'));
    press(&mut app, Key::Enter);
    press(&mut app, Key::Char('d'));
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
    ctrl(&mut app, '3');
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

fn alt(app: &mut App, c: char) -> Vec<Effect> {
    press_with(
        app,
        Key::Char(c),
        Mods {
            alt: true,
            ..Mods::default()
        },
        now(),
    )
}

#[test]
fn g_then_letter_switches_sections() {
    let mut app = loaded_notes(vec![]);
    for (letter, want) in [
        ('t', Section::Todo),
        ('c', Section::Calendar),
        ('p', Section::Photos),
        ('f', Section::Files),
        ('o', Section::Contacts),
        ('n', Section::Notes),
    ] {
        press(&mut app, Key::Char('g'));
        assert!(app.pending_g);
        press(&mut app, Key::Char(letter));
        assert_eq!(app.section, want, "g {letter}");
        assert!(!app.pending_g);
    }
}

#[test]
fn g_n_switches_instead_of_creating_a_note() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, '3');
    press(&mut app, Key::Char('g'));
    let fx = press(&mut app, Key::Char('n'));
    assert_eq!(app.section, Section::Notes);
    assert!(fx.is_empty(), "no note is created: {fx:?}");
    assert!((app.notes.note_count() == 0));
}

#[test]
fn g_t_in_calendar_switches_instead_of_jumping_to_today() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, '3');
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('t'));
    assert_eq!(app.section, Section::Todo);
}

#[test]
fn g_letter_from_a_detail_view_switches() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, '4');
    send(
        &mut app,
        Msg::PhotosLoaded {
            photos: vec![photo(1, "a", "/Photos/a.jpg")],
        },
    );
    press(&mut app, Key::Enter);
    assert_eq!(app.focus, Focus::Detail);
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('f'));
    assert_eq!(app.section, Section::Files);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn gg_still_goes_to_the_top() {
    let mut app = loaded_notes((1..=5).map(|i| note(i, "n", "", 1, i)).collect());
    press(&mut app, Key::Char('G'));
    assert_eq!(app.notes.sel, 4);
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('g'));
    assert_eq!(app.notes.sel, 0);
    assert_eq!(app.section, Section::Notes);
}

#[test]
fn letters_without_g_keep_their_section_meaning() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, '3');
    press(&mut app, Key::Char('c'));
    assert_eq!(app.section, Section::Calendar, "plain c does not switch");
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('x'));
    assert!(!app.pending_g, "an unbound key cancels the g prefix");
    press(&mut app, Key::Char('o'));
    assert_eq!(app.section, Section::Calendar);
}

#[test]
fn g_letters_are_text_while_typing() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::Char('/'));
    press(&mut app, Key::Char('g'));
    press(&mut app, Key::Char('t'));
    assert_eq!(app.section, Section::Notes);
    assert_eq!(app.filter, "gt");
}

#[test]
fn colon_opens_the_palette() {
    let mut app = loaded_notes(vec![]);
    press(&mut app, Key::Char(':'));
    assert_eq!(app.focus, Focus::Palette);
    press(&mut app, Key::Escape);
    assert_eq!(app.focus, Focus::List);
    // In a text field, ':' is text.
    press(&mut app, Key::Char('/'));
    press(&mut app, Key::Char(':'));
    assert_eq!(app.focus, Focus::Input(InputKind::Filter));
    assert_eq!(app.filter, ":");
}

#[test]
fn colon_with_shift_held_opens_the_palette() {
    let app = loaded_notes(vec![]);
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    assert_eq!(keymap(&app, Key::Char(':'), shift), Some(Msg::OpenPalette));
}

#[test]
fn alt_digits_switch_sections_everywhere() {
    let mut app = loaded_notes(vec![]);
    for (digit, want) in [
        ('2', Section::Todo),
        ('3', Section::Calendar),
        ('4', Section::Photos),
        ('5', Section::Files),
        ('6', Section::Contacts),
        ('1', Section::Notes),
    ] {
        alt(&mut app, digit);
        assert_eq!(app.section, want, "Alt+{digit}");
    }
    // Also from inside a text field.
    press(&mut app, Key::Char('/'));
    alt(&mut app, '3');
    assert_eq!(app.section, Section::Calendar);
    assert_eq!(app.focus, Focus::List);
    // Alt+7 and other Alt combos are not bound.
    assert_eq!(alt(&mut app, '7'), vec![]);
    assert_eq!(app.section, Section::Calendar);
    let app2 = loaded_notes(vec![]);
    let alt_mods = Mods {
        alt: true,
        ..Mods::default()
    };
    assert_eq!(keymap(&app2, Key::Char('k'), alt_mods), None);
}

#[test]
fn sidebar_labels_show_g_shortcuts() {
    let app = loaded_notes(vec![]);
    let labels: Vec<String> = view_model::sidebar(&app)
        .into_iter()
        .map(|i| i.shortcut)
        .collect();
    assert_eq!(labels, ["g n", "g t", "g c", "g p", "g f", "g o"]);
}

#[test]
fn switching_sections_cancels_an_open_form() {
    let mut app = loaded_notes(vec![]);
    ctrl(&mut app, '6');
    press(&mut app, Key::Char('n'));
    assert_eq!(app.focus, Focus::Form);
    alt(&mut app, '1');
    assert!(app.form.is_none());
    alt(&mut app, '6');
    assert!(view_model::form(&app).is_none(), "no stale form card");
}
