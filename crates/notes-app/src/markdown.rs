//! GFM preview, wikilink extraction, counts.
//!
//! Preview renders GitHub Flavored Markdown (same parser as the source
//! highlighter). Source characters stay in the editor; this pane is the
//! formatted document, including images.

use cosmic::font;
use cosmic::iced::font::Style;
use cosmic::iced::{Font, Length};
use cosmic::widget;
use cosmic::Element;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, LinkType, Parser, Tag, TagEnd};
use std::path::{Path, PathBuf};

use crate::highlight;
use crate::Message;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: String,
}

pub fn wikilinks(body: &str) -> Vec<WikiLink> {
    let mut out = Vec::new();
    for (event, _) in Parser::new_ext(body, highlight::gfm_options()).into_offset_iter() {
        if let Event::Start(Tag::Link {
            link_type: LinkType::WikiLink { .. },
            dest_url,
            ..
        }) = event
        {
            let target = dest_url.split('|').next().unwrap_or(&dest_url).trim();
            if !target.is_empty() && !looks_like_image(target) {
                out.push(WikiLink {
                    target: target.to_string(),
                });
            }
        }
    }
    out
}

/// Canonical key for a wiki target: lowercase, no anchor/alias, `.md` and
/// path separators normalised. Shared by the graph, backlinks, and open.
pub fn normalize_target(s: &str) -> String {
    let s = s.trim();
    let s = s.split('#').next().unwrap_or(s);
    let s = s.split('|').next().unwrap_or(s).trim();
    let s = s
        .trim_end_matches(".md")
        .trim_end_matches(".MD")
        .trim_end_matches(".Md");
    s.replace('\\', "/")
        .trim_matches('/')
        .to_lowercase()
}

pub fn basename_of(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// One `#` heading in the body: its line and trimmed text, in document order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub line: usize,
    pub level: u8,
    pub text: String,
}

/// Headings of the body, in document order.
pub fn headings(body: &str) -> Vec<Heading> {
    let mut out: Vec<Heading> = Vec::new();
    let mut current: Option<Heading> = None;
    let mut text = String::new();
    for (event, range) in Parser::new_ext(body, highlight::gfm_options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                text.clear();
                current = Some(Heading {
                    line: line_at(body, range.start),
                    level: heading_u8(level),
                    text: String::new(),
                });
            }
            Event::Text(t) => text.push_str(&t),
            Event::Code(t) => text.push_str(&t),
            Event::End(TagEnd::Heading(_)) => {
                if let Some(mut h) = current.take() {
                    h.text = text.trim().to_string();
                    out.push(h);
                }
            }
            _ => {}
        }
    }
    out
}

pub fn word_count(body: &str) -> usize {
    body.split_whitespace().filter(|w| !w.is_empty()).count()
}

/// Rough reading time in minutes (200 wpm), rounded up, minimum 1.
pub fn reading_minutes(words: usize) -> usize {
    if words == 0 {
        return 0;
    }
    (words / 200).max(1)
}

pub fn char_count(body: &str) -> usize {
    body.chars().count()
}

/// Result of Enter on a GFM list item: new buffer text and caret (byte column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEnter {
    pub text: String,
    pub line: usize,
    pub column: usize,
}

