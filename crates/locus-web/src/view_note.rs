//! The open note: rendered Obsidian style, or its Markdown source in a
//! styled editor with a formatting toolbar. Parsing and the formatting
//! commands live in `locus_core::markdown`; this file only draws them, the
//! same way the desktop app does.

use std::ops::Range;

use iced::advanced::text::highlighter::{self, Highlighter};
use iced::font::{Style as FontStyle, Weight};
use iced::widget::text::Span;
use iced::widget::{
    Column, button, checkbox, column, container, mouse_area, rich_text, row, rule, scrollable,
    space, text, text_editor, tooltip,
};
use iced::{Border, Color, Element, Fill, Font, Length, Padding, Theme};
use locus_core::Msg;
use locus_core::markdown::{Block, Format, Link, ListMarker, SourceStyle, Span as MdSpan};

use crate::style::{self, Colors, with_alpha};
use crate::view::core;
use crate::{Message, Shell};

/// Body text size of a rendered note.
const BODY: f32 = 15.0;

/// The bundled faces (see `fonts/`). Fira Sans Regular comes with iced.
pub const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/FiraSans-Bold.ttf"),
    include_bytes!("../fonts/FiraSans-Italic.ttf"),
    include_bytes!("../fonts/FiraSans-BoldItalic.ttf"),
    include_bytes!("../fonts/FiraMono-Regular.ttf"),
];

const SANS: Font = Font::with_name("Fira Sans");
const MONO: Font = Font::with_name("Fira Mono");

fn bold() -> Font {
    Font {
        weight: Weight::Bold,
        ..SANS
    }
}

fn italic(font: Font) -> Font {
    Font {
        style: FontStyle::Italic,
        ..font
    }
}

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 28.0,
        2 => 23.0,
        3 => 20.0,
        4 => 17.0,
        _ => BODY,
    }
}

// ----- source editor -----

/// Colours Markdown syntax in the source editor, line by line. Remembers,
/// for each line, whether it starts inside a fenced code block, so an edit
/// re-highlights correctly from the changed line on.
pub struct MarkdownHighlighter {
    line: usize,
    in_code: Vec<bool>,
}

impl Highlighter for MarkdownHighlighter {
    type Settings = ();
    type Highlight = SourceStyle;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, SourceStyle)>;

    fn new(_: &()) -> Self {
        Self {
            line: 0,
            in_code: vec![false],
        }
    }

    fn update(&mut self, _: &()) {}

    fn change_line(&mut self, line: usize) {
        self.line = line.min(self.in_code.len() - 1);
        self.in_code.truncate(self.line + 1);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let in_code = self.in_code.get(self.line).copied().unwrap_or(false);
        let (styles, next) = locus_core::markdown::source_styles(line, in_code);
        self.line += 1;
        if self.in_code.len() > self.line {
            self.in_code[self.line] = next;
        } else {
            self.in_code.push(next);
        }
        styles.into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// Colours come from the theme, which `Colors::theme` builds: primary is
/// the accent and warning the code colour.
fn source_format(style: &SourceStyle, theme: &Theme) -> highlighter::Format<Font> {
    let p = theme.palette();
    let dim = with_alpha(p.text, 0.5);
    let (color, font) = match style {
        SourceStyle::Marker => (Some(dim), None),
        SourceStyle::Heading(_) => (Some(p.primary), Some(bold())),
        SourceStyle::Bold => (None, Some(bold())),
        SourceStyle::Italic => (None, Some(italic(SANS))),
        SourceStyle::BoldItalic => (None, Some(italic(bold()))),
        SourceStyle::Strike | SourceStyle::Done => (Some(dim), None),
        SourceStyle::Highlight => (Some(p.warning), None),
        SourceStyle::Code => (Some(p.warning), Some(MONO)),
        SourceStyle::Link | SourceStyle::Tag => (Some(p.primary), None),
        SourceStyle::Quote => (Some(dim), Some(italic(SANS))),
    };
    highlighter::Format { color, font }
}

fn toolbar<'a>(c: Colors) -> Element<'a, Message> {
    let b = |label: &'static str, tip: &'static str, format: Format| -> Element<'a, Message> {
        tooltip(
            button(text(label).size(13))
                .padding([3, 8])
                .style(style::quiet_button(c))
                .on_press(core(Msg::FormatNote(format))),
            container(text(tip).size(12).color(c.text))
                .padding([4, 8])
                .style(move |_| style::card(c)),
            tooltip::Position::Bottom,
        )
        .into()
    };
    let gap = || space::horizontal().width(10).into();
    iced::widget::Row::with_children(vec![
        b("B", "Bold (Ctrl+B)", Format::Bold),
        b("I", "Italic (Ctrl+I)", Format::Italic),
        b("S", "Strikethrough (Ctrl+Shift+X)", Format::Strike),
        b("H", "Highlight (Ctrl+Shift+H)", Format::Highlight),
        b("</>", "Code (Ctrl+`)", Format::Code),
        gap(),
        b("H1", "Heading 1 (Ctrl+1)", Format::Heading(1)),
        b("H2", "Heading 2 (Ctrl+2)", Format::Heading(2)),
        b("H3", "Heading 3 (Ctrl+3)", Format::Heading(3)),
        gap(),
        b("•", "Bulleted list", Format::Bullet),
        b("1.", "Numbered list", Format::Numbered),
        b("[ ]", "Task (Ctrl+L)", Format::Task),
        b(">", "Quote", Format::Quote),
        b("{ }", "Code block", Format::CodeBlock),
        gap(),
        b("[[ ]]", "Link to a note", Format::WikiLink),
        b("Link", "Web link", Format::Link),
    ])
    .spacing(2)
    .align_y(iced::Center)
    .into()
}

