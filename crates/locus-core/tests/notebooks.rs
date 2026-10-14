//! Notebook and folder navigation in Notes, and what it means for the
//! editor, the palette, new notes and Todo.
//!
//! Fixture: `Locus` (flag `notes`, root 10) has `Alpha`/`beta` under the root
//! and `deep` under `beta`; `Journal` (flag `journal`, root 20) has `Daily`.
//! Both have a note with id 1.
mod common;

use common::*;
use locus_core::{Effect, Focus, Key, Msg, Section, Target, view_model};

fn two_books() -> App {
    let mut app = App::new();
    send(&mut app, Msg::Reload);
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb(), nb_journal()],
        },
    );
    send(
        &mut app,
        Msg::FoldersLoaded {
            nb: nb(),
            folders: vec![
                folder(10, "root", None),
                folder(11, "beta", Some(10)),
                folder(12, "Alpha", Some(10)),
                folder(13, "deep", Some(11)),
            ],
        },
    );
    send(
        &mut app,
        Msg::FoldersLoaded {
            nb: nb_journal(),
            folders: vec![folder(20, "root", None), folder(21, "Daily", Some(20))],
        },
    );
    send(
        &mut app,
        Msg::NotesLoaded {
            nb: nb(),
            notes: vec![
                note_in(1, 10, "Top note", "", 5),
                note_in(2, 11, "In beta", "x", 4),
                note_in(3, 13, "In deep", "y", 3),
                note_in(4, 10, "Newer top", "z", 9),
            ],
        },
    );
    send(
        &mut app,
        Msg::NotesLoaded {
            nb: nb_journal(),
            notes: vec![
                note_in(1, 20, "Journal one", "j", 7),
                note_in(2, 21, "Entry", "- [ ] journal task\n", 6),
            ],
        },
    );
    app
}

fn titles(app: &App) -> Vec<String> {
    view_model::notes_list(app)
        .into_iter()
        .map(|r| r.title)
        .collect()
}

fn selected_title(app: &App) -> String {
    view_model::notes_list(app)
        .into_iter()
        .find(|r| r.selected)
        .map(|r| r.title)
        .unwrap_or_default()
}

fn key(app: &mut App, c: char) {
    press(app, Key::Char(c));
}

#[test]
fn loading_notebooks_requests_notes_and_folders_for_each() {
    let mut app = App::new();
    send(&mut app, Msg::Reload);
    let fx = send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb(), nb_journal()],
        },
    );
    for n in [nb(), nb_journal()] {
        assert!(fx.contains(&Effect::LoadNotes { nb: n.clone() }));
        assert!(fx.contains(&Effect::LoadFolders { nb: n }));
    }
}

#[test]
fn several_notebooks_start_at_the_notebook_list() {
    let app = two_books();
    assert_eq!(titles(&app), ["Journal", "Locus"]);
    let subs: Vec<_> = view_model::notes_list(&app)
        .into_iter()
        .map(|r| r.subtitle)
        .collect();
    assert_eq!(subs, ["2 notes", "4 notes"]);
    assert_eq!(view_model::notes_breadcrumb(&app), "Notebooks");
    assert_eq!(view_model::sidebar(&app)[0].badge, Some(6));
}

#[test]
fn a_single_notebook_starts_in_its_root() {
    let app = loaded_notes(vec![note(1, "Only", "a", 1, 1)]);
    assert_eq!(titles(&app), ["Only"]);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus");
}

#[test]
fn rows_list_subfolders_by_name_then_notes_newest_first() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert_eq!(titles(&app), ["Alpha", "beta", "Newer top", "Top note"]);
    let rows = view_model::notes_list(&app);
    assert!(rows[0].is_dir && rows[1].is_dir && !rows[2].is_dir);
    assert_eq!(rows[1].subtitle, "2 items");
}

