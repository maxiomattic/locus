//! GitHub Flavored Markdown in the source editor.
//!
//! Spec: [GFM](https://github.github.com/gfm/) (CommonMark + tables,
//! strikethrough, task lists, autolinks, footnotes, alerts). Extra:
//! Obsidian `[[wikilink]]` / `![[embed]]`. Smart punctuation is off so
//! characters in the buffer are never rewritten.

use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdKind {
    Text,
    Marker,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Wiki,
    Quote,
    List,
    Image,
    Table,
    Html,
    Frontmatter,
}

pub fn gfm_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_WIKILINKS
}

pub fn type_size(kind: MdKind) -> f32 {
    match kind {
        MdKind::Heading(1) => 32.0,
        MdKind::Heading(2) => 26.0,
        MdKind::Heading(3) => 22.0,
        MdKind::Heading(_) => 18.0,
        MdKind::Frontmatter => 11.0,
        MdKind::Marker => 13.0,
        _ => 15.0,
    }
}

pub fn line_height_for(kinds: impl IntoIterator<Item = MdKind>) -> f32 {
    kinds
        .into_iter()
        .map(|k| match k {
            MdKind::Heading(1) => 42.0,
            MdKind::Heading(2) => 34.0,
            MdKind::Heading(3) => 30.0,
            MdKind::Heading(_) => 26.0,
            MdKind::Frontmatter => 16.0,
            _ => 22.0,
        })
        .fold(22.0_f32, f32::max)
}

/// Syntax that WYSIWYG hides (hashes, `**`, list prefixes). Visible text stays.
pub fn is_hidden_markup(kind: MdKind, piece: &str) -> bool {
    match kind {
        MdKind::Marker => true,
        MdKind::List | MdKind::Quote => is_line_prefix(piece),
        _ => false,
    }
}

