mod common;

use common::*;
use locus_core::{Effect, Focus, Key, Msg, Section, view_model};

const BODY: &str = "# Plan\n- [ ] write report 📅 2026-10-01\n- [ ] call mum due:2026-10-11\n- [x] water plants 📅 2026-10-05\n- [ ] book flights 📅 2026-12-24\n- [ ] buy bread\n";

fn todo_app(extra: Vec<locus_core::locus_client::notes::Note>) -> locus_core::App {
    let mut notes = vec![note(1, "Plan", BODY, 7, 10)];
    notes.extend(extra);
    let mut app = loaded_notes(notes);
    send(&mut app, Msg::Switch(Section::Todo));
    app
}

#[test]
fn tasks_are_grouped_by_date_relative_to_now() {
    let app = todo_app(vec![]);
    let groups = view_model::todo_groups(&app);
    let titles: Vec<_> = groups.iter().map(|g| g.title).collect();
    assert_eq!(titles, ["Overdue", "Today", "Upcoming", "No date", "Done"]);

    let texts =
        |i: usize| -> Vec<String> { groups[i].items.iter().map(|t| t.text.clone()).collect() };
    assert_eq!(texts(0), ["write report"], "due 2026-10-01 is overdue");
    assert_eq!(texts(1), ["call mum"], "due today");
    assert_eq!(texts(2), ["book flights"]);
    assert_eq!(texts(3), ["buy bread"]);
    assert!(groups[4].collapsed, "Done is collapsed by default");
    assert_eq!(groups[4].count, 1);
    assert!(groups[4].items.is_empty());
    assert_eq!(groups[2].items[0].due_label, "2026-12-24");
    assert!(groups[0].items[0].overdue);
}

#[test]
fn grouping_moves_with_the_clock() {
    let mut app = todo_app(vec![]);
    // A week later, "call mum" is overdue and "book flights" is still upcoming.
    let later = now_at(date(2026, 10, 18), 0);
    locus_core::update(&mut app, Msg::Reload, later);
    let groups = view_model::todo_groups(&app);
    assert_eq!(groups[0].items.len(), 2);
    assert!(groups[1].items.is_empty());
}

#[test]
fn z_shows_done_and_navigation_includes_it_only_then() {
    let mut app = todo_app(vec![]);
    // Four open tasks are navigable; the collapsed Done task is not.
    press(&mut app, Key::End);
    assert_eq!(app.todo.sel, 3);
    send(&mut app, Msg::ToggleDoneTasks);
    press(&mut app, Key::End);
    assert_eq!(app.todo.sel, 4);
    let groups = view_model::todo_groups(&app);
    assert!(!groups[4].collapsed);
    assert!(groups[4].items[0].done);
}

#[test]
fn toggling_writes_exactly_one_changed_line() {
    let mut app = todo_app(vec![]);
    // Select "call mum" (the second visible task).
    press(&mut app, Key::Down);
    let fx = press(&mut app, Key::Char(' '));
    let body = match fx.as_slice() {
        [
            Effect::SaveNote {
                body,
                id: 1,
                expected_revision: 7,
                ..
            },
        ] => body.clone(),
        other => panic!("unexpected {other:?}"),
    };
    let before: Vec<&str> = BODY.split('\n').collect();
    let after: Vec<&str> = body.split('\n').collect();
    assert_eq!(before.len(), after.len());
    let changed: Vec<usize> = (0..before.len())
        .filter(|&i| before[i] != after[i])
        .collect();
    assert_eq!(changed, vec![2], "only the call-mum line changed");
    assert_eq!(after[2], "- [x] call mum due:2026-10-11");
}

#[test]
fn space_toggles_a_done_task_back_open() {
    let mut app = todo_app(vec![]);
    send(&mut app, Msg::ToggleDoneTasks);
    press(&mut app, Key::End);
    let fx = press(&mut app, Key::Char(' '));
    let body = match fx.as_slice() {
        [Effect::SaveNote { body, .. }] => body.clone(),
        other => panic!("{other:?}"),
    };
    assert!(body.contains("- [ ] water plants"));
}

