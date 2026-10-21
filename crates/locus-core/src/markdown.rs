//! Obsidian-flavoured Markdown for the Notes apps: blocks and inline spans
//! for the rendered view, per-line styles for the source editor, and the
//! formatting commands (bold, headings, lists...) the toolbar applies.
//!
//! Notes stay plain Markdown on the ship, so Obsidian reads them unchanged.
//! Covered: ATX headings, paragraphs (a newline is a line break, as in
//! Obsidian), `-`/`*`/`+` and numbered lists with nesting, `- [ ]` tasks,
//! quotes, callouts (`> [!note] Title`), fenced code, rules, and inline
//! `**bold**`, `*italic*`, `~~strike~~`, `==highlight==`, `` `code` ``,
//! `[[wiki links|alias]]`, `![[embeds]]`, `[text](url)` and `#tags`.
//! Tables render as their source lines in monospace.

use std::ops::Range;

/// A run of text with one style.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub highlight: bool,
    pub code: bool,
    pub link: Option<Link>,
}

/// Where a span leads when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// `[[Title]]` or `[[Title|alias]]`: another note, by title.
    Note(String),
    /// `[text](https://...)`.
    Url(String),
    /// `#tag`.
    Tag(String),
}

/// How a list item starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Number(u32),
    Task(bool),
}

/// One rendered block. `line` is the first source line (0-based), which a
/// click uses to open the source there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Span>,
        line: usize,
    },
    /// One or more lines; a `\n` inside a span is a line break.
    Paragraph {
        spans: Vec<Span>,
        line: usize,
    },
    ListItem {
        marker: ListMarker,
        /// 0 at the left margin, one more per indent level.
        depth: usize,
        spans: Vec<Span>,
        line: usize,
    },
    Quote {
        spans: Vec<Span>,
        line: usize,
    },
    /// `> [!kind] Title` and the quote lines after it.
    Callout {
        kind: String,
        title: String,
        spans: Vec<Span>,
        line: usize,
    },
    Code {
        lang: String,
        text: String,
        line: usize,
    },
    /// Lines shown as written, in monospace (tables).
    Verbatim {
        text: String,
        line: usize,
    },
    Rule {
        line: usize,
    },
}

impl Block {
    pub fn line(&self) -> usize {
        match self {
            Block::Heading { line, .. }
            | Block::Paragraph { line, .. }
            | Block::ListItem { line, .. }
            | Block::Quote { line, .. }
            | Block::Callout { line, .. }
            | Block::Code { line, .. }
            | Block::Verbatim { line, .. }
            | Block::Rule { line } => *line,
        }
    }
}

// ----- blocks -----

fn heading(line: &str) -> Option<(u8, &str)> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    if rest.is_empty() {
        return Some((hashes as u8, ""));
    }
    rest.strip_prefix(' ')
        .map(|t| (hashes as u8, t.trim_end_matches(['#', ' '])))
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && (t.chars().all(|c| c == '-')
            || t.chars().all(|c| c == '*')
            || t.chars().all(|c| c == '_'))
}

/// Indent width in columns: a tab counts as four.
fn indent(line: &str) -> (usize, &str) {
    let mut cols = 0;
    for (i, c) in line.char_indices() {
        match c {
            ' ' => cols += 1,
            '\t' => cols += 4,
            _ => return (cols, &line[i..]),
        }
    }
    (cols, "")
}

/// `- item`, `1. item`, `- [x] item`: the marker and the item text.
fn list_item(rest: &str) -> Option<(ListMarker, &str)> {
    let after = if let Some(r) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        r
    } else if rest == "-" || rest == "*" || rest == "+" {
        ""
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 {
            return None;
        }
        let tail = &rest[digits..];
        let tail = tail
            .strip_prefix(". ")
            .or_else(|| tail.strip_prefix(") "))?;
        let n = rest[..digits].parse().ok()?;
        return Some((ListMarker::Number(n), tail));
    };
    for (mark, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
        if let Some(t) = after.strip_prefix(mark) {
            return Some((ListMarker::Task(done), t));
        }
    }
    for (mark, done) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if after == mark {
            return Some((ListMarker::Task(done), ""));
        }
    }
    Some((ListMarker::Bullet, after))
}

fn fence(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix("```").or_else(|| t.strip_prefix("~~~"))
}

fn quote_text(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix("> ").or_else(|| t.strip_prefix('>'))
}

