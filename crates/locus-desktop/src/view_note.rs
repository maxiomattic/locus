//! The open note: rendered Obsidian style, or its Markdown source in a
//! styled editor with a formatting toolbar. Parsing and the formatting
//! commands live in `locus_core::markdown`; this file only draws them.

use std::ops::Range;

use cosmic::iced::advanced::text::highlighter::{self, Highlighter};
use cosmic::iced::font::Style as FontStyle;
use cosmic::iced::widget::text::Span;
use cosmic::iced::{Alignment, Border, Color, Font, Length, Padding};
use cosmic::widget;
use cosmic::{Element, theme};
use locus_core::Msg;
use locus_core::markdown::{Block, Format, Link, ListMarker, SourceStyle, Span as MdSpan};

use crate::view::{accent, core, dim, warning};
use crate::{App, Message};

/// Body text size of a rendered note.
const BODY: f32 = 15.0;

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 28.0,
        2 => 23.0,
        3 => 20.0,
        4 => 17.0,
        _ => BODY,
    }
}

fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

fn italic(font: Font) -> Font {
    Font {
        style: FontStyle::Italic,
        ..font
    }
}

// ----- source editor -----

/// Colours Markdown syntax in the source editor, line by line. Remembers,
/// for each line, whether it starts inside a fenced code block, so an edit
/// re-highlights correctly from the changed line on.
pub(crate) struct MarkdownHighlighter {
    line: usize,
    /// `in_code[i]`: line `i` starts inside a code fence.
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

fn source_format(style: &SourceStyle, _theme: &cosmic::Theme) -> highlighter::Format<Font> {
    let base = cosmic::font::default();
    let (color, font) = match style {
        SourceStyle::Marker => (Some(dim()), None),
        SourceStyle::Heading(_) => (Some(accent()), Some(cosmic::font::bold())),
        SourceStyle::Bold => (None, Some(cosmic::font::bold())),
        SourceStyle::Italic => (None, Some(italic(base))),
        SourceStyle::BoldItalic => (None, Some(italic(cosmic::font::bold()))),
        SourceStyle::Strike | SourceStyle::Done => (Some(dim()), None),
        SourceStyle::Highlight => (Some(warning()), None),
        SourceStyle::Code => (Some(warning()), Some(cosmic::font::mono())),
        SourceStyle::Link | SourceStyle::Tag => (Some(accent()), None),
        SourceStyle::Quote => (Some(dim()), Some(italic(base))),
    };
    highlighter::Format { color, font }
}

/// The formatting toolbar above the source editor.
fn toolbar<'a>() -> Element<'a, Message> {
    let s = theme::spacing();
    let b = |label: &'static str, tip: &'static str, format: Format| -> Element<'a, Message> {
        widget::tooltip(
            widget::button::text(label)
                .padding([s.space_xxxs, s.space_xs])
                .on_press(core(Msg::FormatNote(format))),
            widget::text::caption(tip),
            widget::tooltip::Position::Bottom,
        )
        .into()
    };
    let gap = || {
        widget::space::horizontal()
            .width(Length::Fixed(f32::from(s.space_xs)))
            .into()
    };
    widget::row::with_children(vec![
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
    .spacing(s.space_xxxs)
    .align_y(Alignment::Center)
    .into()
}

/// The editor's own key handling, minus Ctrl+letter: those are app
/// shortcuts (Ctrl+B, Ctrl+E...), and some browsers deliver their letter
/// as text too. Copy, cut, paste, select all and undo stay the editor's.
fn editor_keys(
    press: cosmic::iced::widget::text_editor::KeyPress,
) -> Option<cosmic::iced::widget::text_editor::Binding<Message>> {
    use cosmic::iced::keyboard::Key;
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
    cosmic::iced::widget::text_editor::Binding::from_key_press(press)
}

/// The source editor and its toolbar.
pub(crate) fn source(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let editor = cosmic::iced::widget::text_editor(&app.content)
        .id(app.editor_id.clone())
        .on_action(Message::Editor)
        .padding(s.space_s)
        .size(BODY)
        .height(Length::Fill)
        .key_binding(editor_keys)
        .highlight_with::<MarkdownHighlighter>((), source_format);
    widget::column::with_capacity(2)
        .spacing(s.space_xs)
        .push(toolbar())
        .push(editor)
        .height(Length::Fill)
        .into()
}

// ----- rendered note -----

fn spans<'a>(spans: &[MdSpan], size: f32, bold: bool, muted: bool) -> Vec<Span<'a, Link, Font>> {
    spans
        .iter()
        .map(|sp| {
            let font = match (sp.code, sp.bold || bold, sp.italic) {
                (true, _, _) => cosmic::font::mono(),
                (false, true, true) => italic(cosmic::font::bold()),
                (false, true, false) => cosmic::font::bold(),
                (false, false, true) => italic(cosmic::font::default()),
                (false, false, false) => cosmic::font::default(),
            };
            let mut span = Span::new(sp.text.clone()).size(size).font(font);
            if muted || sp.strike {
                span = span.color(dim());
            }
            span = span.strikethrough(sp.strike);
            if sp.highlight {
                span = span.background(with_alpha(warning(), 0.3));
            }
            if sp.code {
                span = span
                    .size(size - 1.0)
                    .color(warning())
                    .background(with_alpha(dim(), 0.15));
            }
            if let Some(link) = &sp.link {
                span = span.color(accent()).link(link.clone());
                if !matches!(link, Link::Tag(_)) {
                    span = span.underline(true);
                }
            }
            span
        })
        .collect()
}

fn rich<'a>(parts: Vec<Span<'a, Link, Font>>) -> Element<'a, Message> {
    cosmic::iced::widget::rich_text(parts)
        .on_link_click(|link| core(Msg::FollowLink(link)))
        .width(Length::Fill)
        .into()
}

