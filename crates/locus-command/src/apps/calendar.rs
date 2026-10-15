//! `locus calendar`: events. Times are local wall-clock time; the server
//! stores Unix seconds. The event model has a title, a start, an end and
//! notes (no location or all-day flag).

use locus_client::calendar::{Event, EventPatch};
use locus_core::date::{Date, local_date_of, local_hm_of, local_to_unix, parse_hm};
use serde_json::json;

use super::Ctx;
use super::args::{Parsed, parse_id};
use super::fmt::{self, client_error, emit, one_line, stamp, table};

const DAY: i64 = 86_400;

/// Help for `locus calendar --help`.
pub const HELP: &str = "locus calendar <command>

  ls [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--json]   list events (local dates, inclusive)
  add <title> --start <when> [--end <when>] [--notes <text>]
  edit <id> [--title <t>] [--start <when>] [--end <when>] [--notes <text>]
  rm <id>

<when> is YYYY-MM-DD, YYYY-MM-DDTHH:MM, or (for --end) HH:MM on the start day.
add: a start without a time is a whole day (00:00 to the next midnight, or
through the end date); a start with a time lasts an hour unless --end is given.
edit: a date without a time keeps the event's time of day; without --end the
event keeps its length.
";

/// A point in time as typed on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// `YYYY-MM-DD`
    Date(Date),
    /// `YYYY-MM-DDTHH:MM`
    DateTime(Date, u8, u8),
    /// `HH:MM`, on the start's day.
    Time(u8, u8),
}

/// Parse `YYYY-MM-DD`, `YYYY-MM-DDTHH:MM` (or with a space) or `HH:MM`.
pub fn parse_moment(s: &str) -> Result<Moment, String> {
    let bad = || format!("bad date/time (want YYYY-MM-DD[THH:MM] or HH:MM): {s}");
    let t = s.trim();
    if let Some(d) = Date::parse(t) {
        return Ok(Moment::Date(d));
    }
    if let Some((h, m)) = parse_hm(t) {
        return Ok(Moment::Time(h, m));
    }
    let (d, hm) = t.split_once(['T', ' ']).ok_or_else(bad)?;
    let date = Date::parse(d).ok_or_else(bad)?;
    let (h, m) = parse_hm(hm).ok_or_else(bad)?;
    Ok(Moment::DateTime(date, h, m))
}

fn unix(date: Date, h: u8, m: u8, tz: i32) -> Result<i64, String> {
    let v = local_to_unix(date, h, m, tz);
    if v < 0 {
        Err("date is out of range".into())
    } else {
        Ok(v)
    }
}

/// Start and end Unix seconds for a new event. See [`HELP`] for the rules.
pub fn resolve_new(start: Moment, end: Option<Moment>, tz: i32) -> Result<(u64, u64), String> {
    let (s, e) = match (start, end) {
        (Moment::Time(..), _) => return Err("--start needs a date".into()),
        (Moment::Date(d), None) => (unix(d, 0, 0, tz)?, unix(d, 0, 0, tz)? + DAY),
        (Moment::Date(d), Some(Moment::Date(last))) => {
            (unix(d, 0, 0, tz)?, unix(last, 0, 0, tz)? + DAY)
        }
        (Moment::Date(d), Some(Moment::Time(h, m))) => (unix(d, 0, 0, tz)?, unix(d, h, m, tz)?),
        (Moment::Date(d), Some(Moment::DateTime(ed, h, m))) => {
            (unix(d, 0, 0, tz)?, unix(ed, h, m, tz)?)
        }
        (Moment::DateTime(d, h, m), None) => {
            let s = unix(d, h, m, tz)?;
            (s, s + 3600)
        }
        (Moment::DateTime(..), Some(Moment::Date(_))) => {
            return Err("--end needs a time when --start has one".into());
        }
        (Moment::DateTime(d, h, m), Some(Moment::Time(eh, em))) => {
            (unix(d, h, m, tz)?, unix(d, eh, em, tz)?)
        }
        (Moment::DateTime(d, h, m), Some(Moment::DateTime(ed, eh, em))) => {
            (unix(d, h, m, tz)?, unix(ed, eh, em, tz)?)
        }
    };
    if e <= s {
        return Err("end must be after start".into());
    }
    Ok((s as u64, e as u64))
}

