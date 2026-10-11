//! Markdown task lines inside a note body. Pure string work, no IO.
//!
//! A task line is an optionally indented bullet (`-` or `*`) followed by a
//! checkbox: `- [ ] text` or `- [x] text` (`X` also counts as done). A due
//! date may appear anywhere in the text as `📅 YYYY-MM-DD` or `due:YYYY-MM-DD`.
//!
//! Line numbers are 0-based indexes into `body.split('\n')`.

use std::fmt::Write as _;

const CALENDAR: &str = "📅";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// 0-based line index in the note body.
    pub line: usize,
    pub done: bool,
    /// Line text after the checkbox, without the due-date token.
    pub text: String,
    /// `(year, month, day)` from a valid date token.
    pub due: Option<(i32, u8, u8)>,
}

/// Every task line in `body`, in order.
pub fn parse_tasks(body: &str) -> Vec<Task> {
    body.split('\n')
        .enumerate()
        .filter_map(|(line, text)| parse_line(line, text))
        .collect()
}

/// Flip the checkbox on task line `line` (0-based). Only the single mark byte
/// changes; everything else in the body is kept byte for byte. A line that is
/// not a task, or an index past the end, leaves the body unchanged.
pub fn toggle_task(body: &str, line: usize) -> String {
    let mut out = String::with_capacity(body.len());
    for (i, seg) in body.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match checkbox(seg).filter(|_| i == line) {
            Some(cb) => {
                out.push_str(&seg[..cb.mark]);
                out.push(if cb.done { ' ' } else { 'x' });
                out.push_str(&seg[cb.mark + 1..]);
            }
            None => out.push_str(seg),
        }
    }
    out
}

/// Append `- [ ] text`, with a `📅 YYYY-MM-DD` token when `due` is set. A
/// newline is added first when `body` does not already end with one, and the
/// result always ends with a newline. Newlines in `text` become spaces. The
/// caller passes a real calendar date.
pub fn add_task(body: &str, text: &str, due: Option<(i32, u8, u8)>) -> String {
    let mut out = body.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    let text = text.replace(['\r', '\n'], " ");
    let text = text.trim();
    out.push_str("- [ ]");
    if !text.is_empty() {
        out.push(' ');
        out.push_str(text);
    }
    if let Some((y, m, d)) = due {
        let _ = write!(out, " {CALENDAR} {y:04}-{m:02}-{d:02}");
    }
    out.push('\n');
    out
}

/// Where a checkbox sits in a line. `mark` is the byte index of the inner
/// character (` `, `x` or `X`); `text_start` is the first byte after `]`.
struct Checkbox {
    mark: usize,
    done: bool,
    text_start: usize,
}

fn checkbox(line: &str) -> Option<Checkbox> {
    let indented = line.trim_start_matches([' ', '\t']);
    let after_bullet = indented
        .strip_prefix('-')
        .or_else(|| indented.strip_prefix('*'))?;
    // A bullet needs whitespace after it.
    if !after_bullet.starts_with([' ', '\t']) {
        return None;
    }
    let rest = after_bullet.trim_start_matches([' ', '\t']);
    // Absolute byte offset of `[` inside `line`: `rest` is a suffix of `line`.
    let box_at = line.len() - rest.len();
    let done = match rest.get(..3)? {
        "[ ]" => false,
        "[x]" | "[X]" => true,
        _ => return None,
    };
    let after_box = &rest[3..];
    if !after_box.chars().next().is_none_or(char::is_whitespace) {
        return None;
    }
    Some(Checkbox {
        mark: box_at + 1,
        done,
        text_start: box_at + 3,
    })
}

fn parse_line(line_no: usize, line: &str) -> Option<Task> {
    let cb = checkbox(line)?;
    let raw = line[cb.text_start..].trim();
    let (text, due) = extract_due(raw);
    Some(Task {
        line: line_no,
        done: cb.done,
        text,
        due,
    })
}

/// Find the first valid due token, remove it, and tidy the spacing it leaves.
fn extract_due(raw: &str) -> (String, Option<(i32, u8, u8)>) {
    for (start, _) in raw.char_indices() {
        let at_word_start = start == 0 || raw[..start].ends_with(char::is_whitespace);
        if !at_word_start {
            continue;
        }
        let token = if let Some(rest) = raw[start..].strip_prefix("due:") {
            Some((rest, start + "due:".len()))
        } else if raw[start..].starts_with(CALENDAR) {
            let after_emoji = start + CALENDAR.len();
            let skipped = raw[after_emoji..].len() - raw[after_emoji..].trim_start().len();
            Some((&raw[after_emoji + skipped..], after_emoji + skipped))
        } else {
            None
        };
        let Some((rest, date_at)) = token else {
            continue;
        };
        let Some(date) = rest.get(..10).and_then(parse_date) else {
            continue;
        };
        let end = date_at + 10;
        // The date must end the word: `due:2026-01-021` is not a date.
        if raw[end..].chars().next().is_some_and(char::is_alphanumeric) {
            continue;
        }
        let before = raw[..start].trim_end();
        let after = raw[end..].trim_start();
        let text = match (before.is_empty(), after.is_empty()) {
            (true, _) => after.to_string(),
            (_, true) => before.to_string(),
            _ => format!("{before} {after}"),
        };
        return (text, Some(date));
    }
    (raw.to_string(), None)
}

