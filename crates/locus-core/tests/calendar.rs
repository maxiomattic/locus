mod common;

use common::*;
use locus_core::date::local_to_unix;
use locus_core::{CalMode, Focus, Key, Msg, view_model};

fn cal_app(tz: i32, events: Vec<locus_core::locus_client::calendar::Event>) -> locus_core::App {
    let mut app = locus_core::App::new();
    let clock = now_at(date(2026, 10, 11), tz);
    locus_core::update(&mut app, Msg::Switch(locus_core::Section::Calendar), clock);
    locus_core::update(&mut app, Msg::EventsLoaded { events }, clock);
    app
}

fn press_at(app: &mut locus_core::App, key: Key, clock: locus_core::Now) -> Vec<Effect> {
    press_with(app, key, locus_core::Mods::NONE, clock)
}

/// Fill the event form: title, date, start, end, notes (Tab between fields).
fn fill_form(app: &mut locus_core::App, clock: locus_core::Now, vals: [&str; 5]) {
    app.form.as_mut().unwrap().active = 0;
    for (i, v) in vals.iter().enumerate() {
        // Clear the prefilled value, then type the new one.
        let current = app.form.as_ref().unwrap().fields[i].value.clone();
        for _ in 0..current.chars().count() {
            press_at(app, Key::Backspace, clock);
        }
        for c in v.chars() {
            press_at(app, Key::Char(c), clock);
        }
        if i < 4 {
            press_at(app, Key::Tab, clock);
        }
    }
}

#[test]
fn new_event_form_defaults_to_selected_day() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    press_at(&mut app, Key::Char('n'), clock);
    let form = view_model::form(&app).unwrap();
    assert_eq!(form.title, "New event");
    let values: Vec<_> = form.fields.iter().map(|f| f.value.clone()).collect();
    assert_eq!(values, ["", "2026-10-11", "09:00", "10:00", ""]);
    assert!(form.fields[0].active);
}

#[test]
fn saving_a_new_event_sends_unix_seconds_for_the_local_zone() {
    // UTC+2: local 10:30 on 2026-10-11 is 08:30 UTC.
    let tz = 2 * 3600;
    let clock = now_at(date(2026, 10, 11), tz);
    let mut app = cal_app(tz, vec![]);
    press_at(&mut app, Key::Char('n'), clock);
    fill_form(
        &mut app,
        clock,
        ["Dentist", "2026-10-12", "10:30", "11:15", "bring card"],
    );
    let fx = locus_core::update(&mut app, Msg::SaveForm, clock);
    let expect_start = local_to_unix(date(2026, 10, 12), 10, 30, tz) as u64;
    let expect_end = local_to_unix(date(2026, 10, 12), 11, 15, tz) as u64;
    assert_eq!(
        expect_start,
        (date(2026, 10, 12).unix_days() * 86_400 + 8 * 3600 + 30 * 60) as u64
    );
    assert_eq!(
        fx,
        vec![Effect::CreateEvent {
            title: "Dentist".into(),
            start: expect_start,
            end: expect_end,
            body: "bring card".into(),
        }]
    );
    // The form stays open until the server confirms.
    assert_eq!(app.focus, Focus::Form);
    let saved = event(3, "Dentist", expect_start, expect_end, 1);
    locus_core::update(&mut app, Msg::EventSaved { event: saved }, clock);
    assert!(app.form.is_none());
    assert_eq!(app.focus, Focus::List);
    assert_eq!(app.calendar.day, date(2026, 10, 12));
}

#[test]
fn form_validation_names_the_bad_field() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    press_at(&mut app, Key::Char('n'), clock);

    // Empty title.
    let fx = locus_core::update(&mut app, Msg::SaveForm, clock);
    assert!(fx.is_empty());
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("Title is required")
    );

    fill_form(&mut app, clock, ["Ok", "2026-02-30", "09:00", "10:00", ""]);
    locus_core::update(&mut app, Msg::SaveForm, clock);
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("Date must be YYYY-MM-DD")
    );

    fill_form(&mut app, clock, ["Ok", "2026-10-11", "9am", "10:00", ""]);
    locus_core::update(&mut app, Msg::SaveForm, clock);
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("Start must be HH:MM")
    );

    fill_form(&mut app, clock, ["Ok", "2026-10-11", "10:00", "10:00", ""]);
    locus_core::update(&mut app, Msg::SaveForm, clock);
    assert_eq!(
        view_model::form(&app).unwrap().error.as_deref(),
        Some("End must be after start")
    );
    assert!(app.status.text.contains("End must be after start"));
}