/// Continue (or exit) a list when Enter is pressed on `line` at byte `column`.
///
/// A non-empty item inserts a newline and the next prefix (`- `, `2. `, `- [ ] `).
/// An empty item removes the marker (leave the list). Inside a fenced code
/// block this returns `None` so Enter stays a plain newline.
pub fn list_enter(text: &str, line: usize, column: usize) -> Option<ListEnter> {
    if in_fenced_code(text, line) {
        return None;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let raw = *lines.get(line)?;
    let item = parse_list_item(raw)?;
    let col = floor_col(raw, column);
    let split = col.max(item.prefix_len);
    let content = &raw[item.prefix_len..];

    if content.trim().is_empty() && col >= item.prefix_len {
        let mut next = lines;
        next[line] = "";
        return Some(ListEnter {
            text: next.join("\n"),
            line,
            column: 0,
        });
    }

    let next_prefix = item.next_prefix();
    let before = &raw[..split];
    let after = &raw[split..];
    let mut out = String::with_capacity(text.len() + next_prefix.len() + 1);
    for (i, l) in lines.iter().enumerate() {
        if i == line {
            out.push_str(before);
            out.push('\n');
            out.push_str(&next_prefix);
            out.push_str(after);
        } else {
            out.push_str(l);
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    Some(ListEnter {
        text: out,
        line: line + 1,
        column: next_prefix.len(),
    })
}

struct ListItem {
    prefix_len: usize,
    lead: String,
    ordered: Option<(u64, char)>,
    bullet: Option<char>,
    task: bool,
}

impl ListItem {
    fn next_prefix(&self) -> String {
        let mut s = self.lead.clone();
        if let Some((n, delim)) = self.ordered {
            s.push_str(&format!("{}{} ", n.saturating_add(1).min(999_999_999), delim));
        } else if let Some(b) = self.bullet {
            s.push(b);
            s.push(' ');
        }
        if self.task {
            s.push_str("[ ] ");
        }
        s
    }
}

fn parse_list_item(line: &str) -> Option<ListItem> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    while i < b.len() && b[i] == b'>' {
        i += 1;
        if i < b.len() && b[i] == b' ' {
            i += 1;
        }
    }
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let lead = line[..i].to_string();
    if i >= b.len() {
        return None;
    }
    let marker = i;
    let (ordered, bullet) = if b[i].is_ascii_digit() {
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let digits = i - marker;
        if !(1..=9).contains(&digits) {
            return None;
        }
        if i >= b.len() || (b[i] != b'.' && b[i] != b')') {
            return None;
        }
        let delim = b[i] as char;
        let n: u64 = line[marker..i].parse().ok()?;
        i += 1;
        (Some((n, delim)), None)
    } else if matches!(b[i], b'-' | b'*' | b'+') {
        let ch = b[i] as char;
        i += 1;
        (None, Some(ch))
    } else {
        return None;
    };
    if i < b.len() && b[i] != b' ' && b[i] != b'\t' {
        return None;
    }
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let mut task = false;
    if let Some(rest) = line.get(i..) {
        let checked = rest.starts_with("[x]") || rest.starts_with("[X]");
        let empty = rest.starts_with("[ ]");
        if checked || empty {
            let after = i + 3;
            if after == line.len() || line.as_bytes().get(after) == Some(&b' ') {
                task = true;
                i = after;
                if i < b.len() && b[i] == b' ' {
                    i += 1;
                }
            }
        }
    }
    Some(ListItem {
        prefix_len: i,
        lead,
        ordered,
        bullet,
        task,
    })
}

fn in_fenced_code(text: &str, line: usize) -> bool {
    let mut open: Option<(char, usize)> = None;
    for (i, raw) in text.split('\n').enumerate() {
        if i == line {
            return open.is_some();
        }
        if let Some((ch, n)) = fence_open(raw) {
            match open {
                None => open = Some((ch, n)),
                Some((och, on)) if och == ch && n >= on => open = None,
                Some(_) => {}
            }
        }
    }
    false
}

fn fence_open(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let indent = line.len() - trimmed.len();
    if indent > 3 {
        return None;
    }
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let n = trimmed.chars().take_while(|c| *c == ch).count();
    if n < 3 {
        return None;
    }
    Some((ch, n))
}

fn floor_col(line: &str, col: usize) -> usize {
    let mut col = col.min(line.len());
    while col > 0 && !line.is_char_boundary(col) {
        col -= 1;
    }
    col
}

pub fn toggle_task_line(body: &str, line: usize) -> String {
    body.lines()
        .enumerate()
        .map(|(i, raw)| {
            if i != line {
                return raw.to_string();
            }
            if let Some(rest) = raw.find("- [ ] ") {
                let mut s = raw.to_string();
                s.replace_range(rest..rest + 6, "- [x] ");
                s
            } else if let Some(rest) = raw.find("- [x] ").or_else(|| raw.find("- [X] ")) {
                let mut s = raw.to_string();
                s.replace_range(rest..rest + 6, "- [ ] ");
                s
            } else {
                raw.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Toggle `marker` at the start of `line` (after indent). Returns new text.
pub fn toggle_line_prefix(text: &str, line: usize, marker: &str) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let Some(raw) = lines.get_mut(line) else {
        return text.to_string();
    };
    let indent = raw
        .as_bytes()
        .iter()
        .take_while(|b| **b == b' ' || **b == b'\t')
        .count();
    let (lead, rest) = raw.split_at(indent);
    *raw = if rest.starts_with(marker) {
        format!("{lead}{}", &rest[marker.len()..])
    } else {
        format!("{lead}{marker}{rest}")
    };
    lines.join("\n")
}

/// Cycle ATX heading on `line`: none → `#` → `##` → `###` → none.
pub fn cycle_heading(text: &str, line: usize) -> String {
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let Some(raw) = lines.get_mut(line) else {
        return text.to_string();
    };
    let indent = raw
        .as_bytes()
        .iter()
        .take_while(|b| **b == b' ' || **b == b'\t')
        .count();
    let (lead, rest) = raw.split_at(indent);
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    let after = if hashes > 0 && rest.as_bytes().get(hashes) == Some(&b' ') {
        &rest[hashes + 1..]
    } else if hashes > 0 {
        &rest[hashes..]
    } else {
        rest
    };
    let next = match hashes {
        1 => 2,
        2 => 3,
        3 => 0,
        _ => 1,
    };
    *raw = if next == 0 {
        format!("{lead}{after}")
    } else {
        format!("{lead}{} {after}", "#".repeat(next))
    };
    lines.join("\n")
}

pub fn resolve_image(src: &str, files_dir: &Path) -> Option<PathBuf> {
    let p = Path::new(src);
    if p.is_absolute() && p.exists() {
        return Some(p.to_path_buf());
    }
    let local = files_dir.join(src);
    if local.exists() {
        return Some(local);
    }
    let nested = files_dir.join(p.file_name()?);
    if nested.exists() {
        return Some(nested);
    }
    None
}

pub fn view_visible(body: &str, files_dir: &Path) -> Element<'static, Message> {
    if body.trim().is_empty() {
        return widget::column::with_capacity(0).into();
    }
    let blocks = render_blocks(body, files_dir);
    widget::scrollable(
        widget::column::with_children(blocks)
            .spacing(cosmic::theme::spacing().space_s)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

#[derive(Clone, Copy)]
struct Inline {
    heading: u8,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    quote: bool,
}

impl Default for Inline {
    fn default() -> Self {
        Self {
            heading: 0,
            bold: false,
            italic: false,
            strike: false,
            code: false,
            quote: false,
        }
    }
}

enum OpenLink {
    Url(String),
    Wiki(String),
}

struct Builder<'a> {
    files: &'a Path,
    body: &'a str,
    blocks: Vec<Element<'static, Message>>,
    inlines: Vec<Element<'static, Message>>,
    style: Inline,
    link: Option<OpenLink>,
    image: Option<String>,
    code: Option<String>,
    list_depth: usize,
    table: Option<Vec<Vec<String>>>,
    row: Vec<String>,
    cell: String,
    in_cell: bool,
}

fn render_blocks(body: &str, files_dir: &Path) -> Vec<Element<'static, Message>> {
    let (front, body) = match highlight::yaml_frontmatter(body) {
        Some(range) => {
            let raw = body.get(..range.end).unwrap_or("");
            let rest = body.get(range.end..).unwrap_or("");
            (Some(raw.to_string()), rest.trim_start_matches(['\r', '\n']))
        }
        None => (None, body),
    };
    let mut b = Builder {
        files: files_dir,
        body,
        blocks: Vec::new(),
        inlines: Vec::new(),
        style: Inline::default(),
        link: None,
        image: None,
        code: None,
        list_depth: 0,
        table: None,
        row: Vec::new(),
        cell: String::new(),
        in_cell: false,
    };
    for (event, range) in Parser::new_ext(body, highlight::gfm_options()).into_offset_iter() {
        b.event(event, range);
    }
    b.flush_inlines();
    if let Some(table) = b.table.take() {
        b.blocks.push(view_table(table));
    }
    if let Some(raw) = front {
        let shown: String = raw
            .lines()
            .filter(|line| line.trim() != "---")
            .collect::<Vec<_>>()
            .join("  ·  ");
        if !shown.trim().is_empty() {
            b.blocks.insert(0, widget::text::caption(shown).into());
        }
    }
    b.blocks
}

impl Builder<'_> {
    fn event(&mut self, event: Event<'_>, range: std::ops::Range<usize>) {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(end) => self.end(end),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.push_styled(code.as_ref(), true),
            Event::Html(html) | Event::InlineHtml(html) => self.push_styled(html.as_ref(), true),
            Event::SoftBreak => self.text(" "),
            Event::HardBreak => self.text("\n"),
            Event::Rule => {
                self.flush_inlines();
                self.blocks.push(widget::divider::horizontal::default().into());
            }
            Event::TaskListMarker(done) => {
                let line = line_at(self.body, range.start);
                self.inlines.push(
                    widget::button::text(if done { "[x]" } else { "[ ]" })
                        .on_press(Message::ToggleTask(line))
                        .into(),
                );
            }
            Event::FootnoteReference(id) => self.push_styled(&format!("[{id}]"), false),
            Event::InlineMath(m) | Event::DisplayMath(m) => self.push_styled(m.as_ref(), true),
        }
    }

    fn start(&mut self, tag: Tag<'_>, _range: std::ops::Range<usize>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                self.flush_inlines();
                self.style.heading = heading_u8(level);
            }
            Tag::BlockQuote(_) => {
                self.flush_inlines();
                self.style.quote = true;
            }
            Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented) => {
                self.flush_inlines();
                self.code = Some(String::new());
            }
            Tag::List(_) => {
                self.flush_inlines();
                self.list_depth += 1;
            }
            Tag::Item => self.flush_inlines(),
            Tag::Emphasis | Tag::Superscript | Tag::Subscript => self.style.italic = true,
            Tag::Strong => self.style.bold = true,
            Tag::Strikethrough => self.style.strike = true,
            Tag::Link {
                link_type: LinkType::WikiLink { .. },
                dest_url,
                ..
            } => {
                let target = dest_url.split('|').next().unwrap_or(&dest_url).trim().to_string();
                self.link = Some(OpenLink::Wiki(target));
            }
            Tag::Link { dest_url, .. } => {
                self.link = Some(OpenLink::Url(dest_url.into_string()));
            }
            Tag::Image { dest_url, .. } => {
                self.image = Some(dest_url.into_string());
            }
            Tag::Table(_) => {
                self.flush_inlines();
                self.table = Some(Vec::new());
            }
            Tag::TableHead | Tag::TableRow => self.row.clear(),
            Tag::TableCell => {
                self.in_cell = true;
                self.cell.clear();
            }
            Tag::HtmlBlock => {
                self.flush_inlines();
                self.code = Some(String::new());
            }
            Tag::FootnoteDefinition(id) => {
                self.flush_inlines();
                self.push_styled(&format!("[{id}]: "), false);
            }
            _ => {}
        }
    }

    fn end(&mut self, end: TagEnd) {
        match end {
            TagEnd::Paragraph | TagEnd::Item | TagEnd::FootnoteDefinition => {
                self.flush_inlines();
            }
            TagEnd::Heading(_) => {
                self.flush_inlines();
                self.style.heading = 0;
            }
            TagEnd::BlockQuote(_) => {
                self.flush_inlines();
                self.style.quote = false;
            }
            TagEnd::CodeBlock | TagEnd::HtmlBlock => {
                if let Some(code) = self.code.take() {
                    self.blocks.push(
                        widget::text(code)
                            .size(14.0)
                            .font(font::mono())
                            .into(),
                    );
                }
            }
            TagEnd::List(_) => {
                self.flush_inlines();
                self.list_depth = self.list_depth.saturating_sub(1);
            }
            TagEnd::Emphasis | TagEnd::Superscript | TagEnd::Subscript => self.style.italic = false,
            TagEnd::Strong => self.style.bold = false,
            TagEnd::Strikethrough => self.style.strike = false,
            TagEnd::Link => self.link = None,
            TagEnd::Image => {
                if let Some(src) = self.image.take() {
                    self.flush_inlines();
                    self.blocks.push(view_image(&src, self.files));
                }
            }
            TagEnd::Table => {
                if let Some(table) = self.table.take() {
                    self.blocks.push(view_table(table));
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.push(std::mem::take(&mut self.row));
                }
            }
            TagEnd::TableCell => {
                self.in_cell = false;
                self.row.push(std::mem::take(&mut self.cell));
            }
            _ => {}
        }
    }

    fn text(&mut self, s: &str) {
        if let Some(code) = self.code.as_mut() {
            code.push_str(s);
            return;
        }
        if self.image.is_some() {
            return;
        }
        if self.in_cell {
            self.cell.push_str(s);
            return;
        }
        self.push_styled(s, false);
    }

    fn push_styled(&mut self, s: &str, as_code: bool) {
        if s.is_empty() {
            return;
        }
        let el = styled_text(s, self.style, as_code);
        let el = match &self.link {
            Some(OpenLink::Wiki(target)) => widget::button::link(s.to_string())
                .on_press(Message::OpenWiki(target.clone()))
                .into(),
            Some(OpenLink::Url(url)) => widget::button::link(s.to_string())
                .on_press(Message::OpenUrl(url.clone()))
                .into(),
            None => el,
        };
        self.inlines.push(el);
    }

    fn flush_inlines(&mut self) {
        if self.inlines.is_empty() {
            return;
        }
        let children = std::mem::take(&mut self.inlines);
        let row: Element<'static, Message> = widget::flex_row(children).width(Length::Fill).into();
        let pad = (self.list_depth.saturating_sub(1) * 16) as u16;
        let quote = self.style.quote;
        let wrapped = if pad > 0 || quote {
            widget::container(row)
                .padding([0, 0, 0, pad.max(if quote { 12 } else { 0 })])
                .width(Length::Fill)
                .into()
        } else {
            row
        };
        self.blocks.push(wrapped);
    }
}