/// Strict `YYYY-MM-DD` with a real calendar day.
fn parse_date(s: &str) -> Option<(i32, u8, u8)> {
    let b = s.as_bytes();
    if b.len() != 10 {
        return None;
    }
    let digits_ok = b.iter().enumerate().all(|(i, &c)| match i {
        4 | 7 => c == b'-',
        _ => c.is_ascii_digit(),
    });
    if !digits_ok {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u8 = s[5..7].parse().ok()?;
    let d: u8 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some((y, m, d))
}

fn days_in_month(y: i32, m: u8) -> u8 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(body: &str) -> Vec<(usize, bool, String)> {
        parse_tasks(body)
            .into_iter()
            .map(|t| (t.line, t.done, t.text))
            .collect()
    }

    #[test]
    fn parses_open_and_done_boxes() {
        let body = "- [ ] buy milk\n- [x] file taxes\n- [X] ship it\n";
        assert_eq!(
            texts(body),
            vec![
                (0, false, "buy milk".into()),
                (1, true, "file taxes".into()),
                (2, true, "ship it".into()),
            ]
        );
    }

    #[test]
    fn star_bullets_are_tasks() {
        assert_eq!(texts("* [ ] star\n"), vec![(0, false, "star".into())]);
    }

    #[test]
    fn indentation_keeps_line_numbers() {
        let body = "# Heading\n\n  - [ ] two spaces\n\t- [x] tab\n    * [ ] four\nplain\n";
        assert_eq!(
            texts(body),
            vec![
                (2, false, "two spaces".into()),
                (3, true, "tab".into()),
                (4, false, "four".into()),
            ]
        );
    }

    #[test]
    fn non_tasks_are_ignored() {
        let body = concat!(
            "plain text\n",
            "- plain bullet\n",
            "-[ ] no space after bullet\n",
            "- [ ]no space after box\n",
            "1. [ ] numbered\n",
            "- [-] other mark\n",
            "- [] empty box\n",
            "[ ] no bullet\n",
            "- [ ] real\n",
        );
        assert_eq!(texts(body), vec![(8, false, "real".into())]);
    }

    #[test]
    fn empty_task_text_is_allowed() {
        assert_eq!(texts("- [ ]\n"), vec![(0, false, String::new())]);
        assert_eq!(texts("- [ ] \n"), vec![(0, false, String::new())]);
    }

    #[test]
    fn emoji_due_date_is_extracted_and_removed() {
        let tasks = parse_tasks("- [ ] pay rent 📅 2026-10-01\n");
        assert_eq!(tasks[0].text, "pay rent");
        assert_eq!(tasks[0].due, Some((2026, 10, 1)));
    }

    #[test]
    fn colon_due_date_is_extracted() {
        let tasks = parse_tasks("- [ ] call mom due:2026-02-28\n");
        assert_eq!(tasks[0].text, "call mom");
        assert_eq!(tasks[0].due, Some((2026, 2, 28)));
    }

    #[test]
    fn due_date_in_the_middle_joins_text_cleanly() {
        let tasks = parse_tasks("- [ ] write 📅 2026-01-02 report\n");
        assert_eq!(tasks[0].text, "write report");
        let tasks = parse_tasks("- [ ] a due:2026-01-02 b\n");
        assert_eq!(tasks[0].text, "a b");
    }

    #[test]
    fn due_date_at_start_is_removed() {
        let tasks = parse_tasks("- [ ] due:2026-05-05 water plants\n");
        assert_eq!(tasks[0].text, "water plants");
        assert_eq!(tasks[0].due, Some((2026, 5, 5)));
    }

    #[test]
    fn only_the_first_valid_date_is_used() {
        let tasks = parse_tasks("- [ ] x due:2026-01-01 due:2027-02-02\n");
        assert_eq!(tasks[0].due, Some((2026, 1, 1)));
        assert_eq!(tasks[0].text, "x due:2027-02-02");
    }

    #[test]
    fn task_without_date_has_none() {
        let tasks = parse_tasks("- [ ] no date here\n");
        assert_eq!(tasks[0].due, None);
        assert_eq!(tasks[0].text, "no date here");
    }

    #[test]
    fn invalid_dates_are_left_in_text() {
        for token in [
            "due:2026-13-01",
            "due:2026-00-10",
            "due:2026-04-31",
            "due:2023-02-29",
            "due:1900-02-29",
            "due:2026-01-00",
            "due:2026-1-01",
            "due:2026/01/01",
            "📅 2026-01-021",
        ] {
            let tasks = parse_tasks(&format!("- [ ] x {token}\n"));
            assert_eq!(tasks[0].due, None, "token {token}");
            assert!(tasks[0].text.contains(token), "token {token} kept");
        }
    }

    #[test]
    fn leap_days_follow_gregorian_rules() {
        assert_eq!(
            parse_tasks("- [ ] a due:2024-02-29\n")[0].due,
            Some((2024, 2, 29))
        );
        assert_eq!(
            parse_tasks("- [ ] a due:2000-02-29\n")[0].due,
            Some((2000, 2, 29))
        );
        assert_eq!(parse_tasks("- [ ] a due:2100-02-29\n")[0].due, None);
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let tasks = parse_tasks("- [ ] a due:2026-01-01\r\n- [x] b\r\n");
        assert_eq!(tasks[0].text, "a");
        assert_eq!(tasks[0].due, Some((2026, 1, 1)));
        assert_eq!(tasks[1].text, "b");
        assert!(tasks[1].done);
    }

    #[test]
    fn unicode_text_survives() {
        let tasks = parse_tasks("- [ ] café ☕ 📅 2026-03-04 naïve\n");
        assert_eq!(tasks[0].text, "café ☕ naïve");
        assert_eq!(tasks[0].due, Some((2026, 3, 4)));
    }

    #[test]
    fn toggle_flips_only_the_named_line() {
        let body = "- [ ] a\n- [x] b\n- [ ] c\n";
        assert_eq!(toggle_task(body, 0), "- [x] a\n- [x] b\n- [ ] c\n");
        assert_eq!(toggle_task(body, 1), "- [ ] a\n- [ ] b\n- [ ] c\n");
        assert_eq!(toggle_task(body, 2), "- [ ] a\n- [x] b\n- [x] c\n");
    }

    #[test]
    fn toggle_is_an_involution() {
        let body = "# T\n  - [ ] nested 📅 2026-01-01\n- [x] done\n";
        for line in 0..3 {
            assert_eq!(toggle_task(&toggle_task(body, line), line), body);
        }
    }

    #[test]
    fn toggle_preserves_all_other_bytes() {
        let body = "Intro é ☕\r\n\t*  [ ] tabbed   text due:2026-01-01\r\n\n- [x] end";
        let out = toggle_task(body, 1);
        assert_eq!(
            out,
            "Intro é ☕\r\n\t*  [x] tabbed   text due:2026-01-01\r\n\n- [x] end"
        );
        let out = toggle_task(body, 3);
        assert_eq!(
            out,
            "Intro é ☕\r\n\t*  [ ] tabbed   text due:2026-01-01\r\n\n- [ ] end"
        );
    }

    #[test]
    fn toggle_non_task_or_out_of_range_is_noop() {
        let body = "just text\n- [ ] a\n";
        assert_eq!(toggle_task(body, 0), body);
        assert_eq!(toggle_task(body, 99), body);
        assert_eq!(toggle_task("", 0), "");
    }

    #[test]
    fn toggle_keeps_missing_trailing_newline() {
        assert_eq!(toggle_task("- [ ] only", 0), "- [x] only");
    }

    #[test]
    fn add_task_to_empty_body() {
        assert_eq!(add_task("", "first", None), "- [ ] first\n");
    }

    #[test]
    fn add_task_adds_missing_newline_first() {
        assert_eq!(add_task("notes", "t", None), "notes\n- [ ] t\n");
    }

    #[test]
    fn add_task_does_not_double_newline() {
        assert_eq!(add_task("notes\n", "t", None), "notes\n- [ ] t\n");
        assert_eq!(add_task("notes\n\n", "t", None), "notes\n\n- [ ] t\n");
    }

    #[test]
    fn add_task_with_due_round_trips_through_parse() {
        let body = add_task("# Plan\n", "ship crate", Some((2026, 3, 4)));
        assert_eq!(body, "# Plan\n- [ ] ship crate 📅 2026-03-04\n");
        let tasks = parse_tasks(&body);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].line, 1);
        assert!(!tasks[0].done);
        assert_eq!(tasks[0].text, "ship crate");
        assert_eq!(tasks[0].due, Some((2026, 3, 4)));
    }

    #[test]
    fn add_task_pads_date_fields() {
        assert_eq!(
            add_task("", "x", Some((2026, 1, 9))),
            "- [ ] x 📅 2026-01-09\n"
        );
    }

    #[test]
    fn add_task_flattens_newlines_and_trims() {
        assert_eq!(add_task("", "  a\nb\r\n ", None), "- [ ] a b\n");
    }

    #[test]
    fn add_task_with_empty_text_has_no_stray_space() {
        assert_eq!(add_task("", "", None), "- [ ]\n");
        assert_eq!(
            add_task("", "", Some((2026, 1, 1))),
            "- [ ] 📅 2026-01-01\n"
        );
    }

    #[test]
    fn add_then_toggle_marks_new_line() {
        let body = add_task("a\n", "new", None);
        let out = toggle_task(&body, 1);
        assert_eq!(out, "a\n- [x] new\n");
    }
}