#[test]
fn enter_moves_between_fields_until_the_last_one_saves() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    press_at(&mut app, Key::Char('n'), clock);
    type_str_at(&mut app, clock, "Lunch");
    let fx = press_at(&mut app, Key::Enter, clock);
    assert!(fx.is_empty());
    assert!(view_model::form(&app).unwrap().fields[1].active);
    // Shift+Tab goes back.
    locus_core::update(&mut app, Msg::PrevField, clock);
    assert!(view_model::form(&app).unwrap().fields[0].active);
}

fn type_str_at(app: &mut locus_core::App, clock: locus_core::Now, s: &str) {
    for c in s.chars() {
        press_at(app, Key::Char(c), clock);
    }
}

#[test]
fn h_and_l_move_days_and_brackets_move_months() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    press_at(&mut app, Key::Char('l'), clock);
    assert_eq!(app.calendar.day, date(2026, 10, 12));
    press_at(&mut app, Key::Char('h'), clock);
    press_at(&mut app, Key::Char('h'), clock);
    assert_eq!(app.calendar.day, date(2026, 10, 10));
    press_at(&mut app, Key::Char(']'), clock);
    assert_eq!(app.calendar.day, date(2026, 11, 10));
    press_at(&mut app, Key::Char('['), clock);
    press_at(&mut app, Key::Char('['), clock);
    assert_eq!(app.calendar.day, date(2026, 9, 10));
    press_at(&mut app, Key::Char('t'), clock);
    assert_eq!(app.calendar.day, date(2026, 10, 11));
}

#[test]
fn month_clamps_the_day_when_moving_between_months() {
    let clock = now_at(date(2026, 1, 31), 0);
    let mut app = cal_app(0, vec![]);
    locus_core::update(&mut app, Msg::Today, clock);
    assert_eq!(app.calendar.day, date(2026, 1, 31));
    press_at(&mut app, Key::Char(']'), clock);
    assert_eq!(app.calendar.day, date(2026, 2, 28));
}

#[test]
fn month_grid_is_six_by_seven_and_starts_on_monday() {
    // 2026-10-01 is a Thursday, so the grid starts on Monday 2026-09-28.
    let app = cal_app(0, vec![]);
    let grid = view_model::month_grid(&app);
    assert_eq!(grid.title, "October 2026");
    let cells: Vec<_> = grid.weeks.iter().flatten().collect();
    assert_eq!(cells.len(), 42);
    assert_eq!(cells[0].date, date(2026, 9, 28));
    assert!(!cells[0].in_month);
    assert_eq!(cells[3].date, date(2026, 10, 1));
    assert!(cells[3].in_month);
    assert_eq!(cells[41].date, date(2026, 11, 8));
    let today: Vec<_> = cells.iter().filter(|c| c.is_today).collect();
    assert_eq!(today.len(), 1);
    assert_eq!(today[0].day, 11);
    assert!(today[0].is_selected);
}

#[test]
fn month_grid_counts_events_per_day() {
    let tz = 0;
    let start = local_to_unix(date(2026, 10, 14), 9, 0, tz) as u64;
    let events = vec![
        event(1, "a", start, start + 3600, 1),
        event(2, "b", start + 7200, start + 10_800, 1),
    ];
    let app = cal_app(tz, events);
    let grid = view_model::month_grid(&app);
    let cell = grid
        .weeks
        .iter()
        .flatten()
        .find(|c| c.date == date(2026, 10, 14))
        .unwrap();
    assert_eq!(cell.event_count, 2);
}