/// New start and end for an edit of `old`. See [`HELP`] for the rules.
pub fn resolve_edit(
    old: &Event,
    start: Option<Moment>,
    end: Option<Moment>,
    tz: i32,
) -> Result<(u64, u64), String> {
    let old_start = old.start as i64;
    let old_end = old.end as i64;
    let (osd, (osh, osm)) = (local_date_of(old_start, tz), local_hm_of(old_start, tz));
    let (oeh, oem) = local_hm_of(old_end, tz);
    let (s_date, s_h, s_m) = match start {
        None => (osd, osh, osm),
        Some(Moment::Date(d)) => (d, osh, osm),
        Some(Moment::DateTime(d, h, m)) => (d, h, m),
        Some(Moment::Time(h, m)) => (osd, h, m),
    };
    let s = unix(s_date, s_h, s_m, tz)?;
    let e = match end {
        None => s + (old_end - old_start),
        Some(Moment::Date(d)) => unix(d, oeh, oem, tz)?,
        Some(Moment::DateTime(d, h, m)) => unix(d, h, m, tz)?,
        Some(Moment::Time(h, m)) => unix(s_date, h, m, tz)?,
    };
    if e <= s {
        return Err("end must be after start".into());
    }
    Ok((s as u64, e as u64))
}

/// Whether `ev` overlaps the local dates `from..=to` (either may be open).
pub fn in_range(ev: &Event, from: Option<Date>, to: Option<Date>, tz: i32) -> bool {
    let lo = from.map_or(i64::MIN, |d| local_to_unix(d, 0, 0, tz));
    let hi = to.map_or(i64::MAX, |d| local_to_unix(d, 0, 0, tz) + DAY);
    (ev.end as i64) > lo && (ev.start as i64) < hi
}

/// Run one `locus calendar` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "add" => add(ctx, args).await,
        "edit" => edit(ctx, args).await,
        "rm" => rm(ctx, args).await,
        other => Err(format!(
            "unknown calendar command {other}. try: locus calendar --help"
        )),
    }
}

fn parse_date(s: &str) -> Result<Date, String> {
    Date::parse(s.trim()).ok_or_else(|| format!("date must be YYYY-MM-DD: {s}"))
}

/// The end column: just the time when it is on the start's day.
fn end_text(ev: &Event, tz: i32) -> String {
    let same_day = local_date_of(ev.start as i64, tz) == local_date_of(ev.end as i64, tz);
    let full = stamp(ev.end, tz);
    if same_day {
        full.split_once(' ')
            .map(|x| x.1.to_string())
            .unwrap_or(full)
    } else {
        full
    }
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["--from", "--to"], &["--json"])?;
    p.positionals(
        0,
        0,
        "locus calendar ls [--from YYYY-MM-DD] [--to YYYY-MM-DD] [--json]",
    )?;
    let from = p.value("--from").map(parse_date).transpose()?;
    let to = p.value("--to").map(parse_date).transpose()?;
    let mut events = ctx
        .client()
        .await?
        .list_events()
        .await
        .map_err(client_error)?;
    events.retain(|e| in_range(e, from, to, ctx.tz));
    events.sort_by_key(|e| (e.start, e.id));
    if p.has("--json") {
        let out: Vec<_> = events
            .iter()
            .map(|e| {
                json!({"id": e.id, "title": e.title, "start": e.start, "end": e.end,
                       "notes": e.body, "calendar": e.calendar, "revision": e.revision})
            })
            .collect();
        fmt::json(&out);
    } else {
        let rows: Vec<Vec<String>> = events
            .iter()
            .map(|e| {
                vec![
                    e.id.to_string(),
                    stamp(e.start, ctx.tz),
                    end_text(e, ctx.tz),
                    one_line(&e.title),
                ]
            })
            .collect();
        emit(&table(&["ID", "START", "END", "TITLE"], &rows));
    }
    Ok(())
}

async fn add(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let usage = "locus calendar add <title> --start <when> [--end <when>] [--notes <text>]";
    let p = Parsed::parse(&args, &["--start", "--end", "--notes"], &[])?;
    let title = &p.positionals(1, 1, usage)?[0];
    if title.trim().is_empty() {
        return Err("title is required".into());
    }
    let start = parse_moment(p.value("--start").ok_or(format!("usage: {usage}"))?)?;
    let end = p.value("--end").map(parse_moment).transpose()?;
    let (s, e) = resolve_new(start, end, ctx.tz)?;
    let ev = ctx
        .client()
        .await?
        .create_event(title.trim(), s, e, p.value("--notes").unwrap_or(""))
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", ev.id));
    Ok(())
}

