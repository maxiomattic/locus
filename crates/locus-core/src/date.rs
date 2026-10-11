//! Calendar dates and local/UTC conversion. Pure arithmetic on top of `time`.
//!
//! `tz_offset_secs` everywhere in this crate means "local = utc + offset"
//! (east of UTC is positive), the same sign convention as `UtcOffset`.

use serde::{Deserialize, Serialize};
use time::Month;

const UNIX_EPOCH_JULIAN: i32 = 2_440_588;
const DAY_SECS: i64 = 86_400;

/// A proleptic Gregorian calendar date. Ordering is chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Date {
    pub y: i32,
    pub m: u8,
    pub d: u8,
}

impl Date {
    /// Validated constructor. `None` for impossible dates such as Feb 30.
    pub fn new(y: i32, m: u8, d: u8) -> Option<Date> {
        let td = time::Date::from_calendar_date(y, Month::try_from(m).ok()?, d).ok()?;
        Some(Date::from_time(td))
    }

    /// Strict `YYYY-MM-DD`.
    pub fn parse(s: &str) -> Option<Date> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let digits = |r: std::ops::Range<usize>| s.get(r)?.parse::<u32>().ok();
        let y = digits(0..4)? as i32;
        let m = digits(5..7)? as u8;
        let d = digits(8..10)? as u8;
        Date::new(y, m, d)
    }

    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }

    fn from_time(td: time::Date) -> Date {
        Date {
            y: td.year(),
            m: td.month() as u8,
            d: td.day(),
        }
    }

    fn to_time(self) -> Option<time::Date> {
        time::Date::from_calendar_date(self.y, Month::try_from(self.m).ok()?, self.d).ok()
    }

    /// Days since 1970-01-01 (negative before the epoch).
    pub fn unix_days(self) -> i64 {
        self.to_time()
            .map(|t| i64::from(t.to_julian_day()) - i64::from(UNIX_EPOCH_JULIAN))
            .unwrap_or(0)
    }

    pub fn from_unix_days(days: i64) -> Option<Date> {
        let julian = i32::try_from(days + i64::from(UNIX_EPOCH_JULIAN)).ok()?;
        time::Date::from_julian_day(julian)
            .ok()
            .map(Date::from_time)
    }

    /// Monday = 0 .. Sunday = 6.
    pub fn weekday_mon0(self) -> u8 {
        self.to_time()
            .map(|t| t.weekday().number_days_from_monday())
            .unwrap_or(0)
    }

    pub fn add_days(self, n: i64) -> Date {
        Date::from_unix_days(self.unix_days() + n).unwrap_or(self)
    }

    /// Add months, clamping the day to the length of the target month.
    pub fn add_months(self, n: i32) -> Date {
        let total = self.y * 12 + i32::from(self.m) - 1 + n;
        let y = total.div_euclid(12);
        let m = (total.rem_euclid(12) + 1) as u8;
        let d = self.d.min(days_in_month(y, m));
        Date { y, m, d }
    }

    pub fn first_of_month(self) -> Date {
        Date {
            y: self.y,
            m: self.m,
            d: 1,
        }
    }
}

pub fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Unix seconds for local wall-clock `date hh:mm` in the zone `tz_offset_secs`.
pub fn local_to_unix(date: Date, hh: u8, mm: u8, tz_offset_secs: i32) -> i64 {
    date.unix_days() * DAY_SECS + i64::from(hh) * 3600 + i64::from(mm) * 60
        - i64::from(tz_offset_secs)
}

/// The local calendar date containing the instant `unix_secs`.
pub fn local_date_of(unix_secs: i64, tz_offset_secs: i32) -> Date {
    let local = unix_secs + i64::from(tz_offset_secs);
    Date::from_unix_days(local.div_euclid(DAY_SECS)).unwrap_or(Date {
        y: 1970,
        m: 1,
        d: 1,
    })
}

/// Local `HH:MM` of the instant `unix_secs`.
pub fn local_hm_of(unix_secs: i64, tz_offset_secs: i32) -> (u8, u8) {
    let local = (unix_secs + i64::from(tz_offset_secs)).rem_euclid(DAY_SECS);
    ((local / 3600) as u8, ((local % 3600) / 60) as u8)
}

/// Parse `HH:MM` (24-hour, zero padded or not).
pub fn parse_hm(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_bad_dates() {
        assert_eq!(Date::parse("2026-10-11"), Date::new(2026, 10, 11));
        assert_eq!(Date::parse("2026-02-30"), None);
        assert_eq!(Date::parse("2026-1-11"), None);
        assert_eq!(Date::parse("2024-02-29"), Date::new(2024, 2, 29));
        assert_eq!(Date::parse("2023-02-29"), None);
        assert_eq!(Date::parse("2026-10-11x"), None);
    }

    #[test]
    fn weekday_is_monday_first() {
        // 2026-10-11 is a Sunday, 2026-10-12 a Monday.
        assert_eq!(Date::new(2026, 10, 11).unwrap().weekday_mon0(), 6);
        assert_eq!(Date::new(2026, 10, 12).unwrap().weekday_mon0(), 0);
        assert_eq!(Date::new(1970, 1, 1).unwrap().weekday_mon0(), 3);
    }

    #[test]
    fn add_months_clamps_day() {
        let jan31 = Date::new(2026, 1, 31).unwrap();
        assert_eq!(jan31.add_months(1), Date::new(2026, 2, 28).unwrap());
        assert_eq!(jan31.add_months(-2), Date::new(2025, 11, 30).unwrap());
        assert_eq!(jan31.add_months(13), Date::new(2027, 2, 28).unwrap());
    }

    #[test]
    fn add_days_crosses_year_and_leap_day() {
        let d = Date::new(2024, 2, 28).unwrap();
        assert_eq!(d.add_days(1), Date::new(2024, 2, 29).unwrap());
        assert_eq!(d.add_days(2), Date::new(2024, 3, 1).unwrap());
        assert_eq!(d.add_days(-59), Date::new(2023, 12, 31).unwrap());
    }

    #[test]
    fn unix_conversions_respect_offset() {
        // 2026-10-11 10:30 at UTC+2 is 08:30 UTC.
        let d = Date::new(2026, 10, 11).unwrap();
        let utc = local_to_unix(d, 10, 30, 2 * 3600);
        assert_eq!(utc, local_to_unix(d, 8, 30, 0));
        assert_eq!(local_date_of(utc, 2 * 3600), d);
        assert_eq!(local_hm_of(utc, 2 * 3600), (10, 30));
        // Just before local midnight at UTC-5 is still the previous UTC day.
        let late = local_to_unix(d, 23, 59, -5 * 3600);
        assert_eq!(local_date_of(late, -5 * 3600), d);
        assert_eq!(local_date_of(late + 60, -5 * 3600), d.add_days(1));
    }

    #[test]
    fn epoch_is_day_zero() {
        assert_eq!(Date::new(1970, 1, 1).unwrap().unix_days(), 0);
        assert_eq!(
            local_to_unix(Date::new(1970, 1, 2).unwrap(), 0, 0, 0),
            86_400
        );
    }

    #[test]
    fn parse_hm_bounds() {
        assert_eq!(parse_hm("09:05"), Some((9, 5)));
        assert_eq!(parse_hm("9:5"), Some((9, 5)));
        assert_eq!(parse_hm("24:00"), None);
        assert_eq!(parse_hm("12:60"), None);
        assert_eq!(parse_hm("noon"), None);
    }
}