fn callout_head(text: &str) -> Option<(String, String)> {
    let rest = text.strip_prefix("[!")?;
    let end = rest.find(']')?;
    let kind = rest[..end].to_ascii_lowercase();
    let title = rest[end + 1..]
        .trim_start_matches(['+', '-'])
        .trim()
        .to_string();
    Some((kind, title))
}

fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.len() > 1
}

/// The document as blocks. Blank lines separate paragraphs and are not
/// blocks themselves. YAML front matter (`---` ... `---` at the top) is
/// left out, as Obsidian shows it as properties, not text.
pub fn blocks(body: &str) -> Vec<Block> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    if lines.first().is_some_and(|l| l.trim_end() == "---") {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim_end() == "---") {
            i = end + 2;
        }
    }
    while i < lines.len() {
        let line = lines[i];
        let start = i;
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(info) = fence(line) {
            let marker = &line.trim_start()[..3];
            let mut text = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(marker) {
                text.push(lines[i]);
                i += 1;
            }
            i += 1; // the closing fence, or past the end
            out.push(Block::Code {
                lang: info.trim().to_string(),
                text: text.join("\n"),
                line: start,
            });
            continue;
        }
        if let Some((level, text)) = heading(line) {
            out.push(Block::Heading {
                level,
                spans: inline(text),
                line: start,
            });
            i += 1;
            continue;
        }
        if is_rule(line) {
            out.push(Block::Rule { line: start });
            i += 1;
            continue;
        }
        if let Some(text) = quote_text(line) {
            let mut body_lines = Vec::new();
            let head = callout_head(text);
            if head.is_none() {
                body_lines.push(text);
            }
            i += 1;
            while i < lines.len() {
                match quote_text(lines[i]) {
                    Some(t) => body_lines.push(t),
                    None => break,
                }
                i += 1;
            }
            let spans = inline(&body_lines.join("\n"));
            out.push(match head {
                Some((kind, title)) => Block::Callout {
                    title: if title.is_empty() {
                        capitalize(&kind)
                    } else {
                        title
                    },
                    kind,
                    spans,
                    line: start,
                },
                None => Block::Quote { spans, line: start },
            });
            continue;
        }
        let (cols, rest) = indent(line);
        if let Some((marker, text)) = list_item(rest) {
            out.push(Block::ListItem {
                marker,
                depth: cols / 2,
                spans: inline(text),
                line: start,
            });
            i += 1;
            continue;
        }
        if is_table_row(line) {
            let mut rows = Vec::new();
            while i < lines.len() && is_table_row(lines[i]) {
                rows.push(lines[i].trim());
                i += 1;
            }
            out.push(Block::Verbatim {
                text: rows.join("\n"),
                line: start,
            });
            continue;
        }
        // A paragraph runs until a blank line or another kind of block.
        let mut para = vec![line.trim_end()];
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            let (_, rest) = indent(l);
            if l.trim().is_empty()
                || fence(l).is_some()
                || heading(l).is_some()
                || quote_text(l).is_some()
                || list_item(rest).is_some()
                || is_table_row(l)
                || is_rule(l)
            {
                break;
            }
            para.push(l.trim_end());
            i += 1;
        }
        out.push(Block::Paragraph {
            spans: inline(&para.join("\n")),
            line: start,
        });
    }
    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ----- inline -----

/// Toggles in effect while scanning a line.
#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    highlight: bool,
}

/// Does `marker` occur again in `rest`, with text before it?
fn closes(rest: &str, marker: &str) -> bool {
    rest.find(marker).is_some_and(|at| at > 0)
}

fn is_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric())
}