async fn edit(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let usage =
        "locus calendar edit <id> [--title <t>] [--start <when>] [--end <when>] [--notes <text>]";
    let p = Parsed::parse(&args, &["--title", "--start", "--end", "--notes"], &[])?;
    let id = parse_id(&p.positionals(1, 1, usage)?[0], "event")?;
    let client = ctx.client().await?;
    let old = client
        .list_events()
        .await
        .map_err(client_error)?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("no event {id}"))?;
    let start = p.value("--start").map(parse_moment).transpose()?;
    let end = p.value("--end").map(parse_moment).transpose()?;
    let mut patch = EventPatch {
        title: p.value("--title").map(|t| t.trim().to_string()),
        body: p.value("--notes").map(str::to_string),
        ..Default::default()
    };
    if patch.title.as_deref() == Some("") {
        return Err("title is required".into());
    }
    if start.is_some() || end.is_some() {
        let (s, e) = resolve_edit(&old, start, end, ctx.tz)?;
        patch.start = Some(s);
        patch.end = Some(e);
    }
    if patch == EventPatch::default() {
        return Err(format!("nothing to change\nusage: {usage}"));
    }
    client
        .patch_event(id, old.revision, &patch)
        .await
        .map_err(client_error)?;
    emit(&format!("updated {id}\n"));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let id = parse_id(&p.positionals(1, 1, "locus calendar rm <id>")?[0], "event")?;
    ctx.client()
        .await?
        .delete_event(id, None)
        .await
        .map_err(client_error)?;
    emit(&format!("deleted {id}\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> Date {
        Date::new(y, m, day).unwrap()
    }

    fn event(start: u64, end: u64) -> Event {
        Event {
            id: 1,
            calendar: String::new(),
            title: "t".into(),
            start,
            end,
            body: String::new(),
            revision: 3,
        }
    }

    #[test]
    fn moments_parse() {
        assert_eq!(parse_moment("2026-03-04"), Ok(Moment::Date(d(2026, 3, 4))));
        assert_eq!(
            parse_moment("2026-03-04T09:30"),
            Ok(Moment::DateTime(d(2026, 3, 4), 9, 30))
        );
        assert_eq!(
            parse_moment("2026-03-04 9:05"),
            Ok(Moment::DateTime(d(2026, 3, 4), 9, 5))
        );
        assert_eq!(parse_moment("17:00"), Ok(Moment::Time(17, 0)));
        assert!(parse_moment("2026-03-04T25:00").is_err());
        assert!(parse_moment("2026-02-30").is_err());
        assert!(parse_moment("soon").is_err());
    }

    #[test]
    fn new_event_with_time_defaults_to_an_hour() {
        let start = Moment::DateTime(d(2026, 1, 1), 9, 0);
        let (s, e) = resolve_new(start, None, 0).unwrap();
        assert_eq!(s, 1_767_258_000);
        assert_eq!(e - s, 3600);
        let (_, e) = resolve_new(start, Some(Moment::Time(10, 30)), 0).unwrap();
        assert_eq!(e - s, 5400);
    }

    #[test]
    fn new_whole_day_events() {
        let (s, e) = resolve_new(Moment::Date(d(2026, 1, 1)), None, 0).unwrap();
        assert_eq!((s, e - s), (1_767_225_600, 86_400));
        let (s, e) = resolve_new(
            Moment::Date(d(2026, 1, 1)),
            Some(Moment::Date(d(2026, 1, 3))),
            0,
        )
        .unwrap();
        assert_eq!(e - s, 3 * 86_400);
    }

    #[test]
    fn new_event_respects_zone_and_rejects_bad_ranges() {
        let start = Moment::DateTime(d(2026, 1, 1), 1, 0);
        let (s, _) = resolve_new(start, None, 3600).unwrap();
        assert_eq!(s, 1_767_225_600);
        assert!(resolve_new(start, Some(Moment::Time(0, 30)), 0).is_err());
        assert!(resolve_new(start, Some(Moment::Date(d(2026, 1, 2))), 0).is_err());
        assert!(resolve_new(Moment::Time(9, 0), None, 0).is_err());
    }

    #[test]
    fn edit_keeps_length_and_time_of_day() {
        // 2026-01-01 09:00 to 10:30 UTC.
        let old = event(1_767_258_000, 1_767_263_400);
        let (s, e) = resolve_edit(&old, Some(Moment::Date(d(2026, 1, 5))), None, 0).unwrap();
        assert_eq!(s, 1_767_258_000 + 4 * 86_400);
        assert_eq!(e - s, 5400);
        let (s, e) =
            resolve_edit(&old, Some(Moment::DateTime(d(2026, 1, 1), 14, 0)), None, 0).unwrap();
        assert_eq!(e - s, 5400);
        assert_eq!(s, 1_767_258_000 + 5 * 3600);
    }

    #[test]
    fn edit_end_only() {
        let old = event(1_767_258_000, 1_767_263_400);
        let (s, e) = resolve_edit(&old, None, Some(Moment::Time(12, 0)), 0).unwrap();
        assert_eq!(s, 1_767_258_000);
        assert_eq!(e - s, 3 * 3600);
        assert!(resolve_edit(&old, None, Some(Moment::Time(8, 0)), 0).is_err());
    }

    #[test]
    fn range_filter_is_inclusive_of_whole_days() {
        let ev = event(1_767_258_000, 1_767_261_600); // 2026-01-01 09:00-10:00
        assert!(in_range(&ev, Some(d(2026, 1, 1)), Some(d(2026, 1, 1)), 0));
        assert!(!in_range(&ev, Some(d(2026, 1, 2)), None, 0));
        assert!(!in_range(&ev, None, Some(d(2025, 12, 31)), 0));
        assert!(in_range(&ev, None, None, 0));
    }
}