fn is_line_prefix(piece: &str) -> bool {
    let s = piece.trim_end().trim_start_matches([' ', '\t']);
    if s.is_empty() {
        return !piece.is_empty();
    }
    if matches!(
        s,
        "-" | "*"
            | "+"
            | ">"
            | "- [ ]"
            | "- [x]"
            | "- [X]"
            | "* [ ]"
            | "* [x]"
            | "* [X]"
            | "+ [ ]"
            | "+ [x]"
            | "+ [X]"
    ) {
        return true;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() && matches!(bytes[i], b'.' | b')') {
        let rest = s[i + 1..].trim();
        return rest.is_empty() || matches!(rest, "[ ]" | "[x]" | "[X]");
    }
    false
}

fn run_hidden(line: &str, range: &Range<usize>, kind: MdKind) -> bool {
    let start = range.start.min(line.len());
    let end = range.end.min(line.len());
    if start >= end {
        return false;
    }
    is_hidden_markup(kind, &line[start..end])
}

/// Byte column in the painted WYSIWYG line for a source column.
pub fn source_to_visual(line: &str, kinds: &[(Range<usize>, MdKind)], source_col: usize) -> usize {
    let source_col = source_col.min(line.len());
    if kinds.is_empty() {
        return source_col;
    }
    let mut visual = 0;
    for (range, kind) in kinds {
        let start = range.start.min(line.len());
        let end = range.end.min(line.len());
        if source_col <= start {
            return visual;
        }
        let hidden = run_hidden(line, range, *kind);
        if source_col < end {
            return if hidden {
                visual
            } else {
                visual + (source_col - start)
            };
        }
        if !hidden {
            visual += end - start;
        }
    }
    visual
}

/// Source column for a click/caret in the painted WYSIWYG line.
pub fn visual_to_source(line: &str, kinds: &[(Range<usize>, MdKind)], visual_col: usize) -> usize {
    if kinds.is_empty() {
        return visual_col.min(line.len());
    }
    let mut visual = 0;
    for (range, kind) in kinds {
        if run_hidden(line, range, *kind) {
            continue;
        }
        let start = range.start.min(line.len());
        let end = range.end.min(line.len());
        let len = end.saturating_sub(start);
        if visual_col < visual + len {
            return start + (visual_col - visual);
        }
        visual += len;
    }
    line.len()
}

/// Move a source caret off the interior of hidden markup.
pub fn snap_hidden_column(
    line: &str,
    kinds: &[(Range<usize>, MdKind)],
    col: usize,
    prefer_end: bool,
) -> usize {
    let mut col = col.min(line.len());
    loop {
        let next = kinds
            .iter()
            .find(|(range, kind)| {
                run_hidden(line, range, *kind) && col > range.start && col < range.end
            })
            .map(|(range, _)| if prefer_end { range.end.min(line.len()) } else { range.start });
        match next {
            Some(n) if n != col => col = n,
            _ => return col,
        }
    }
}

pub fn skip_hidden_before_backspace(
    line: &str,
    kinds: &[(Range<usize>, MdKind)],
    col: usize,
) -> usize {
    let mut col = col.min(line.len());
    while col > 0 && byte_hidden(line, kinds, col - 1) {
        col -= 1;
    }
    col
}

pub fn skip_hidden_before_delete(line: &str, kinds: &[(Range<usize>, MdKind)], col: usize) -> usize {
    let mut col = col.min(line.len());
    while col < line.len() && byte_hidden(line, kinds, col) {
        col += 1;
    }
    col
}

fn byte_hidden(line: &str, kinds: &[(Range<usize>, MdKind)], index: usize) -> bool {
    kinds.iter().any(|(range, kind)| {
        index >= range.start && index < range.end && run_hidden(line, range, *kind)
    })
}

pub fn styled_document(text: &str) -> Vec<Vec<(Range<usize>, MdKind)>> {
    let mut cells: Vec<Option<MdKind>> = vec![None; text.len()];
    let mut stack: Vec<(MdKind, Range<usize>)> = Vec::new();
    for (event, range) in Parser::new_ext(text, gfm_options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(kind) = start_kind(&tag) {
                    stack.push((kind, range));
                }
            }
            Event::End(end) => {
                if pops(&end)
                    && let Some((kind, span)) = stack.pop()
                {
                    fill_empty(&mut cells, &span, leftover_kind(kind));
                }
            }
            Event::Text(_) | Event::Html(_) | Event::InlineHtml(_) => {
                paint(&mut cells, &range, content_kind(&stack));
            }
            Event::Code(_) => {
                let ticks = tick_wrap(text, &range);
                fill_empty(&mut cells, &ticks, MdKind::Marker);
                paint(&mut cells, &range, MdKind::Code);
            }
            Event::SoftBreak | Event::HardBreak => {
                paint(&mut cells, &range, content_kind(&stack));
            }
            Event::Rule => fill_empty(&mut cells, &range, MdKind::Marker),
            Event::TaskListMarker(_) => paint(&mut cells, &range, MdKind::List),
            Event::FootnoteReference(_) => paint(&mut cells, &range, MdKind::Link),
            Event::InlineMath(_) | Event::DisplayMath(_) => paint(&mut cells, &range, MdKind::Code),
        }
    }
    if let Some(range) = yaml_frontmatter(text) {
        paint(&mut cells, &range, MdKind::Frontmatter);
    }
    split_lines(text, &cells)
}

/// Obsidian-style YAML at the start of a note: `---` … `---` on its own lines.
pub fn yaml_frontmatter(text: &str) -> Option<Range<usize>> {
    let bom = text.starts_with('\u{feff}');
    let body = if bom { &text['\u{feff}'.len_utf8()..] } else { text };
    let open = if body.starts_with("---\r\n") {
        5
    } else if body.starts_with("---\n") {
        4
    } else {
        return None;
    };
    let after = &body[open..];
    let mut offset = 0;
    for line in after.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']).trim();
        if content == "---" {
            let end = if bom { '\u{feff}'.len_utf8() } else { 0 } + open + offset + line.len();
            return Some(0..end.min(text.len()));
        }
        offset += line.len();
        if offset > 12_000 {
            break;
        }
    }
    None
}

#[cfg(test)]
fn styled_line(line: &str) -> Vec<(Range<usize>, MdKind)> {
    styled_document(line).into_iter().next().unwrap_or_default()
}