fn heading_u8(level: HeadingLevel) -> u8 {
    level as u8
}

fn line_at(body: &str, offset: usize) -> usize {
    body.get(..offset.min(body.len()))
        .unwrap_or("")
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
}

fn styled_text(s: &str, style: Inline, as_code: bool) -> Element<'static, Message> {
    let size = match style.heading {
        1 => 32.0,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        n if n > 0 => 16.0,
        _ => 15.0,
    };
    let font = if as_code || style.code {
        font::mono()
    } else if style.heading > 0 || style.bold {
        font::bold()
    } else if style.italic || style.strike || style.quote {
        Font {
            style: Style::Italic,
            ..font::default()
        }
    } else {
        font::default()
    };
    widget::text(s.to_string()).size(size).font(font).into()
}

fn view_image(src: &str, files: &Path) -> Element<'static, Message> {
    if let Some(path) = resolve_image(src, files) {
        widget::image(widget::image::Handle::from_path(path))
            .width(Length::Fill)
            .height(Length::Fixed(220.0))
            .into()
    } else {
        widget::text::caption(format!("missing image · {src}")).into()
    }
}

fn view_table(rows: Vec<Vec<String>>) -> Element<'static, Message> {
    let mut col = widget::column::with_capacity(rows.len())
        .spacing(cosmic::theme::spacing().space_xxs)
        .width(Length::Fill);
    for (i, row) in rows.into_iter().enumerate() {
        let cells: Vec<Element<'static, Message>> = row
            .into_iter()
            .map(|cell| {
                let t = if i == 0 {
                    widget::text(cell).font(font::bold())
                } else {
                    widget::text(cell)
                };
                t.size(14.0).into()
            })
            .collect();
        col = col.push(widget::flex_row(cells).width(Length::Fill));
        if i == 0 {
            col = col.push(widget::divider::horizontal::default());
        }
    }
    col.into()
}