#[test]
fn enter_goes_down_and_h_comes_back_to_the_row_it_left() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus");
    key(&mut app, 'j');
    assert_eq!(selected_title(&app), "beta");
    press(&mut app, Key::Enter);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus / beta");
    assert_eq!(titles(&app), ["deep", "In beta"]);
    press(&mut app, Key::Enter);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus / beta / deep");
    assert_eq!(titles(&app), ["In deep"]);

    key(&mut app, 'h');
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus / beta");
    assert_eq!(selected_title(&app), "deep");
    press(&mut app, Key::Backspace);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus");
    assert_eq!(selected_title(&app), "beta");
    key(&mut app, 'h');
    assert_eq!(view_model::notes_breadcrumb(&app), "Notebooks");
    assert_eq!(selected_title(&app), "Locus");
    // Nothing above the list.
    key(&mut app, 'h');
    assert_eq!(view_model::notes_breadcrumb(&app), "Notebooks");
}

#[test]
fn a_save_that_resorts_the_list_keeps_the_saved_note_selected() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    key(&mut app, 'G');
    assert_eq!(selected_title(&app), "Top note");
    // Saving makes it the newest note, so it sorts above "Newer top".
    send(
        &mut app,
        Msg::NoteSaved {
            nb: nb(),
            note: note_in(1, 10, "Top note", "edited", 99),
        },
    );
    assert_eq!(titles(&app), ["Alpha", "beta", "Top note", "Newer top"]);
    assert_eq!(selected_title(&app), "Top note");
}

#[test]
fn l_opens_a_note_in_the_editor() {
    let mut app = two_books();
    key(&mut app, 'j');
    key(&mut app, 'l');
    key(&mut app, 'G');
    key(&mut app, 'l');
    assert_eq!(app.focus, Focus::Editor);
    let ed = view_model::editor(&app).unwrap();
    assert_eq!((ed.note_id, ed.title.as_str()), (1, "Top note"));
}

#[test]
fn the_location_is_remembered_when_leaving_notes() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    ctrl(&mut app, '3');
    ctrl(&mut app, '1');
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus");
}

#[test]
fn filter_applies_inside_the_current_folder_only() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    key(&mut app, '/');
    type_str(&mut app, "deep");
    press(&mut app, Key::Enter);
    assert!(titles(&app).is_empty(), "the match is in a subfolder");
    press(&mut app, Key::Escape);
    press(&mut app, Key::Escape);
    key(&mut app, '/');
    type_str(&mut app, "alp");
    press(&mut app, Key::Enter);
    assert_eq!(titles(&app), ["Alpha"]);
}

#[test]
fn n_creates_in_the_current_folder() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    let fx = press(&mut app, Key::Char('n'));
    assert!(matches!(
        fx.as_slice(),
        [Effect::CreateNote { nb: n, folder: 11, .. }] if *n == nb()
    ));
}

#[test]
fn n_from_the_list_uses_the_last_opened_notebook_else_the_first() {
    let mut app = two_books();
    let fx = press(&mut app, Key::Char('n'));
    assert!(matches!(
        fx.as_slice(),
        [Effect::CreateNote { nb: n, folder: 10, .. }] if *n == nb()
    ));

    let mut app = two_books();
    press(&mut app, Key::Enter); // Journal
    key(&mut app, 'h');
    let fx = press(&mut app, Key::Char('n'));
    assert!(matches!(
        fx.as_slice(),
        [Effect::CreateNote { nb: n, folder: 20, .. }] if *n == nb_journal()
    ));
}

#[test]
fn a_created_note_shows_where_it_was_made_and_opens() {
    let mut app = two_books();
    press(&mut app, Key::Enter);
    press(&mut app, Key::Char('n'));
    send(
        &mut app,
        Msg::NoteCreated {
            nb: nb_journal(),
            note: note_in(9, 20, "Untitled", "", 50),
        },
    );
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(selected_title(&app), "Untitled");
    assert_eq!(view_model::notes_breadcrumb(&app), "Journal");
}