fn start_kind(tag: &Tag<'_>) -> Option<MdKind> {
    Some(match tag {
        Tag::Paragraph | Tag::List(_) | Tag::MetadataBlock(_) | Tag::DefinitionList => return None,
        Tag::Heading { level, .. } => MdKind::Heading(*level as u8),
        Tag::BlockQuote(_) => MdKind::Quote,
        Tag::CodeBlock(CodeBlockKind::Indented | CodeBlockKind::Fenced(_)) => MdKind::Code,
        Tag::HtmlBlock => MdKind::Html,
        Tag::Item => MdKind::List,
        Tag::FootnoteDefinition(_) => MdKind::Link,
        Tag::DefinitionListTitle | Tag::DefinitionListDefinition => MdKind::List,
        Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => MdKind::Table,
        Tag::Emphasis | Tag::Superscript | Tag::Subscript => MdKind::Italic,
        Tag::Strong => MdKind::Bold,
        Tag::Strikethrough => MdKind::Strike,
        Tag::Link {
            link_type: LinkType::WikiLink { .. },
            ..
        } => MdKind::Wiki,
        Tag::Link { .. } => MdKind::Link,
        Tag::Image { .. } => MdKind::Image,
    })
}

fn pops(end: &TagEnd) -> bool {
    !matches!(
        end,
        TagEnd::Paragraph | TagEnd::List(_) | TagEnd::MetadataBlock(_) | TagEnd::DefinitionList
    )
}

fn leftover_kind(kind: MdKind) -> MdKind {
    match kind {
        MdKind::List => MdKind::List,
        MdKind::Text => MdKind::Text,
        _ => MdKind::Marker,
    }
}

fn content_kind(stack: &[(MdKind, Range<usize>)]) -> MdKind {
    let mut heading = None;
    let mut inner = MdKind::Text;
    for (kind, _) in stack {
        match kind {
            MdKind::Heading(_) => heading = Some(*kind),
            MdKind::List => {}
            other => inner = *other,
        }
    }
    if matches!(
        inner,
        MdKind::Code | MdKind::Link | MdKind::Wiki | MdKind::Image | MdKind::Html
    ) {
        inner
    } else {
        heading.unwrap_or(inner)
    }
}

fn paint(cells: &mut [Option<MdKind>], range: &Range<usize>, kind: MdKind) {
    let start = range.start.min(cells.len());
    let end = range.end.min(cells.len());
    for cell in cells.iter_mut().take(end).skip(start) {
        *cell = Some(kind);
    }
}

fn fill_empty(cells: &mut [Option<MdKind>], range: &Range<usize>, kind: MdKind) {
    let start = range.start.min(cells.len());
    let end = range.end.min(cells.len());
    for cell in cells.iter_mut().take(end).skip(start) {
        if cell.is_none() {
            *cell = Some(kind);
        }
    }
}

fn tick_wrap(text: &str, range: &Range<usize>) -> Range<usize> {
    let b = text.as_bytes();
    let mut start = range.start;
    let mut end = range.end;
    while start > 0 && b[start - 1] == b'`' {
        start -= 1;
    }
    while end < b.len() && b[end] == b'`' {
        end += 1;
    }
    start..end
}

fn split_lines(text: &str, cells: &[Option<MdKind>]) -> Vec<Vec<(Range<usize>, MdKind)>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        let slice = if end <= cells.len() {
            &cells[start..end]
        } else {
            &[]
        };
        lines.push(coalesce(slice));
        start = end + 1;
    }
    lines
}