/// A tinted, rounded box (callouts, quotes, code).
fn tinted<'a>(content: impl Into<Element<'a, Message>>, tint: Color) -> Element<'a, Message> {
    let radius = theme::active().cosmic().radius_s();
    widget::container(content)
        .padding([10, 14])
        .width(Length::Fill)
        .class(theme::Container::custom(move |_| {
            widget::container::Style {
                background: Some(with_alpha(tint, 0.12).into()),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn callout_tint(kind: &str) -> Color {
    match kind {
        "warning" | "caution" | "attention" => warning(),
        "danger" | "error" | "bug" | "failure" | "fail" | "missing" => {
            theme::active().cosmic().destructive_text_color().into()
        }
        "success" | "check" | "done" => theme::active().cosmic().success_text_color().into(),
        _ => accent(),
    }
}

fn block<'a>(b: &Block) -> Element<'a, Message> {
    let line = b.line();
    let body: Element<'a, Message> = match b {
        Block::Heading {
            level, spans: sp, ..
        } => {
            let size = heading_size(*level);
            widget::container(rich(spans(sp, size, true, *level >= 6)))
                .padding(Padding {
                    top: if *level <= 2 { 8.0 } else { 4.0 },
                    ..Padding::ZERO
                })
                .into()
        }
        Block::Paragraph { spans: sp, .. } => rich(spans(sp, BODY, false, false)),
        Block::ListItem {
            marker,
            depth,
            spans: sp,
            ..
        } => {
            let done = *marker == ListMarker::Task(true);
            let lead: Element<'a, Message> = match marker {
                ListMarker::Bullet => widget::text::body("•").size(BODY).into(),
                ListMarker::Number(n) => widget::text::body(format!("{n}.")).size(BODY).into(),
                ListMarker::Task(checked) => widget::checkbox(*checked)
                    .on_toggle(move |_| core(Msg::ToggleTaskAt(line)))
                    .into(),
            };
            widget::row::with_capacity(3)
                .spacing(8)
                .align_y(Alignment::Start)
                .push(widget::space::horizontal().width(Length::Fixed(22.0 * *depth as f32)))
                .push(widget::container(lead).width(Length::Fixed(22.0)))
                .push(rich(spans(
                    &sp.iter()
                        .cloned()
                        .map(|mut s| {
                            s.strike |= done;
                            s
                        })
                        .collect::<Vec<_>>(),
                    BODY,
                    false,
                    done,
                )))
                .into()
        }
        Block::Quote { spans: sp, .. } => tinted(rich(spans(sp, BODY, false, true)), dim()),
        Block::Callout {
            kind,
            title,
            spans: sp,
            ..
        } => {
            let tint = callout_tint(kind);
            let head = widget::text::body(title.clone())
                .size(BODY)
                .font(cosmic::font::bold())
                .class(theme::Text::Color(tint));
            let mut col = widget::column::with_capacity(2).spacing(6).push(head);
            if !sp.is_empty() {
                col = col.push(rich(spans(sp, BODY, false, false)));
            }
            tinted(col, tint)
        }
        Block::Code { text, .. } => tinted(
            widget::text::body(text.clone())
                .font(cosmic::font::mono())
                .size(BODY - 1.0),
            dim(),
        ),
        Block::Verbatim { text, .. } => widget::text::body(text.clone())
            .font(cosmic::font::mono())
            .size(BODY - 1.0)
            .into(),
        Block::Rule { .. } => widget::divider::horizontal::default().into(),
    };
    // A click on a block opens the source there, as in Obsidian.
    widget::mouse_area(body)
        .on_press(core(Msg::EditNoteAt(line)))
        .into()
}

/// The note rendered. A click on text opens the source at that block; a
/// click below the text opens it at the end.
pub(crate) fn rendered<'a>(blocks: &[Block], last_line: usize) -> Element<'a, Message> {
    let s = theme::spacing();
    let mut col = widget::column::with_capacity(blocks.len() + 1);
    if blocks.is_empty() {
        col = col.push(
            widget::text::body("Empty note. Click to write.").class(theme::Text::Color(dim())),
        );
    }
    let mut prev: Option<&Block> = None;
    for b in blocks {
        // Items of one list sit close together; other blocks get a gap.
        let gap = match (prev, b) {
            (None, _) => 0.0,
            (Some(Block::ListItem { .. }), Block::ListItem { .. }) => 4.0,
            _ => f32::from(s.space_s),
        };
        col = col.push(widget::container(block(b)).padding(Padding {
            top: gap,
            ..Padding::ZERO
        }));
        prev = Some(b);
    }
    let page = widget::container(col)
        .padding([s.space_s, s.space_m])
        .max_width(860.0)
        .width(Length::Fill);
    widget::mouse_area(
        widget::scrollable(page)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(core(Msg::EditNoteAt(last_line)))
    .into()
}