/// The inline spans of `text`, markers removed. Adjacent spans of the same
/// style are merged.
pub fn inline(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Span>, style: Style| {
        if buf.is_empty() {
            return;
        }
        let text = std::mem::take(buf);
        if let Some(last) = out.last_mut()
            && last.link.is_none()
            && !last.code
            && last.bold == style.bold
            && last.italic == style.italic
            && last.strike == style.strike
            && last.highlight == style.highlight
        {
            last.text.push_str(&text);
            return;
        }
        out.push(Span {
            text,
            bold: style.bold,
            italic: style.italic,
            strike: style.strike,
            highlight: style.highlight,
            ..Span::default()
        });
    };
    let push_special =
        |out: &mut Vec<Span>, style: Style, text: String, code: bool, link: Option<Link>| {
            out.push(Span {
                text,
                bold: style.bold,
                italic: style.italic,
                strike: style.strike,
                highlight: style.highlight,
                code,
                link,
            });
        };
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let prev = text[..i].chars().next_back();
        let c = rest.chars().next().expect("in bounds");
        // Escapes.
        if c == '\\' {
            if let Some(n) = rest[1..]
                .chars()
                .next()
                .filter(|n| n.is_ascii_punctuation())
            {
                buf.push(n);
                i += 1 + n.len_utf8();
                continue;
            }
        }
        // `code`
        if c == '`' {
            let ticks = rest.bytes().take_while(|&b| b == b'`').count();
            let marker = &rest[..ticks];
            if let Some(end) = rest[ticks..].find(marker) {
                flush(&mut buf, &mut out, style);
                let code = rest[ticks..ticks + end].trim().to_string();
                push_special(&mut out, style, code, true, None);
                i += ticks + end + ticks;
                continue;
            }
        }
        // ![[embed]] and [[wiki|alias]]
        let wiki_at = if rest.starts_with("![[") {
            Some(3)
        } else if rest.starts_with("[[") {
            Some(2)
        } else {
            None
        };
        if let Some(open) = wiki_at {
            if let Some(end) = rest[open..].find("]]") {
                let inner = &rest[open..open + end];
                let (target, label) = match inner.split_once('|') {
                    Some((t, l)) => (t, l),
                    None => (inner, inner),
                };
                // `Note#Heading` links to the note.
                let note = target.split('#').next().unwrap_or(target).trim();
                flush(&mut buf, &mut out, style);
                push_special(
                    &mut out,
                    style,
                    label.trim().to_string(),
                    false,
                    Some(Link::Note(note.to_string())),
                );
                i += open + end + 2;
                continue;
            }
        }
        // [text](url)
        if c == '[' {
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close + 2..].find(')') {
                    let label = &rest[1..close];
                    let url = rest[close + 2..close + 2 + end].trim();
                    if !label.contains('\n') && !url.contains(char::is_whitespace) {
                        flush(&mut buf, &mut out, style);
                        let link = if url.contains("://") || url.starts_with("mailto:") {
                            Link::Url(url.to_string())
                        } else {
                            Link::Note(url.trim_end_matches(".md").replace("%20", " "))
                        };
                        push_special(&mut out, style, label.to_string(), false, Some(link));
                        i += close + 2 + end + 1;
                        continue;
                    }
                }
            }
        }
        // #tag
        if c == '#' && !is_word(prev) && prev != Some('#') {
            let tag: String = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .collect();
            if tag.chars().any(|c| !c.is_ascii_digit()) {
                flush(&mut buf, &mut out, style);
                push_special(
                    &mut out,
                    style,
                    format!("#{tag}"),
                    false,
                    Some(Link::Tag(tag.clone())),
                );
                i += 1 + tag.len();
                continue;
            }
        }
        // **bold** / __bold__, ~~strike~~, ==highlight==, *italic* / _italic_
        let pair = |m: &str| rest.starts_with(m);
        let mut toggled = false;
        for (marker, which) in [("**", 0), ("__", 0), ("~~", 1), ("==", 2)] {
            if !pair(marker) {
                continue;
            }
            // `__` inside words is not emphasis (snake__case).
            if marker == "__" && !style.bold && (is_word(prev) || is_word(rest[2..].chars().next()))
            {
                continue;
            }
            let on = match which {
                0 => style.bold,
                1 => style.strike,
                _ => style.highlight,
            };
            if on || closes(&rest[2..], marker) {
                flush(&mut buf, &mut out, style);
                match which {
                    0 => style.bold = !style.bold,
                    1 => style.strike = !style.strike,
                    _ => style.highlight = !style.highlight,
                }
                i += 2;
                toggled = true;
                break;
            }
        }
        if toggled {
            continue;
        }
        if (c == '*' || c == '_') && !rest.starts_with("**") && !rest.starts_with("__") {
            let next = rest[1..].chars().next();
            let marker = &rest[..1];
            let opening = !style.italic
                && next.is_some_and(|n| !n.is_whitespace())
                && closes(&rest[1..], marker)
                && !(c == '_' && is_word(prev));
            let closing = style.italic && prev.is_some_and(|p| !p.is_whitespace());
            if opening || closing {
                flush(&mut buf, &mut out, style);
                style.italic = !style.italic;
                i += 1;
                continue;
            }
        }
        buf.push(c);
        i += c.len_utf8();
    }
    flush(&mut buf, &mut out, style);
    out
}