/// The editor's own key handling, minus Ctrl+letter: those are app
/// shortcuts (Ctrl+B, Ctrl+E...), and some browsers deliver their letter
/// as text too. Copy, cut, paste, select all and undo stay the editor's.
fn editor_keys(
    press: iced::widget::text_editor::KeyPress,
) -> Option<iced::widget::text_editor::Binding<Message>> {
    use iced::keyboard::Key;
    let ctrl = press.modifiers.control() || press.modifiers.logo();
    if ctrl && !press.modifiers.alt() {
        if let Key::Character(c) = &press.key {
            if !matches!(
                c.as_str(),
                "c" | "x" | "v" | "a" | "z" | "y" | "C" | "X" | "V" | "A" | "Z" | "Y"
            ) {
                return None;
            }
        }
    }
    iced::widget::text_editor::Binding::from_key_press(press)
}

/// The source editor and its toolbar.
pub fn source(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let editor = text_editor(&s.editor)
        .id("editor")
        .on_action(Message::Editor)
        .padding(12)
        .size(BODY)
        .height(Fill)
        .key_binding(editor_keys)
        .highlight_with::<MarkdownHighlighter>((), source_format);
    column![toolbar(c), editor].spacing(8).height(Fill).into()
}

// ----- rendered note -----

fn spans<'a>(
    c: Colors,
    spans: &[MdSpan],
    size: f32,
    heading: bool,
    muted: bool,
) -> Vec<Span<'a, Link, Font>> {
    spans
        .iter()
        .map(|sp| {
            let font = match (sp.code, sp.bold || heading, sp.italic) {
                (true, _, _) => MONO,
                (false, true, true) => italic(bold()),
                (false, true, false) => bold(),
                (false, false, true) => italic(SANS),
                (false, false, false) => SANS,
            };
            let mut span = Span::new(sp.text.clone())
                .size(size)
                .font(font)
                .color(if muted || sp.strike { c.muted } else { c.text })
                .strikethrough(sp.strike);
            if sp.highlight {
                span = span.background(with_alpha(c.warn, 0.3));
            }
            if sp.code {
                span = span
                    .size(size - 1.0)
                    .color(c.warn)
                    .background(with_alpha(c.muted, 0.15));
            }
            if let Some(link) = &sp.link {
                span = span.color(c.accent).link(link.clone());
                if !matches!(link, Link::Tag(_)) {
                    span = span.underline(true);
                }
            }
            span
        })
        .collect()
}