#[test]
fn palette_jumps_to_a_note_in_another_notebook() {
    let mut app = two_books();
    ctrl(&mut app, 'k');
    type_str(&mut app, "entry");
    let rows = view_model::palette_rows(&app);
    assert_eq!(rows[0].label, "Entry");
    assert_eq!(rows[0].detail, "Journal / Daily");
    assert_eq!(rows[0].hint, "note");
    press(&mut app, Key::Enter);

    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(view_model::notes_breadcrumb(&app), "Journal / Daily");
    assert_eq!(selected_title(&app), "Entry");
    assert_eq!(app.editor.as_ref().unwrap().nb, nb_journal());
}

#[test]
fn palette_note_in_a_deep_folder_reveals_the_folder() {
    let mut app = two_books();
    ctrl(&mut app, 'k');
    type_str(&mut app, "in deep");
    assert_eq!(
        view_model::palette_rows(&app)[0].detail,
        "Locus / beta / deep"
    );
    press(&mut app, Key::Enter);
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus / beta / deep");
    assert_eq!(selected_title(&app), "In deep");
}

#[test]
fn same_note_id_in_two_notebooks_is_not_confused() {
    let mut app = two_books();
    let at = |nb: &locus_core::locus_client::notes::NotebookSummary| {
        Target::NoteAt(locus_core::NoteRef {
            nb: locus_core::NbKey::of(nb),
            id: 1,
        })
    };
    send(
        &mut app,
        Msg::Open {
            section: Section::Notes,
            target: Some(at(&nb_journal())),
        },
    );
    let ed = app.editor.as_ref().unwrap();
    assert_eq!(
        (ed.nb.clone(), ed.title.as_str()),
        (nb_journal(), "Journal one")
    );

    // A save landing for Locus note 1 leaves the Journal editor alone.
    send(
        &mut app,
        Msg::NoteSaved {
            nb: nb(),
            note: note_in(1, 10, "Renamed top", "new", 99),
        },
    );
    let ed = app.editor.as_ref().unwrap();
    assert_eq!((ed.title.as_str(), ed.body.as_str()), ("Journal one", "j"));

    // So does a rename and a delete there.
    send(
        &mut app,
        Msg::NoteRenamed {
            nb: nb(),
            id: 1,
            title: "x".into(),
        },
    );
    send(&mut app, Msg::NoteDeleted { nb: nb(), id: 1 });
    assert_eq!(app.editor.as_ref().unwrap().title, "Journal one");
    assert_eq!(app.notes.note_count(), 5);

    // The bare form means the default notebook, which is now Journal.
    send(
        &mut app,
        Msg::Open {
            section: Section::Notes,
            target: Some(Target::Note(2)),
        },
    );
    assert_eq!(app.editor.as_ref().unwrap().title, "Entry");
}

#[test]
fn editor_saves_go_to_the_notes_own_notebook() {
    let mut app = two_books();
    ctrl(&mut app, 'k');
    type_str(&mut app, "journal one");
    press(&mut app, Key::Enter);
    send(&mut app, Msg::EditorChanged("changed".into()));
    let fx = ctrl(&mut app, 's');
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb_journal(),
            id: 1,
            body: "changed".into(),
            expected_revision: 1,
        }]
    );
}

#[test]
fn rename_and_delete_use_the_selected_notes_notebook() {
    let mut app = two_books();
    press(&mut app, Key::Enter); // Journal
    key(&mut app, 'j'); // Daily is row 0, Journal one row 1
    assert_eq!(selected_title(&app), "Journal one");
    key(&mut app, 'r');
    type_str(&mut app, "!");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::RenameNote {
            nb: nb_journal(),
            id: 1,
            title: "Journal one!".into()
        }]
    );
    key(&mut app, 'd');
    let fx = press(&mut app, Key::Char('y'));
    assert_eq!(
        fx,
        vec![Effect::DeleteNote {
            nb: nb_journal(),
            id: 1
        }]
    );
}

