//! Calendar section: agenda and month views over canonical events, plus the
//! inline event form. Times are unix seconds; local wall-clock time goes
//! through the `tz_offset_secs` of the caller's [`Now`].

use locus_client::calendar::{Event, EventPatch};

use crate::date::{Date, local_date_of, local_hm_of, local_to_unix, parse_hm};
use crate::msg::{Effect, Msg, Nav, Now};
use crate::state::{
    App, CalMode, Focus, Form, FormField, FormKind, Section, clamp_sel, matches_filter,
};
use crate::update::step;

/// Events on `day` in the local zone, in start order, after the filter.
pub fn events_on(app: &App, day: Date) -> Vec<&Event> {
    let tz = app.now.tz_offset_secs;
    let mut v: Vec<&Event> = app
        .calendar
        .events
        .iter()
        .filter(|e| local_date_of(e.start as i64, tz) == day)
        .filter(|e| matches_filter(&app.filter, &[&e.title, &e.body]))
        .collect();
    v.sort_by(|a, b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
    v
}

pub fn selected_event(app: &App) -> Option<&Event> {
    events_on(app, app.calendar.day)
        .get(app.calendar.sel)
        .copied()
}

pub fn clamp(app: &mut App) {
    let len = events_on(app, app.calendar.day).len();
    app.calendar.sel = clamp_sel(app.calendar.sel, len);
}

pub fn toggle_mode(app: &mut App) {
    app.calendar.mode = match app.calendar.mode {
        CalMode::Agenda => CalMode::Month,
        CalMode::Month => CalMode::Agenda,
    };
}

pub fn shift_month(app: &mut App, months: i32) {
    app.calendar.day = app.calendar.day.add_months(months);
    app.calendar.sel = 0;
}

pub fn go_today(app: &mut App, now: Now) {
    app.calendar.day = now.today;
    app.calendar.sel = 0;
}

pub fn nav(app: &mut App, nav: Nav) {
    match nav {
        Nav::Left => {
            app.calendar.day = app.calendar.day.add_days(-1);
            app.calendar.sel = 0;
        }
        Nav::Right => {
            app.calendar.day = app.calendar.day.add_days(1);
            app.calendar.sel = 0;
        }
        other => {
            let len = events_on(app, app.calendar.day).len();
            app.calendar.sel = step(app.calendar.sel, len, other);
        }
    }
}

pub fn events_loaded(app: &mut App, events: Vec<Event>) {
    app.calendar.events = events;
    app.loading[Section::Calendar.index()] = false;
    app.loaded[Section::Calendar.index()] = true;
    clamp(app);
}

pub fn event_saved(app: &mut App, event: Event) {
    let tz = app.now.tz_offset_secs;
    let day = local_date_of(event.start as i64, tz);
    let id = event.id;
    match app.calendar.events.iter_mut().find(|e| e.id == id) {
        Some(slot) => *slot = event,
        None => app.calendar.events.push(event),
    }
    if is_event_form(app) {
        app.form = None;
        app.focus = Focus::List;
    }
    app.calendar.day = day;
    app.calendar.sel = events_on(app, day)
        .iter()
        .position(|e| e.id == id)
        .unwrap_or(0);
    app.info("Saved event");
}

pub fn event_deleted(app: &mut App, id: u64) {
    app.calendar.events.retain(|e| e.id != id);
    clamp(app);
    app.info("Event deleted");
}

fn is_event_form(app: &App) -> bool {
    matches!(
        app.form.as_ref().map(|f| &f.kind),
        Some(FormKind::NewEvent | FormKind::EditEvent { .. })
    )
}

fn field(label: &'static str, value: impl Into<String>) -> FormField {
    FormField {
        label,
        value: value.into(),
    }
}

fn open_form(app: &mut App, kind: FormKind, fields: Vec<FormField>) {
    app.form = Some(Form {
        kind,
        fields,
        active: 0,
        error: None,
    });
    app.focus = Focus::Form;
}

pub fn new_event(app: &mut App) {
    let day = app.calendar.day.iso();
    open_form(
        app,
        FormKind::NewEvent,
        vec![
            field("title", ""),
            field("date", day),
            field("start", "09:00"),
            field("end", "10:00"),
            field("notes", ""),
        ],
    );
}

pub fn edit_selected(app: &mut App) {
    let Some(ev) = selected_event(app).cloned() else {
        app.info("No event selected");
        return;
    };
    let tz = app.now.tz_offset_secs;
    let (sh, sm) = local_hm_of(ev.start as i64, tz);
    let (eh, em) = local_hm_of(ev.end as i64, tz);
    let date = local_date_of(ev.start as i64, tz);
    open_form(
        app,
        FormKind::EditEvent {
            id: ev.id,
            revision: ev.revision,
        },
        vec![
            field("title", ev.title),
            field("date", date.iso()),
            field("start", format!("{sh:02}:{sm:02}")),
            field("end", format!("{eh:02}:{em:02}")),
            field("notes", ev.body),
        ],
    );
}

pub fn ask_delete(app: &mut App) {
    let Some(ev) = selected_event(app).map(|e| (e.id, e.title.clone())) else {
        app.info("No event selected");
        return;
    };
    app.ask(
        format!("Delete event \"{}\"?", ev.1),
        Msg::ExecuteDelete {
            section: Section::Calendar,
            id: ev.0,
        },
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    fx.push(Effect::DeleteEvent { id });
    app.info("Deleting event");
}

/// A validated event form: title, unix start and end seconds, notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEvent {
    pub title: String,
    pub start: u64,
    pub end: u64,
    pub body: String,
}

/// Validate the form. Errors name the field that is wrong.
pub fn parse_form(form: &Form, tz_offset_secs: i32) -> Result<ParsedEvent, String> {
    let title = form.value("title").trim();
    if title.is_empty() {
        return Err("Title is required".into());
    }
    let date = Date::parse(form.value("date").trim()).ok_or("Date must be YYYY-MM-DD")?;
    let (sh, sm) = parse_hm(form.value("start")).ok_or("Start must be HH:MM")?;
    let (eh, em) = parse_hm(form.value("end")).ok_or("End must be HH:MM")?;
    let start = local_to_unix(date, sh, sm, tz_offset_secs);
    let end = local_to_unix(date, eh, em, tz_offset_secs);
    if end <= start {
        return Err("End must be after start".into());
    }
    let to_u64 = |v: i64| u64::try_from(v).map_err(|_| "Date is out of range".to_string());
    Ok(ParsedEvent {
        title: title.to_string(),
        start: to_u64(start)?,
        end: to_u64(end)?,
        body: form.value("notes").to_string(),
    })
}

pub fn submit_form(app: &mut App, now: Now, fx: &mut Vec<Effect>) {
    let Some(form) = app.form.clone() else {
        return;
    };
    let parsed = match parse_form(&form, now.tz_offset_secs) {
        Ok(p) => p,
        Err(e) => {
            if let Some(f) = app.form.as_mut() {
                f.error = Some(e.clone());
            }
            app.error(e);
            return;
        }
    };
    match form.kind {
        FormKind::NewEvent => fx.push(Effect::CreateEvent {
            title: parsed.title,
            start: parsed.start,
            end: parsed.end,
            body: parsed.body,
        }),
        FormKind::EditEvent { id, revision } => fx.push(Effect::PatchEvent {
            id,
            expected_revision: revision,
            patch: EventPatch {
                title: Some(parsed.title),
                start: Some(parsed.start),
                end: Some(parsed.end),
                body: Some(parsed.body),
            },
        }),
        FormKind::NewContact | FormKind::EditContact { .. } => return,
    }
    if let Some(f) = app.form.as_mut() {
        f.error = None;
    }
    app.info("Saving event");
}