/// The plain text of some spans.
pub fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

// ----- source styling -----

/// How the source editor colours a range of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStyle {
    /// Markdown syntax: `#`, `**`, `[[`, list bullets... drawn dimmed.
    Marker,
    Heading(u8),
    Bold,
    Italic,
    BoldItalic,
    Strike,
    Highlight,
    Code,
    Link,
    Tag,
    Quote,
    /// A done task's text.
    Done,
}

/// Styles for one source line. `in_code` says whether the line is inside a
/// fenced block; the second value is that state for the next line.
pub fn source_styles(line: &str, in_code: bool) -> (Vec<(Range<usize>, SourceStyle)>, bool) {
    let mut out = Vec::new();
    if let Some(_info) = fence(line) {
        out.push((0..line.len(), SourceStyle::Marker));
        return (out, !in_code);
    }
    if in_code {
        out.push((0..line.len(), SourceStyle::Code));
        return (out, true);
    }
    if let Some((level, _)) = heading(line) {
        let hashes = level as usize;
        out.push((0..hashes.min(line.len()), SourceStyle::Marker));
        if line.len() > hashes {
            out.push((hashes..line.len(), SourceStyle::Heading(level)));
        }
        return (out, false);
    }
    if is_rule(line) {
        out.push((0..line.len(), SourceStyle::Marker));
        return (out, false);
    }
    let mut offset = 0;
    let mut base = None;
    if let Some(text) = quote_text(line) {
        offset = line.len() - text.len();
        out.push((0..offset, SourceStyle::Marker));
        base = Some(SourceStyle::Quote);
    } else {
        let (_, rest) = indent(line);
        if let Some((marker, text)) = list_item(rest) {
            offset = line.len() - text.len();
            out.push((line.len() - rest.len()..offset, SourceStyle::Marker));
            if marker == ListMarker::Task(true) {
                base = Some(SourceStyle::Done);
            }
        }
    }
    inline_source(&line[offset..], offset, base, &mut out);
    (out, false)
}

