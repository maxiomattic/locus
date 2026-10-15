//! Output helpers: aligned tables, JSON, timestamps and error text.

use locus_core::date::{local_date_of, local_hm_of};
use serde::Serialize;
use std::io::Write;

/// Write `s` to stdout, ignoring a closed pipe (`locus ... | head`).
pub fn emit(s: &str) {
    let _ = std::io::stdout().write_all(s.as_bytes());
}

/// Print `value` as pretty JSON.
pub fn json<T: Serialize>(value: &T) {
    emit(&serde_json::to_string_pretty(value).unwrap_or_default());
    emit("\n");
}

/// Header plus rows, columns padded to the widest cell and separated by two
/// spaces. The last column is not padded. Cells are single-line.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(cols) {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |cells: &[String]| {
        let mut out = String::new();
        for (i, cell) in cells.iter().enumerate().take(cols) {
            out.push_str(cell);
            if i + 1 < cols {
                out.push_str(&" ".repeat(widths[i] - cell.chars().count() + 2));
            }
        }
        out.trim_end().to_string() + "\n"
    };
    let head: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
    let mut out = line(&head);
    for row in rows {
        out.push_str(&line(row));
    }
    out
}

/// Collapse whitespace and newlines so a value fits on one table row.
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `s` cut to at most `max` characters, ending in `…` when cut.
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// `YYYY-MM-DD HH:MM` in the zone `tz` (seconds east of UTC); empty for 0.
pub fn stamp(unix: u64, tz: i32) -> String {
    if unix == 0 {
        return String::new();
    }
    let (h, m) = local_hm_of(unix as i64, tz);
    format!("{} {h:02}:{m:02}", local_date_of(unix as i64, tz).iso())
}

/// Error text for a client failure, with a hint for stale writes.
pub fn client_error(e: locus_client::Error) -> String {
    match e {
        locus_client::Error::Conflict { current } => {
            let now = current
                .map(|r| format!(" (now at revision {r})"))
                .unwrap_or_default();
            format!("conflict: it changed on the server{now}; re-read and retry")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_aligns_columns() {
        let rows = vec![
            vec!["1".into(), "alpha".into()],
            vec!["200".into(), "b".into()],
        ];
        assert_eq!(
            table(&["ID", "NAME"], &rows),
            "ID   NAME\n1    alpha\n200  b\n"
        );
    }

    #[test]
    fn table_counts_chars_not_bytes() {
        let rows = vec![vec!["é".into(), "x".into()], vec!["ab".into(), "y".into()]];
        assert_eq!(table(&["A", "B"], &rows), "A   B\né   x\nab  y\n");
    }

    #[test]
    fn stamp_uses_zone() {
        assert_eq!(stamp(0, 0), "");
        assert_eq!(stamp(1_767_225_600, 0), "2026-01-01 00:00");
        assert_eq!(stamp(1_767_225_600, 3600), "2026-01-01 01:00");
    }

    #[test]
    fn clip_marks_cuts() {
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(clip("abcd", 3), "ab…");
    }

    #[test]
    fn one_line_collapses() {
        assert_eq!(one_line(" a\n b\t c "), "a b c");
    }
}