fn looks_like_image(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".svg")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wiki_alias() {
        let links = wikilinks("see [[M3 Note|the occ]] and [[Untitled]]");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "M3 Note");
    }

    #[test]
    fn normalize_strips_path_and_marks() {
        assert_eq!(normalize_target("  Foo Bar.md "), "foo bar");
        assert_eq!(normalize_target("Sub/Page#anchor|alias"), "sub/page");
        assert_eq!(normalize_target("Sub\\Page.MD"), "sub/page");
    }

    #[test]
    fn lists_headings_in_order() {
        let body = "intro\n\n# First\nwords\n\n## Nested `code` here\n\nnot a heading";
        let hs = headings(body);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].line, 2);
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[0].text, "First");
        assert_eq!(hs[1].line, 5);
        assert_eq!(hs[1].level, 2);
        assert_eq!(hs[1].text, "Nested code here");
        assert!(headings("plain text").is_empty());
    }

    #[test]
    fn reading_time_rounds_up() {
        assert_eq!(reading_minutes(0), 0);
        assert_eq!(reading_minutes(200), 1);
        assert_eq!(reading_minutes(450), 2);
    }

    #[test]
    fn toggles_checkbox() {
        let body = "- [ ] write\n- [x] done";
        let next = toggle_task_line(body, 0);
        assert!(next.contains("- [x] write"));
    }

    fn caret(text: &str, line: usize, column: usize) -> (String, usize, usize) {
        let got = list_enter(text, line, column).expect("list enter");
        (got.text, got.line, got.column)
    }

    #[test]
    fn enter_continues_bullet() {
        let (text, line, col) = caret("- foo", 0, 5);
        assert_eq!(text, "- foo\n- ");
        assert_eq!((line, col), (1, 2));
    }

    #[test]
    fn enter_continues_ordered_and_task() {
        let (text, line, col) = caret("1. foo", 0, 6);
        assert_eq!(text, "1. foo\n2. ");
        assert_eq!((line, col), (1, 3));
        let (text, _, col) = caret("- [x] done", 0, 10);
        assert_eq!(text, "- [x] done\n- [ ] ");
        assert_eq!(col, 6);
    }

    #[test]
    fn enter_keeps_indent_quote_and_splits() {
        let (text, line, col) = caret("  - foo", 0, 7);
        assert_eq!(text, "  - foo\n  - ");
        assert_eq!((line, col), (1, 4));
        let (text, _, col) = caret("> - foo", 0, 7);
        assert_eq!(text, "> - foo\n> - ");
        assert_eq!(col, 4);
        let (text, _, col) = caret("- foobar", 0, 5);
        assert_eq!(text, "- foo\n- bar");
        assert_eq!(col, 2);
    }

    #[test]
    fn enter_on_empty_item_exits_list() {
        let (text, line, col) = caret("- ", 0, 2);
        assert_eq!(text, "");
        assert_eq!((line, col), (0, 0));
        assert!(list_enter("hello", 0, 5).is_none());
        assert!(list_enter("```\n- foo\n```", 1, 5).is_none());
        assert!(list_enter("*italic*", 0, 8).is_none());
    }
}