fn rich<'a>(parts: Vec<Span<'a, Link, Font>>) -> Element<'a, Message> {
    rich_text(parts)
        .on_link_click(|link| core(Msg::FollowLink(link)))
        .width(Fill)
        .into()
}

fn tinted<'a>(content: impl Into<Element<'a, Message>>, tint: Color) -> Element<'a, Message> {
    container(content)
        .padding([10, 14])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(with_alpha(tint, 0.12).into()),
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

fn callout_tint(c: Colors, kind: &str) -> Color {
    match kind {
        "warning" | "caution" | "attention" => c.warn,
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => c.error,
        "success" | "check" | "done" => c.ok,
        _ => c.accent,
    }
}

fn block<'a>(c: Colors, b: &Block) -> Element<'a, Message> {
    let line = b.line();
    let body: Element<'a, Message> = match b {
        Block::Heading {
            level, spans: sp, ..
        } => container(rich(spans(c, sp, heading_size(*level), true, *level >= 6)))
            .padding(Padding {
                top: if *level <= 2 { 8.0 } else { 4.0 },
                ..Padding::ZERO
            })
            .into(),
        Block::Paragraph { spans: sp, .. } => rich(spans(c, sp, BODY, false, false)),
        Block::ListItem {
            marker,
            depth,
            spans: sp,
            ..
        } => {
            let done = *marker == ListMarker::Task(true);
            let lead: Element<'a, Message> = match marker {
                ListMarker::Bullet => text("•").size(BODY).color(c.text).into(),
                ListMarker::Number(n) => text(format!("{n}.")).size(BODY).color(c.text).into(),
                ListMarker::Task(checked) => checkbox(*checked)
                    .on_toggle(move |_| core(Msg::ToggleTaskAt(line)))
                    .into(),
            };
            let sp: Vec<MdSpan> = sp
                .iter()
                .cloned()
                .map(|mut s| {
                    s.strike |= done;
                    s
                })
                .collect();
            row![
                space::horizontal().width(Length::Fixed(22.0 * *depth as f32)),
                container(lead).width(22),
                rich(spans(c, &sp, BODY, false, done)),
            ]
            .spacing(8)
            .into()
        }
        Block::Quote { spans: sp, .. } => tinted(rich(spans(c, sp, BODY, false, true)), c.muted),
        Block::Callout {
            kind,
            title,
            spans: sp,
            ..
        } => {
            let tint = callout_tint(c, kind);
            let mut col =
                column![text(title.clone()).size(BODY).font(bold()).color(tint)].spacing(6);
            if !sp.is_empty() {
                col = col.push(rich(spans(c, sp, BODY, false, false)));
            }
            tinted(col, tint)
        }
        Block::Code { text: t, .. } => tinted(
            text(t.clone()).font(MONO).size(BODY - 1.0).color(c.text),
            c.muted,
        ),
        Block::Verbatim { text: t, .. } => text(t.clone())
            .font(MONO)
            .size(BODY - 1.0)
            .color(c.text)
            .into(),
        Block::Rule { .. } => rule::horizontal(1).into(),
    };
    // A click on a block opens the source there, as in Obsidian.
    mouse_area(body)
        .on_press(core(Msg::EditNoteAt(line)))
        .into()
}

/// The note rendered. A click on text opens the source at that block; a
/// click below the text opens it at the end.
pub fn rendered<'a>(c: Colors, blocks: &[Block], last_line: usize) -> Element<'a, Message> {
    let mut col = Column::new();
    if blocks.is_empty() {
        col = col.push(text("Empty note. Click to write.").color(c.muted));
    }
    let mut prev: Option<&Block> = None;
    for b in blocks {
        // Items of one list sit close together; other blocks get a gap.
        let gap = match (prev, b) {
            (None, _) => 0.0,
            (Some(Block::ListItem { .. }), Block::ListItem { .. }) => 4.0,
            _ => 12.0,
        };
        col = col.push(container(block(c, b)).padding(Padding {
            top: gap,
            ..Padding::ZERO
        }));
        prev = Some(b);
    }
    let page = container(col).padding([8, 20]).max_width(860).width(Fill);
    mouse_area(scrollable(page).width(Fill).height(Fill))
        .on_press(core(Msg::EditNoteAt(last_line)))
        .into()
}