#[test]
fn delete_and_rename_ignore_folder_rows() {
    let mut app = two_books();
    press(&mut app, Key::Enter);
    assert_eq!(selected_title(&app), "Daily");
    key(&mut app, 'd');
    assert_eq!(app.focus, Focus::List);
    key(&mut app, 'r');
    assert_eq!(app.focus, Focus::List);
    key(&mut app, 'i');
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn a_conflict_copy_is_made_next_to_the_original() {
    let mut app = two_books();
    let fx = send(
        &mut app,
        Msg::SaveConflict {
            nb: nb_journal(),
            id: 2,
            body: "mine".into(),
            current: Some(3),
        },
    );
    assert!(matches!(
        &fx[0],
        Effect::CreateNote { nb: n, folder: 21, title, .. }
            if *n == nb_journal() && title == "Entry (conflict copy)"
    ));
}

#[test]
fn todo_collects_tasks_from_every_notebook() {
    let mut app = two_books();
    send(
        &mut app,
        Msg::NoteLoaded {
            nb: nb(),
            note: note_in(1, 10, "Top note", "- [ ] locus task\n", 5),
        },
    );
    ctrl(&mut app, '2');
    let groups = view_model::todo_groups(&app);
    let tasks: Vec<_> = groups[3]
        .items
        .iter()
        .map(|t| (t.text.clone(), t.source_note_title.clone()))
        .collect();
    assert_eq!(
        tasks,
        [
            ("journal task".to_string(), "Journal / Entry".to_string()),
            ("locus task".to_string(), "Locus / Top note".to_string()),
        ]
    );

    // Toggling writes to the task's own notebook.
    let fx = press(&mut app, Key::Char('x'));
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb_journal(),
            id: 2,
            body: "- [x] journal task\n".into(),
            expected_revision: 1,
        }]
    );
    // Enter jumps to the source in the right notebook.
    press(&mut app, Key::Char('j'));
    press(&mut app, Key::Enter);
    assert_eq!(app.section, Section::Notes);
    assert_eq!(app.editor.as_ref().unwrap().title, "Top note");
    assert_eq!(app.editor.as_ref().unwrap().nb, nb());
}

#[test]
fn todo_fetches_missing_bodies_once_per_note() {
    let mut app = two_books();
    let fx = ctrl(&mut app, '2');
    let loads: Vec<_> = fx
        .iter()
        .filter(|e| matches!(e, Effect::LoadNote { .. }))
        .collect();
    assert_eq!(loads, [&Effect::LoadNote { nb: nb(), id: 1 }]);
    let fx = send(
        &mut app,
        Msg::NoteLoaded {
            nb: nb_journal(),
            note: note_in(1, 20, "Journal one", "j", 7),
        },
    );
    assert!(fx.is_empty(), "the pending fetch is not repeated: {fx:?}");
}

#[test]
fn new_tasks_go_to_the_inbox_of_the_default_notebook() {
    let mut app = two_books();
    press(&mut app, Key::Enter); // browse Journal
    ctrl(&mut app, '2');
    key(&mut app, 'n');
    type_str(&mut app, "buy milk");
    let fx = press(&mut app, Key::Enter);
    assert!(matches!(
        fx.as_slice(),
        [Effect::CreateNote { nb: n, folder: 20, title, .. }]
            if *n == nb_journal() && title == "Inbox"
    ));
}

#[test]
fn reloading_notebooks_keeps_the_location_and_loaded_notes() {
    let mut app = two_books();
    key(&mut app, 'j');
    press(&mut app, Key::Enter);
    ctrl(&mut app, 'r');
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb(), nb_journal()],
        },
    );
    assert_eq!(view_model::notes_breadcrumb(&app), "Locus");
    assert_eq!(app.notes.note_count(), 6);
    // A notebook that disappeared sends the list back to the notebooks.
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb_journal()],
        },
    );
    assert_eq!(view_model::notes_breadcrumb(&app), "Notebooks");
}