/// Marker and text ranges for the inline syntax of `text`, which starts at
/// byte `at` of its line.
fn inline_source(
    text: &str,
    at: usize,
    base: Option<SourceStyle>,
    out: &mut Vec<(Range<usize>, SourceStyle)>,
) {
    let mut style = Style::default();
    let mut i = 0;
    let mut run_start = 0;
    let current = |s: Style| -> Option<SourceStyle> {
        if s.highlight {
            Some(SourceStyle::Highlight)
        } else if s.strike {
            Some(SourceStyle::Strike)
        } else if s.bold && s.italic {
            Some(SourceStyle::BoldItalic)
        } else if s.bold {
            Some(SourceStyle::Bold)
        } else if s.italic {
            Some(SourceStyle::Italic)
        } else {
            base
        }
    };
    let emit_run =
        |from: usize, to: usize, s: Style, out: &mut Vec<(Range<usize>, SourceStyle)>| {
            if to > from {
                if let Some(st) = current(s) {
                    out.push((at + from..at + to, st));
                }
            }
        };
    while i < text.len() {
        let rest = &text[i..];
        let prev = text[..i].chars().next_back();
        let c = rest.chars().next().expect("in bounds");
        if c == '`' {
            let ticks = rest.bytes().take_while(|&b| b == b'`').count();
            if let Some(end) = rest[ticks..].find(&rest[..ticks]) {
                emit_run(run_start, i, style, out);
                out.push((at + i..at + i + ticks, SourceStyle::Marker));
                out.push((at + i + ticks..at + i + ticks + end, SourceStyle::Code));
                out.push((
                    at + i + ticks + end..at + i + 2 * ticks + end,
                    SourceStyle::Marker,
                ));
                i += 2 * ticks + end;
                run_start = i;
                continue;
            }
        }
        let open = if rest.starts_with("![[") {
            3
        } else if rest.starts_with("[[") {
            2
        } else {
            0
        };
        if open > 0 {
            if let Some(end) = rest[open..].find("]]") {
                emit_run(run_start, i, style, out);
                out.push((at + i..at + i + open, SourceStyle::Marker));
                out.push((at + i + open..at + i + open + end, SourceStyle::Link));
                out.push((
                    at + i + open + end..at + i + open + end + 2,
                    SourceStyle::Marker,
                ));
                i += open + end + 2;
                run_start = i;
                continue;
            }
        }
        if c == '[' {
            if let Some(close) = rest.find("](") {
                if let Some(end) = rest[close + 2..].find(')') {
                    emit_run(run_start, i, style, out);
                    out.push((at + i..at + i + 1, SourceStyle::Marker));
                    out.push((at + i + 1..at + i + close, SourceStyle::Link));
                    out.push((
                        at + i + close..at + i + close + 3 + end,
                        SourceStyle::Marker,
                    ));
                    i += close + 3 + end;
                    run_start = i;
                    continue;
                }
            }
        }
        if c == '#' && !is_word(prev) && prev != Some('#') {
            let len: usize = rest[1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
                .map(char::len_utf8)
                .sum();
            if len > 0 && rest[1..1 + len].chars().any(|c| !c.is_ascii_digit()) {
                emit_run(run_start, i, style, out);
                out.push((at + i..at + i + 1 + len, SourceStyle::Tag));
                i += 1 + len;
                run_start = i;
                continue;
            }
        }
        let mut toggled = false;
        for (marker, which) in [("**", 0), ("__", 0), ("~~", 1), ("==", 2)] {
            if !rest.starts_with(marker) {
                continue;
            }
            if marker == "__" && !style.bold && (is_word(prev) || is_word(rest[2..].chars().next()))
            {
                continue;
            }
            let on = match which {
                0 => style.bold,
                1 => style.strike,
                _ => style.highlight,
            };
            if on || closes(&rest[2..], marker) {
                emit_run(run_start, i, style, out);
                out.push((at + i..at + i + 2, SourceStyle::Marker));
                match which {
                    0 => style.bold = !style.bold,
                    1 => style.strike = !style.strike,
                    _ => style.highlight = !style.highlight,
                }
                i += 2;
                run_start = i;
                toggled = true;
                break;
            }
        }
        if toggled {
            continue;
        }
        if (c == '*' || c == '_') && !rest.starts_with("**") && !rest.starts_with("__") {
            let next = rest[1..].chars().next();
            let opening = !style.italic
                && next.is_some_and(|n| !n.is_whitespace())
                && closes(&rest[1..], &rest[..1])
                && !(c == '_' && is_word(prev));
            let closing = style.italic && prev.is_some_and(|p| !p.is_whitespace());
            if opening || closing {
                emit_run(run_start, i, style, out);
                out.push((at + i..at + i + 1, SourceStyle::Marker));
                style.italic = !style.italic;
                i += 1;
                run_start = i;
                continue;
            }
        }
        i += c.len_utf8();
    }
    emit_run(run_start, text.len(), style, out);
}

// ----- tasks -----

/// Tick or untick the task on source line `line`. `None` if that line is not
/// a task.
pub fn toggle_task_line(body: &str, line: usize) -> Option<String> {
    let mut lines: Vec<String> = body.split('\n').map(str::to_string).collect();
    let l = lines.get_mut(line)?;
    let (_, rest) = indent(l);
    let lead = l.len() - rest.len();
    let after_bullet = lead + 2;
    match list_item(rest)? {
        (ListMarker::Task(done), _) => {
            let mark = if done { " " } else { "x" };
            l.replace_range(after_bullet + 1..after_bullet + 2, mark);
            Some(lines.join("\n"))
        }
        _ => None,
    }
}

// ----- formatting commands -----

/// What the toolbar applies to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Highlight,
    Code,
    Link,
    WikiLink,
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Quote,
    CodeBlock,
}

/// A position in the text: line and byte column.
pub type Pos = (usize, usize);

/// The text after a format command, and the new selection (`start`, `end`;
/// equal for a plain cursor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub start: Pos,
    pub end: Pos,
}

fn to_offset(lines: &[&str], (line, col): Pos) -> usize {
    let mut off = 0;
    for l in lines.iter().take(line) {
        off += l.len() + 1;
    }
    let len = lines.get(line).map_or(0, |l| l.len());
    let mut col = col.min(len);
    if let Some(l) = lines.get(line) {
        while !l.is_char_boundary(col) {
            col -= 1;
        }
    }
    off + col
}

fn to_pos(text: &str, offset: usize) -> Pos {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count();
    let col = before.len() - before.rfind('\n').map_or(0, |i| i + 1);
    (line, col)
}