#[test]
fn toggle_saves_conflict_is_handled_like_the_editor() {
    let mut app = todo_app(vec![]);
    press(&mut app, Key::Char(' '));
    let fx = send(
        &mut app,
        Msg::SaveConflict {
            nb: nb(),
            id: 1,
            body: BODY.into(),
            current: Some(8),
        },
    );
    assert!(
        fx.iter().any(
            |e| matches!(e, Effect::CreateNote { title, .. } if title == "Plan (conflict copy)")
        )
    );
    assert!(fx.contains(&Effect::LoadNote { nb: nb(), id: 1 }));
}

#[test]
fn enter_on_a_task_opens_its_source_note() {
    let mut app = todo_app(vec![note(2, "Other", "- [ ] other task", 1, 1)]);
    // Tasks sort by date then title: "book flights" is in Upcoming.
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert_eq!(app.section, Section::Notes);
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(view_model::editor(&app).unwrap().note_id, 1);
}

#[test]
fn add_task_appends_to_existing_inbox() {
    let inbox = note(9, "Inbox", "- [ ] old thing\n", 3, 1);
    let mut app = todo_app(vec![inbox]);
    ctrl(&mut app, 'n');
    assert_eq!(app.focus, Focus::Input(locus_core::InputKind::TaskText));
    type_str(&mut app, "Renew passport due:2026-11-02");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 9,
            body: "- [ ] old thing\n- [ ] Renew passport 📅 2026-11-02\n".into(),
            expected_revision: 3,
        }]
    );
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn add_task_creates_inbox_when_missing() {
    let mut app = todo_app(vec![]);
    ctrl(&mut app, 'n');
    type_str(&mut app, "Water the fern");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(
        fx,
        vec![Effect::CreateNote {
            nb: nb(),
            folder: 10,
            title: "Inbox".into(),
            body: "- [ ] Water the fern\n".into(),
        }]
    );
}

#[test]
fn add_task_waits_for_an_unloaded_inbox_body() {
    // Inbox is listed but its body was not fetched.
    let mut app = todo_app(vec![note(9, "Inbox", "", 2, 1)]);
    ctrl(&mut app, 'n');
    type_str(&mut app, "Pay rent");
    let fx = press(&mut app, Key::Enter);
    assert_eq!(fx, vec![Effect::LoadNote { nb: nb(), id: 9 }]);
    // The body arrives; the pending task is appended and saved.
    let fx = send(
        &mut app,
        Msg::NoteLoaded {
            nb: nb(),
            note: note(9, "Inbox", "- [ ] Water\n", 2, 1),
        },
    );
    assert_eq!(
        fx,
        vec![Effect::SaveNote {
            nb: nb(),
            id: 9,
            body: "- [ ] Water\n- [ ] Pay rent\n".into(),
            expected_revision: 2,
        }]
    );
}

#[test]
fn add_task_rejects_empty_text_and_bad_due_dates() {
    let mut app = todo_app(vec![]);
    ctrl(&mut app, 'n');
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.status.level, locus_core::Level::Error);
    type_str(&mut app, "due:2026-13-01");
    assert!(press(&mut app, Key::Enter).is_empty());
    assert_eq!(app.focus, Focus::Input(locus_core::InputKind::TaskText));
    assert!(app.status.text.contains("YYYY-MM-DD"));
}

#[test]
fn entering_todo_fetches_bodies_that_are_missing() {
    // Notes listed without bodies: entering Todo asks for each body.
    let mut app = loaded_notes(vec![note(1, "A", "", 1, 1), note(2, "B", "- [ ] x", 1, 2)]);
    let fx = send(&mut app, Msg::Switch(Section::Todo));
    assert_eq!(fx, vec![Effect::LoadNote { nb: nb(), id: 1 }]);
    let fx = send(
        &mut app,
        Msg::NoteLoaded {
            nb: nb(),
            note: note(1, "A", "- [ ] y", 1, 1),
        },
    );
    assert!(fx.is_empty());
    assert_eq!(view_model::todo_groups(&app)[3].items.len(), 2);
}

#[test]
fn toggle_on_no_task_does_nothing() {
    let mut app = loaded_notes(vec![note(1, "Empty", "just prose", 1, 1)]);
    send(&mut app, Msg::Switch(Section::Todo));
    assert!(press(&mut app, Key::Char(' ')).is_empty());
    assert_eq!(app.focus, Focus::List);
}