fn coalesce(cells: &[Option<MdKind>]) -> Vec<(Range<usize>, MdKind)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < cells.len() {
        let kind = cells[i].unwrap_or(MdKind::Text);
        let mut j = i + 1;
        while j < cells.len() && cells[j].unwrap_or(MdKind::Text) == kind {
            j += 1;
        }
        out.push((i..j, kind));
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<MdKind> {
        styled_document(src)
            .into_iter()
            .flatten()
            .map(|(_, k)| k)
            .collect()
    }

    fn has(src: &str, kind: MdKind) -> bool {
        kinds(src).into_iter().any(|k| k == kind)
    }

    #[test]
    fn heading_keeps_marker() {
        let spans = styled_line("# hello");
        assert!(spans.iter().any(|(_, k)| *k == MdKind::Marker));
        assert!(spans.iter().any(|(_, k)| *k == MdKind::Heading(1)));
    }

    #[test]
    fn yaml_frontmatter_is_quiet() {
        let src = "---\norigin: agent\ncreated: 2026-09-08\n---\n# Hello\n";
        assert!(yaml_frontmatter(src).is_some());
        let lines = styled_document(src);
        assert!(lines[0].iter().all(|(_, k)| *k == MdKind::Frontmatter));
        assert!(lines[1].iter().all(|(_, k)| *k == MdKind::Frontmatter));
        assert!(lines[2].iter().all(|(_, k)| *k == MdKind::Frontmatter));
        assert!(lines[3].iter().all(|(_, k)| *k == MdKind::Frontmatter));
        assert!(
            lines.iter().flatten().any(|(_, k)| *k == MdKind::Heading(1)),
            "{lines:?}"
        );
        assert!(yaml_frontmatter("# Hello\n\n---\n\nnext\n").is_none());
    }

    #[test]
    fn gfm_strikethrough_and_table() {
        assert!(has("~~gone~~", MdKind::Strike));
        assert!(has("| a | b |\n| --- | --- |\n| 1 | 2 |\n", MdKind::Table));
    }

    #[test]
    fn gfm_task_and_wiki() {
        assert!(has("- [ ] write", MdKind::List));
        assert!(has("see [[M3 Note]] please", MdKind::Wiki));
        assert!(has("**bold**", MdKind::Bold));
        assert!(has("`code`", MdKind::Code));
        assert!(has("[label](https://example.com)", MdKind::Link));
        assert!(has("![alt](pic.png)", MdKind::Image));
        assert!(has("> quote", MdKind::Quote));
        assert!(has("```\nfn x() {}\n```", MdKind::Code));
        assert!(has("note[^1]\n\n[^1]: hi\n", MdKind::Link));
    }

    #[test]
    fn does_not_eat_characters() {
        let src = "# hello **bold** and [[Note]]";
        let painted: String = styled_document(src)
            .iter()
            .zip(src.split('\n'))
            .flat_map(|(spans, line)| {
                spans.iter().map(|(r, _)| &line[r.start.min(line.len())..r.end.min(line.len())])
            })
            .collect();
        assert_eq!(painted, src);
    }

    #[test]
    fn wysiwyg_hides_markers_keeps_words() {
        assert!(is_hidden_markup(MdKind::Marker, "# "));
        assert!(is_hidden_markup(MdKind::Marker, "**"));
        assert!(is_hidden_markup(MdKind::List, "- "));
        assert!(is_hidden_markup(MdKind::List, "- [ ] "));
        assert!(!is_hidden_markup(MdKind::Heading(1), "hello"));
        assert!(!is_hidden_markup(MdKind::Bold, "bold"));
        assert!(!is_hidden_markup(MdKind::Wiki, "Note"));
    }

    fn visual_text(line: &str, kinds: &[(Range<usize>, MdKind)]) -> String {
        kinds
            .iter()
            .filter(|(range, kind)| !run_hidden(line, range, *kind))
            .map(|(range, _)| &line[range.start.min(line.len())..range.end.min(line.len())])
            .collect()
    }

    #[test]
    fn wysiwyg_maps_heading_bold_wiki() {
        let src = "# hello **bold** and [[Note]]";
        let kinds = &styled_document(src)[0];
        assert_eq!(visual_text(src, kinds), "hello bold and Note");
        let hello = src.find("hello").unwrap();
        assert_eq!(visual_to_source(src, kinds, 0), hello);
        assert_eq!(source_to_visual(src, kinds, hello), 0);
        let bold = src.find("bold").unwrap();
        assert_eq!(visual_to_source(src, kinds, "hello ".len()), bold);
        assert_eq!(
            snap_hidden_column(src, kinds, bold - 1, false),
            src.find("**").unwrap()
        );
        assert_eq!(snap_hidden_column(src, kinds, bold - 1, true), bold);
    }

    #[test]
    fn wysiwyg_backspace_skips_stars() {
        let src = "hello **bold**";
        let kinds = &styled_document(src)[0];
        let bold = src.find("bold").unwrap();
        assert_eq!(skip_hidden_before_backspace(src, kinds, bold), "hello ".len());
        assert_eq!(
            skip_hidden_before_delete(src, kinds, src.find("**").unwrap()),
            bold
        );
    }
}