/// Apply `format` to the selection from `a` to `b` (either order) of `text`.
/// Inline formats wrap the selection, or unwrap it when it is already
/// wrapped; with nothing selected they insert the markers with the cursor
/// between. Line formats apply to every line the selection touches, and
/// remove themselves when every one of those lines already has them.
pub fn apply_format(text: &str, a: Pos, b: Pos, format: Format) -> Formatted {
    let lines: Vec<&str> = text.split('\n').collect();
    let (oa, ob) = (to_offset(&lines, a), to_offset(&lines, b));
    let (s, e) = (oa.min(ob), oa.max(ob));
    match format {
        Format::Bold => wrap(text, s, e, "**", "**"),
        Format::Italic => wrap(text, s, e, "*", "*"),
        Format::Strike => wrap(text, s, e, "~~", "~~"),
        Format::Highlight => wrap(text, s, e, "==", "=="),
        Format::Code => wrap(text, s, e, "`", "`"),
        Format::WikiLink => wrap(text, s, e, "[[", "]]"),
        Format::Link => {
            let label = &text[s..e];
            let url = "https://";
            let new = format!("{}[{label}]({url}){}", &text[..s], &text[e..]);
            // Select the URL so typing replaces it.
            let us = s + label.len() + 3;
            Formatted {
                start: to_pos(&new, us),
                end: to_pos(&new, us + url.len()),
                text: new,
            }
        }
        Format::CodeBlock => {
            let (first, last) = (to_pos(text, s).0, to_pos(text, e).0);
            let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            out.insert(last + 1, "```".into());
            out.insert(first, "```".into());
            let new = out.join("\n");
            let cursor = (first + 1, 0);
            Formatted {
                text: new,
                start: cursor,
                end: cursor,
            }
        }
        _ => line_format(&lines, to_pos(text, s).0, to_pos(text, e).0, format, a, b),
    }
}

fn wrap(text: &str, s: usize, e: usize, open: &str, close: &str) -> Formatted {
    let inner = &text[s..e];
    // Already wrapped, markers inside the selection: unwrap.
    if inner.len() >= open.len() + close.len() && inner.starts_with(open) && inner.ends_with(close)
    {
        let un = &inner[open.len()..inner.len() - close.len()];
        let new = format!("{}{un}{}", &text[..s], &text[e..]);
        return Formatted {
            start: to_pos(&new, s),
            end: to_pos(&new, s + un.len()),
            text: new,
        };
    }
    // Markers just outside the selection: unwrap.
    if s >= open.len() && text[..s].ends_with(open) && text[e..].starts_with(close) {
        let new = format!(
            "{}{inner}{}",
            &text[..s - open.len()],
            &text[e + close.len()..]
        );
        let ns = s - open.len();
        return Formatted {
            start: to_pos(&new, ns),
            end: to_pos(&new, ns + inner.len()),
            text: new,
        };
    }
    let new = format!("{}{open}{inner}{close}{}", &text[..s], &text[e..]);
    let ns = s + open.len();
    Formatted {
        start: to_pos(&new, ns),
        end: to_pos(&new, ns + inner.len()),
        text: new,
    }
}

/// The prefix a line format puts at the start of a line (after its indent).
fn line_prefix(format: Format, n: usize) -> String {
    match format {
        Format::Heading(level) => format!("{} ", "#".repeat(level.clamp(1, 6) as usize)),
        Format::Bullet => "- ".into(),
        Format::Numbered => format!("{n}. "),
        Format::Task => "- [ ] ".into(),
        Format::Quote => "> ".into(),
        _ => String::new(),
    }
}

/// The length of a line's existing block prefix (heading, list, task or
/// quote marker), after the indent, and the format it is.
fn existing_prefix(rest: &str) -> (usize, Option<Format>) {
    if let Some((level, _)) = heading(rest) {
        let hashes = level as usize;
        let len = if rest.len() > hashes {
            hashes + 1
        } else {
            hashes
        };
        return (len, Some(Format::Heading(level)));
    }
    if let Some(t) = quote_text(rest) {
        return (rest.len() - t.len(), Some(Format::Quote));
    }
    if let Some((marker, t)) = list_item(rest) {
        let f = match marker {
            ListMarker::Bullet => Format::Bullet,
            ListMarker::Number(_) => Format::Numbered,
            ListMarker::Task(_) => Format::Task,
        };
        return (rest.len() - t.len(), Some(f));
    }
    (0, None)
}