#[test]
fn events_are_matched_to_the_local_day() {
    // 23:30 UTC on Oct 11 is already Oct 12 in UTC+2.
    let tz = 2 * 3600;
    let start = local_to_unix(date(2026, 10, 12), 0, 30, tz) as u64;
    let app = cal_app(tz, vec![event(1, "Late", start, start + 1800, 1)]);
    let on_12 = view_model::agenda(&{
        let mut a = app.clone();
        a.calendar.day = date(2026, 10, 12);
        a
    });
    assert_eq!(on_12[0].events[0].title, "Late");
    assert_eq!(on_12[0].events[0].time_label, "00:30-01:00");
}

#[test]
fn j_and_k_move_between_events_of_the_day() {
    let tz = 0;
    let day = date(2026, 10, 11);
    let a = local_to_unix(day, 9, 0, tz) as u64;
    let b = local_to_unix(day, 13, 0, tz) as u64;
    let clock = now_at(day, tz);
    let mut app = cal_app(
        tz,
        vec![
            event(1, "Standup", a, a + 900, 1),
            event(2, "Lunch", b, b + 3600, 1),
        ],
    );
    press_at(&mut app, Key::Char('j'), clock);
    assert_eq!(app.calendar.sel, 1);
    press_at(&mut app, Key::Char('j'), clock);
    assert_eq!(app.calendar.sel, 1, "stops at the last event");
    let agenda = view_model::agenda(&app);
    assert!(agenda[0].events[1].selected);
}

#[test]
fn agenda_mode_toggles_with_m() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    assert_eq!(app.calendar.mode, CalMode::Agenda);
    press_at(&mut app, Key::Char('m'), clock);
    assert_eq!(app.calendar.mode, CalMode::Month);
}

#[test]
fn edit_prefills_the_form_and_patches_with_revision() {
    let tz = 0;
    let start = local_to_unix(date(2026, 10, 11), 9, 0, tz) as u64;
    let clock = now_at(date(2026, 10, 11), tz);
    let mut app = cal_app(tz, vec![event(4, "Standup", start, start + 900, 6)]);
    press_at(&mut app, Key::Char('e'), clock);
    let form = view_model::form(&app).unwrap();
    assert_eq!(form.title, "Edit event");
    assert_eq!(form.fields[2].value, "09:00");
    assert_eq!(form.fields[3].value, "09:15");
    let fx = locus_core::update(&mut app, Msg::SaveForm, clock);
    assert_eq!(fx.len(), 1);
    match &fx[0] {
        Effect::PatchEvent {
            id,
            expected_revision,
            patch,
        } => {
            assert_eq!((*id, *expected_revision), (4, 6));
            assert_eq!(patch.title.as_deref(), Some("Standup"));
            assert_eq!(patch.end, Some(start + 900));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn delete_event_confirms_first() {
    let tz = 0;
    let start = local_to_unix(date(2026, 10, 11), 9, 0, tz) as u64;
    let clock = now_at(date(2026, 10, 11), tz);
    let mut app = cal_app(tz, vec![event(4, "Standup", start, start + 900, 6)]);
    press_at(&mut app, Key::Char('d'), clock);
    assert_eq!(app.focus, Focus::Confirm);
    let fx = press_at(&mut app, Key::Char('y'), clock);
    assert_eq!(fx, vec![Effect::DeleteEvent { id: 4 }]);
}

#[test]
fn event_filter_matches_titles() {
    let tz = 0;
    let start = local_to_unix(date(2026, 10, 11), 9, 0, tz) as u64;
    let mut app = cal_app(
        tz,
        vec![
            event(1, "Dentist", start, start + 900, 1),
            event(2, "Standup", start + 1000, start + 1900, 1),
        ],
    );
    app.filter = "dent".into();
    let agenda = view_model::agenda(&app);
    assert_eq!(agenda[0].events.len(), 1);
    assert_eq!(agenda[0].events[0].title, "Dentist");
}

#[test]
fn bad_form_does_not_emit_anything_on_edit_of_missing_event() {
    let clock = now_at(date(2026, 10, 11), 0);
    let mut app = cal_app(0, vec![]);
    assert!(press_at(&mut app, Key::Char('e'), clock).is_empty());
    assert!(app.form.is_none());
}