fn line_format(
    lines: &[&str],
    first: usize,
    last: usize,
    format: Format,
    a: Pos,
    b: Pos,
) -> Formatted {
    let same = |f: Option<Format>| match (f, format) {
        (Some(Format::Heading(x)), Format::Heading(y)) => x == y,
        (Some(x), y) => x == y,
        (None, _) => false,
    };
    let targets: Vec<usize> = (first..=last.min(lines.len().saturating_sub(1)))
        .filter(|&i| !lines[i].trim().is_empty() || first == last)
        .collect();
    let remove = !targets.is_empty()
        && targets
            .iter()
            .all(|&i| same(existing_prefix(indent(lines[i]).1).1));
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    // Column shift per line, to keep the selection on the same text.
    let mut shift: Vec<isize> = vec![0; lines.len()];
    for (n, &i) in targets.iter().enumerate() {
        let l = lines[i];
        let (_, rest) = indent(l);
        let lead = l.len() - rest.len();
        let (old_len, _) = existing_prefix(rest);
        let new_prefix = if remove {
            String::new()
        } else {
            line_prefix(format, n + 1)
        };
        out[i] = format!("{}{}{}", &l[..lead], new_prefix, &rest[old_len..]);
        shift[i] = new_prefix.len() as isize - old_len as isize;
    }
    let text = out.join("\n");
    let adjust = |(line, col): Pos| -> Pos {
        let s = shift.get(line).copied().unwrap_or(0);
        let col = (col as isize + s).max(0) as usize;
        (line, col.min(out.get(line).map_or(0, |l| l.len())))
    };
    Formatted {
        start: adjust(a),
        end: adjust(b),
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Span {
        Span {
            text: text.into(),
            ..Span::default()
        }
    }

    #[test]
    fn inline_styles_and_links() {
        let spans = inline("a **b** *c* ~~d~~ ==e== `f` [[Note|g]] [h](https://x.y) #tag");
        let got: Vec<(String, bool, bool, bool, bool, bool, Option<Link>)> = spans
            .iter()
            .map(|s| {
                (
                    s.text.clone(),
                    s.bold,
                    s.italic,
                    s.strike,
                    s.highlight,
                    s.code,
                    s.link.clone(),
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("a ".into(), false, false, false, false, false, None),
                ("b".into(), true, false, false, false, false, None),
                (" ".into(), false, false, false, false, false, None),
                ("c".into(), false, true, false, false, false, None),
                (" ".into(), false, false, false, false, false, None),
                ("d".into(), false, false, true, false, false, None),
                (" ".into(), false, false, false, false, false, None),
                ("e".into(), false, false, false, true, false, None),
                (" ".into(), false, false, false, false, false, None),
                ("f".into(), false, false, false, false, true, None),
                (" ".into(), false, false, false, false, false, None),
                (
                    "g".into(),
                    false,
                    false,
                    false,
                    false,
                    false,
                    Some(Link::Note("Note".into()))
                ),
                (" ".into(), false, false, false, false, false, None),
                (
                    "h".into(),
                    false,
                    false,
                    false,
                    false,
                    false,
                    Some(Link::Url("https://x.y".into()))
                ),
                (" ".into(), false, false, false, false, false, None),
                (
                    "#tag".into(),
                    false,
                    false,
                    false,
                    false,
                    false,
                    Some(Link::Tag("tag".into()))
                ),
            ]
        );
    }

    #[test]
    fn unclosed_markers_and_snake_case_stay_literal() {
        assert_eq!(inline("2 * 3 = 6"), vec![s("2 * 3 = 6")]);
        assert_eq!(
            inline("snake_case_name and **open"),
            vec![s("snake_case_name and **open")]
        );
        assert_eq!(inline("issue #12"), vec![s("issue #12")]);
        assert_eq!(inline(r"\*not italic\*"), vec![s("*not italic*")]);
    }

    #[test]
    fn blocks_cover_obsidian_syntax() {
        let body = "---\ntags: x\n---\n# Title\n\nPara line one\nline two\n\n- a\n  - [x] b\n1. c\n> [!tip] Hint\n> body\n> plain quote? no\n\n> quote\n```rust\nlet x = 1;\n```\n---\n| a | b |\n|---|---|";
        let b = blocks(body);
        assert!(matches!(
            &b[0],
            Block::Heading {
                level: 1,
                line: 3,
                ..
            }
        ));
        assert!(
            matches!(&b[1], Block::Paragraph { line: 5, spans } if plain(spans) == "Para line one\nline two")
        );
        assert!(matches!(
            &b[2],
            Block::ListItem {
                marker: ListMarker::Bullet,
                depth: 0,
                ..
            }
        ));
        assert!(matches!(
            &b[3],
            Block::ListItem {
                marker: ListMarker::Task(true),
                depth: 1,
                line: 9,
                ..
            }
        ));
        assert!(matches!(
            &b[4],
            Block::ListItem {
                marker: ListMarker::Number(1),
                ..
            }
        ));
        assert!(
            matches!(&b[5], Block::Callout { kind, title, .. } if kind == "tip" && title == "Hint")
        );
        assert!(matches!(&b[6], Block::Quote { .. }));
        assert!(
            matches!(&b[7], Block::Code { lang, text, .. } if lang == "rust" && text == "let x = 1;")
        );
        assert!(matches!(&b[8], Block::Rule { .. }));
        assert!(matches!(&b[9], Block::Verbatim { .. }));
        assert_eq!(b.len(), 10);
    }

    #[test]
    fn source_styles_mark_syntax() {
        let (st, code) = source_styles("## Head", false);
        assert_eq!(
            st,
            vec![(0..2, SourceStyle::Marker), (2..7, SourceStyle::Heading(2))]
        );
        assert!(!code);
        let (st, _) = source_styles("- **b** x", false);
        assert_eq!(
            st,
            vec![
                (0..2, SourceStyle::Marker),
                (2..4, SourceStyle::Marker),
                (4..5, SourceStyle::Bold),
                (5..7, SourceStyle::Marker)
            ]
        );
        let (_, code) = source_styles("```", false);
        assert!(code);
        let (st, code) = source_styles("let x", true);
        assert_eq!(st, vec![(0..5, SourceStyle::Code)]);
        assert!(code);
    }

    #[test]
    fn tasks_toggle_by_line() {
        let body = "x\n- [ ] one\n  - [x] two\n- three";
        assert_eq!(
            toggle_task_line(body, 1).unwrap(),
            "x\n- [x] one\n  - [x] two\n- three"
        );
        assert_eq!(
            toggle_task_line(body, 2).unwrap(),
            "x\n- [ ] one\n  - [ ] two\n- three"
        );
        assert_eq!(toggle_task_line(body, 3), None);
        assert_eq!(toggle_task_line(body, 0), None);
    }

    #[test]
    fn inline_formats_wrap_and_unwrap() {
        let f = apply_format("say hello now", (0, 4), (0, 9), Format::Bold);
        assert_eq!(f.text, "say **hello** now");
        assert_eq!((f.start, f.end), ((0, 6), (0, 11)));
        let g = apply_format(&f.text, f.start, f.end, Format::Bold);
        assert_eq!(g.text, "say hello now");
        assert_eq!((g.start, g.end), ((0, 4), (0, 9)));
        // Nothing selected: markers with the cursor between.
        let h = apply_format("ab", (0, 1), (0, 1), Format::Italic);
        assert_eq!(h.text, "a**b");
        assert_eq!(h.start, (0, 2));
        let l = apply_format("site", (0, 0), (0, 4), Format::Link);
        assert_eq!(l.text, "[site](https://)");
        assert_eq!((l.start, l.end), ((0, 7), (0, 15)));
    }

    #[test]
    fn line_formats_toggle_and_replace() {
        let f = apply_format("a\nb\n\nc", (0, 0), (3, 1), Format::Bullet);
        assert_eq!(f.text, "- a\n- b\n\n- c");
        let g = apply_format(&f.text, (0, 2), (3, 3), Format::Bullet);
        assert_eq!(g.text, "a\nb\n\nc");
        let n = apply_format("- a\n- b", (0, 0), (1, 0), Format::Numbered);
        assert_eq!(n.text, "1. a\n2. b");
        let h = apply_format("## Title", (0, 5), (0, 5), Format::Heading(1));
        assert_eq!(h.text, "# Title");
        assert_eq!(h.start, (0, 4));
        let t = apply_format("  todo", (0, 2), (0, 2), Format::Task);
        assert_eq!(t.text, "  - [ ] todo");
    }
}
