//! Source editor: markdown characters stay visible and are styled in place.
//! WYSIWYG is the same buffer with markers omitted from the paint.

use cosmic::font;
use cosmic::iced::core::text::{self, Paragraph, Span, Text};
use cosmic::iced::core::widget::{tree, Tree, Widget};
use cosmic::iced::core::time::{Duration, Instant};
use cosmic::iced::core::window;
use cosmic::iced::core::{
    alignment, clipboard, layout, mouse, renderer, Clipboard, Event, Layout, Length, Pixels,
    Rectangle, Shell, Size,
};
use cosmic::iced::core::renderer::Renderer as _;
use cosmic::iced::core::text::Renderer as _;
use cosmic::iced::keyboard;
use cosmic::iced::widget::text_editor::{
    self, Action, Binding, Content, Edit, KeyPress, Status,
};
use cosmic::iced::{Color, Font};
use cosmic::Element;
use std::ops::Range;
use std::sync::Arc;

use crate::highlight::{self, MdKind};
use crate::smooth_scroll;
use crate::vim;
use crate::Message;

pub fn view<'a>(content: &'a Content, vim: &'a vim::State) -> Element<'a, Message> {
    view_mode(content, false, vim)
}

pub fn view_write<'a>(content: &'a Content, vim: &'a vim::State) -> Element<'a, Message> {
    view_mode(content, true, vim)
}

fn view_mode<'a>(content: &'a Content, formatted: bool, vim: &'a vim::State) -> Element<'a, Message> {
    smooth_scroll::vertical(
        Element::new(SourceEditor {
            content,
            formatted,
            vim_enabled: vim.enabled,
            vim_mode: vim.mode,
            on_action: Box::new(Message::Body),
            on_caret: Box::new(Message::SetCaret),
            on_select: Box::new(|from_line, from_col, line, col| {
                Message::DragSelect(from_line, from_col, line, col)
            }),
            on_wiki: Box::new(Message::OpenWiki),
            on_vim_mode: Box::new(Message::VimMode),
            on_vim_keys: Box::new(Message::VimKeys),
            on_vim_run: Box::new(Message::VimRun),
            on_undo: Box::new(|| Message::Undo),
            on_redo: Box::new(|| Message::Redo),
        }),
        Length::Fill,
    )
}

struct SourceEditor<'a> {
    content: &'a Content,
    formatted: bool,
    vim_enabled: bool,
    vim_mode: vim::Mode,
    on_action: Box<dyn Fn(Action) -> Message + 'a>,
    on_caret: Box<dyn Fn(usize, usize) -> Message + 'a>,
    on_select: Box<dyn Fn(usize, usize, usize, usize) -> Message + 'a>,
    on_wiki: Box<dyn Fn(String) -> Message + 'a>,
    on_vim_mode: Box<dyn Fn(vim::Mode) -> Message + 'a>,
    on_vim_keys: Box<dyn Fn(String) -> Message + 'a>,
    on_vim_run: Box<dyn Fn(String) -> Message + 'a>,
    on_undo: Box<dyn Fn() -> Message + 'a>,
    on_redo: Box<dyn Fn() -> Message + 'a>,
}

struct State<P: Paragraph> {
    paragraphs: Vec<P>,
    line_heights: Vec<f32>,
    kinds: Vec<Vec<(Range<usize>, MdKind)>>,
    snapshot: String,
    width: f32,
    focused: bool,
    command: bool,
    window_focused: bool,
    now: Instant,
    blink_at: Instant,
    formatted: bool,
    vim_pending: String,
    shift: bool,
    /// Left-button gesture: origin in the text, pointer start, and whether it became a drag.
    press: Option<Press>,
    last_click: Option<(Instant, f32, f32, u8)>,
}

struct Press {
    line: usize,
    col: usize,
    x: f32,
    y: f32,
    selecting: bool,
}

impl<P: Paragraph> Default for State<P> {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            paragraphs: Vec::new(),
            line_heights: Vec::new(),
            kinds: Vec::new(),
            snapshot: String::new(),
            width: 0.0,
            focused: true,
            command: false,
            window_focused: true,
            now,
            blink_at: now,
            formatted: false,
            vim_pending: String::new(),
            shift: false,
            press: None,
            last_click: None,
        }
    }
}

impl<P: Paragraph> State<P> {
    const BLINK_MS: u128 = 500;

    fn show_caret(&mut self) {
        let now = Instant::now();
        self.now = now;
        self.blink_at = now;
    }

    fn caret_visible(&self) -> bool {
        self.focused
            && self.window_focused
            && (self
                .now
                .checked_duration_since(self.blink_at)
                .unwrap_or_default()
                .as_millis() / Self::BLINK_MS).is_multiple_of(2)
    }

    fn schedule_blink(&self, shell: &mut Shell<'_, Message>) {
        if !self.focused || !self.window_focused {
            return;
        }
        let elapsed = self
            .now
            .checked_duration_since(self.blink_at)
            .unwrap_or_default()
            .as_millis();
        let until = Self::BLINK_MS - (elapsed % Self::BLINK_MS);
        shell.request_redraw_at(self.now + Duration::from_millis(until as u64));
    }
}

impl<'a> Widget<Message, cosmic::Theme, cosmic::Renderer> for SourceEditor<'a> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<<cosmic::Renderer as text::Renderer>::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<<cosmic::Renderer as text::Renderer>::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &cosmic::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree
            .state
            .downcast_mut::<State<<cosmic::Renderer as text::Renderer>::Paragraph>>();
        let limits = limits.width(Length::Fill);
        let max = limits.max();
        let text = self.content.text();
        rebuild(state, renderer, &text, max.width, self.formatted);
        let content_height = state
            .paragraphs
            .iter()
            .map(|p| p.min_bounds().height.max(22.0))
            .sum::<f32>()
            .max(22.0)
            + 16.0;
        let height = content_height.max(limits.min().height).max(1.0);
        layout::Node::new(Size::new(max.width, height))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree
            .state
            .downcast_mut::<State<<cosmic::Renderer as text::Renderer>::Paragraph>>();
        let padding = 8.0;
        let inner = Rectangle {
            x: bounds.x + padding,
            y: bounds.y + padding,
            width: (bounds.width - padding * 2.0).max(0.0),
            height: (bounds.height - padding * 2.0).max(0.0),
        };

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                state.schedule_blink(shell);
            }
            Event::Window(window::Event::Focused) => {
                state.window_focused = true;
                state.show_caret();
                shell.request_redraw();
            }
            Event::Window(window::Event::Unfocused) => {
                state.window_focused = false;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(pos) = cursor.position_in(inner) {
                    state.focused = true;
                    state.show_caret();
                    if let Some((line, col, wiki)) = hit_line(
                        &state.paragraphs,
                        pos,
                        &self.content.text(),
                        &state.kinds,
                        state.formatted,
                    ) {
                        if state.command
                            && let Some(target) = wiki {
                                shell.publish((self.on_wiki)(target));
                                shell.capture_event();
                                shell.request_redraw();
                                return;
                            }
                        let now = Instant::now();
                        let count = match state.last_click {
                            Some((at, x, y, n))
                                if now.duration_since(at) < Duration::from_millis(400)
                                    && (pos.x - x).abs() < 6.0
                                    && (pos.y - y).abs() < 6.0 =>
                            {
                                n.saturating_add(1).min(3)
                            }
                            _ => 1,
                        };
                        state.last_click = Some((now, pos.x, pos.y, count));
                        match count {
                            2 => {
                                shell.publish((self.on_caret)(line, col));
                                shell.publish((self.on_action)(Action::SelectWord));
                                state.press = None;
                            }
                            3 => {
                                shell.publish((self.on_caret)(line, col));
                                shell.publish((self.on_action)(Action::SelectLine));
                                state.press = None;
                            }
                            _ if state.shift => {
                                let caret = self.content.cursor();
                                let origin = caret.selection.unwrap_or(caret.position);
                                state.press = Some(Press {
                                    line: origin.line,
                                    col: origin.column,
                                    x: pos.x,
                                    y: pos.y,
                                    selecting: true,
                                });
                                shell.publish((self.on_select)(
                                    origin.line,
                                    origin.column,
                                    line,
                                    col,
                                ));
                            }
                            _ => {
                                // Click moves the caret and drops any selection. Drag starts
                                // only after the pointer actually moves.
                                state.press = Some(Press {
                                    line,
                                    col,
                                    x: pos.x,
                                    y: pos.y,
                                    selecting: false,
                                });
                                shell.publish((self.on_caret)(line, col));
                            }
                        }
                        shell.capture_event();
                    }
                    shell.request_redraw();
                } else if !cursor.is_over(bounds) {
                    state.focused = false;
                    state.press = None;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let Some(pos) = pointer_in(cursor, inner) else {
                    return;
                };
                let (from_line, from_col) = {
                    let Some(press) = state.press.as_mut() else {
                        return;
                    };
                    if !press.selecting {
                        let dx = pos.x - press.x;
                        let dy = pos.y - press.y;
                        if dx.hypot(dy) < 4.0 {
                            return;
                        }
                        press.selecting = true;
                    }
                    (press.line, press.col)
                };
                if let Some((line, col, _)) = hit_line(
                    &state.paragraphs,
                    pos,
                    &self.content.text(),
                    &state.kinds,
                    state.formatted,
                ) {
                    shell.publish((self.on_select)(from_line, from_col, line, col));
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.press = None;
            }
            Event::Keyboard(keyboard::Event::KeyReleased { modifiers, .. }) => {
                state.command = modifiers.command();
                state.shift = modifiers.shift();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                state.command = modifiers.command();
                state.shift = modifiers.shift();
                if !state.focused {
                    return;
                }
                if self.vim_enabled {
                    let mut vs = vim::State {
                        enabled: true,
                        mode: self.vim_mode,
                        pending: std::mem::take(&mut state.vim_pending),
                    };
                    let effects = vim::handle(
                        &mut vs,
                        key,
                        *modifiers,
                        text.as_ref().map(|s| s.as_str()),
                    );
                    let swallow = self.vim_mode != vim::Mode::Insert || !effects.is_empty();
                    if swallow {
                        apply_vim(self, clipboard, shell, effects, &vs);
                        state.vim_pending = vs.pending;
                        state.show_caret();
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                    state.vim_pending = vs.pending;
                } else {
                    state.vim_pending.clear();
                }
                if modifiers.command() {
                    match key.as_ref() {
                        keyboard::Key::Character(c)
                            if matches!(c, "c" | "x" | "v" | "a" | "C" | "X" | "V" | "A") =>
                        {}
                        _ => return,
                    }
                }
                state.show_caret();
                shell.request_redraw();
                let press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status: Status::Focused {
                        is_hovered: cursor.is_over(bounds),
                    },
                };
                if let Some(binding) = Binding::<Message>::from_key_press(press) {
                    match binding {
                        Binding::Unfocus => state.focused = false,
                        Binding::Copy => {
                            if let Some(sel) = self.content.selection() {
                                clipboard.write(clipboard::Kind::Standard, sel);
                            }
                        }
                        Binding::Cut => {
                            if let Some(sel) = self.content.selection() {
                                clipboard.write(clipboard::Kind::Standard, sel);
                                shell.publish((self.on_action)(Action::Edit(Edit::Delete)));
                            }
                        }
                        Binding::Paste => {
                            if let Some(contents) = clipboard.read(clipboard::Kind::Standard) {
                                shell.publish((self.on_action)(Action::Edit(Edit::Paste(
                                    Arc::new(contents),
                                ))));
                            }
                        }
                        Binding::Custom(msg) => shell.publish(msg),
                        other => {
                            if let Some(action) = binding_action(other) {
                                shell.publish((self.on_action)(action));
                            }
                        }
                    }
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree
            .state
            .downcast_ref::<State<<cosmic::Renderer as text::Renderer>::Paragraph>>();
        let bounds = layout.bounds();
        let clip = match viewport.intersection(&bounds) {
            Some(c) => c,
            None => return,
        };
        let padding = 8.0;
        let y = bounds.y + padding;
        let x = bounds.x + padding;
        let caret = self.content.cursor();
        let caret_on = state.caret_visible();
        let text = self.content.text();
        draw_selection(
            renderer,
            theme,
            state,
            &text,
            caret,
            self.vim_enabled,
            self.vim_mode,
            x,
            y,
            clip,
        );

        let mut y = bounds.y + padding;

        for (i, para) in state.paragraphs.iter().enumerate() {
            let h = para.min_bounds().height.max(22.0);
            let line_bounds = Rectangle {
                x,
                y,
                width: (bounds.width - padding * 2.0).max(0.0),
                height: h,
            };
            if line_bounds.intersects(&clip) {
                renderer.fill_paragraph(para, line_bounds.position(), style.text_color, clip);
                if caret_on && caret.position.line == i {
                    let text = self.content.text();
                    let col = if state.formatted {
                        let line = text.split('\n').nth(i).unwrap_or("");
                        let kinds = state.kinds.get(i).map(Vec::as_slice).unwrap_or(&[]);
                        let visual = highlight::source_to_visual(line, kinds, caret.position.column);
                        visual.min(highlight::source_to_visual(line, kinds, line.len()))
                    } else {
                        caret.position.column
                    };
                    let pos = para
                        .cursor_position(0, col, text::Affinity::After)
                        .unwrap_or(cosmic::iced::Point::ORIGIN);
                    let caret_h = state.line_heights.get(i).copied().unwrap_or(22.0).max(18.0);
                    let block = self.vim_enabled && self.vim_mode.block_caret();
                    let next = para
                        .cursor_position(0, col.saturating_add(1), text::Affinity::After)
                        .map(|p| p.x)
                        .unwrap_or(pos.x + 8.0);
                    let caret_w = if block {
                        (next - pos.x).abs().max(7.0).min(18.0)
                    } else {
                        1.2
                    };
                    let rect = Rectangle::new(
                        cosmic::iced::Point::new(x + pos.x, y + pos.y),
                        Size::new(caret_w, caret_h),
                    );
                    if let Some(caret_clip) = clip.intersection(&rect) {
                        let color = if block {
                            Color {
                                r: style.text_color.r,
                                g: style.text_color.g,
                                b: style.text_color.b,
                                a: 0.38,
                            }
                        } else {
                            style.text_color
                        };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: caret_clip,
                                ..renderer::Quad::default()
                            },
                            color,
                        );
                    }
                }
            }
            y += h;
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &cosmic::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

fn apply_vim(
    editor: &SourceEditor<'_>,
    clipboard: &mut dyn Clipboard,
    shell: &mut Shell<'_, Message>,
    effects: Vec<vim::Effect>,
    vs: &vim::State,
) {
    for effect in effects {
        match effect {
            vim::Effect::Mode(mode) => {
                shell.publish((editor.on_vim_mode)(mode));
                if !mode.paints_selection() {
                    collapse_selection(editor, shell);
                }
            }
            vim::Effect::Action(action) => shell.publish((editor.on_action)(action)),
            vim::Effect::Undo => shell.publish((editor.on_undo)()),
            vim::Effect::Redo => shell.publish((editor.on_redo)()),
            vim::Effect::Copy => {
                if let Some(sel) = editor.content.selection() {
                    clipboard.write(clipboard::Kind::Standard, sel);
                }
                collapse_selection(editor, shell);
            }
            vim::Effect::Cut => {
                if let Some(sel) = editor.content.selection() {
                    clipboard.write(clipboard::Kind::Standard, sel);
                    shell.publish((editor.on_action)(Action::Edit(Edit::Delete)));
                }
            }
            vim::Effect::Paste => {
                if let Some(contents) = clipboard.read(clipboard::Kind::Standard) {
                    shell.publish((editor.on_action)(Action::Edit(Edit::Paste(Arc::new(
                        contents,
                    )))));
                }
            }
            vim::Effect::Run(cmd) => shell.publish((editor.on_vim_run)(cmd)),
            vim::Effect::Keys => {}
        }
    }
    shell.publish((editor.on_vim_keys)(vs.keys()));
}

fn collapse_selection(editor: &SourceEditor<'_>, shell: &mut Shell<'_, Message>) {
    let pos = editor.content.cursor().position;
    shell.publish((editor.on_caret)(pos.line, pos.column));
}

fn paint_sel_rects(
    renderer: &mut cosmic::Renderer,
    clip: Rectangle,
    origin: cosmic::iced::Point,
    rects: impl IntoIterator<Item = Rectangle>,
    fill: Color,
) {
    for rect in rects {
        let placed = Rectangle {
            x: origin.x + rect.x,
            y: origin.y + rect.y,
            width: rect.width.max(2.0),
            height: rect.height.max(2.0),
        };
        if let Some(sel_clip) = clip.intersection(&placed) {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: sel_clip,
                    ..renderer::Quad::default()
                },
                fill,
            );
        }
    }
}

fn visual_col(formatted: bool, line: &str, kinds: &[(Range<usize>, MdKind)], col: usize) -> usize {
    if formatted {
        highlight::source_to_visual(line, kinds, col)
            .min(highlight::source_to_visual(line, kinds, line.len()))
    } else {
        col.min(line.len())
    }
}

fn line_highlights<P: Paragraph>(para: &P, start: usize, end: usize) -> Vec<Rectangle> {
    let (a, b) = if start <= end { (start, end) } else { (end, start) };
    let mut rects = para.highlight(0, (a, text::Affinity::After), (b, text::Affinity::After));
    if rects.is_empty() {
        let p0 = para
            .cursor_position(0, a, text::Affinity::After)
            .unwrap_or(cosmic::iced::Point::ORIGIN);
        let p1 = para
            .cursor_position(0, b, text::Affinity::After)
            .unwrap_or(cosmic::iced::Point::new(p0.x + 8.0, p0.y));
        rects.push(Rectangle::new(
            cosmic::iced::Point::new(p0.x.min(p1.x), p0.y.min(p1.y)),
            Size::new((p0.x - p1.x).abs().max(2.0), 18.0),
        ));
    }
    rects
}

fn draw_selection<P: Paragraph>(
    renderer: &mut cosmic::Renderer,
    theme: &cosmic::Theme,
    state: &State<P>,
    text: &str,
    caret: text_editor::Cursor,
    vim_enabled: bool,
    vim_mode: vim::Mode,
    x: f32,
    y0: f32,
    clip: Rectangle,
) {
    if vim_enabled && matches!(vim_mode, vim::Mode::Normal | vim::Mode::Command) {
        return;
    }
    let Some(anchor) = caret.selection else {
        return;
    };
    let a = caret.position;
    let b = anchor;
    let (start, end) = if (a.line, a.column) <= (b.line, b.column) {
        (a, b)
    } else {
        (b, a)
    };
    let accent: Color = theme.cosmic().accent_color().into();
    let fill = Color { a: 0.28, ..accent };
    let block = vim_enabled && matches!(vim_mode, vim::Mode::VisualBlock);
    let full_line = vim_enabled && matches!(vim_mode, vim::Mode::VisualLine);
    let mut y = y0;
    let lines: Vec<&str> = text.split('\n').collect();
    let start_vis = {
        let line = lines.get(start.line).copied().unwrap_or("");
        let kinds = state.kinds.get(start.line).map(Vec::as_slice).unwrap_or(&[]);
        visual_col(state.formatted, line, kinds, start.column)
    };
    let end_vis = {
        let line = lines.get(end.line).copied().unwrap_or("");
        let kinds = state.kinds.get(end.line).map(Vec::as_slice).unwrap_or(&[]);
        visual_col(state.formatted, line, kinds, end.column)
    };
    let block_left = start_vis.min(end_vis);
    let block_right = start_vis.max(end_vis).max(block_left.saturating_add(1));
    for (i, para) in state.paragraphs.iter().enumerate() {
        let h = para.min_bounds().height.max(22.0);
        if i >= start.line && i <= end.line {
            let line = lines.get(i).copied().unwrap_or("");
            let kinds = state.kinds.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let len = visual_col(state.formatted, line, kinds, line.len());
            let (sc, ec) = if block {
                let left = block_left.min(len);
                (left, block_right.min(len).max(left))
            } else if full_line {
                (0, len)
            } else {
                let sc = if i == start.line { start_vis.min(len) } else { 0 };
                let ec = if i == end.line { end_vis.min(len) } else { len };
                (sc, ec)
            };
            paint_sel_rects(
                renderer,
                clip,
                cosmic::iced::Point::new(x, y),
                line_highlights(para, sc, ec),
                fill,
            );
        }
        y += h;
    }
}

fn rebuild<P: Paragraph<Font = Font>>(
    state: &mut State<P>,
    renderer: &cosmic::Renderer,
    text: &str,
    width: f32,
    formatted: bool,
) {
    if state.snapshot == text
        && (state.width - width).abs() < 0.5
        && state.formatted == formatted
        && !state.paragraphs.is_empty()
    {
        return;
    }
    state.snapshot = text.to_string();
    state.width = width;
    state.formatted = formatted;
    state.paragraphs.clear();
    state.line_heights.clear();
    let styled = highlight::styled_document(text);
    for (line, kinds) in text.split('\n').zip(styled.iter()) {
        let spans = line_spans(line, kinds, formatted);
        let lh = highlight::line_height_for(kinds.iter().map(|(_, k)| *k));
        state.line_heights.push(lh);
        let para = P::with_spans(Text {
            content: spans.as_slice(),
            bounds: Size::new(width.max(1.0), f32::MAX),
            size: Pixels(15.0),
            line_height: text::LineHeight::Absolute(Pixels(lh)),
            font: renderer.default_font(),
            align_x: text::Alignment::Default,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::Word,
            ellipsize: text::Ellipsize::None,
        });
        state.paragraphs.push(para);
    }
    state.kinds = styled;
}

fn line_spans(
    line: &str,
    kinds: &[(Range<usize>, MdKind)],
    formatted: bool,
) -> Vec<Span<'static, (), Font>> {
    let dim = Color {
        r: 0.55,
        g: 0.55,
        b: 0.55,
        a: 0.9,
    };
    let meta = Color {
        r: 0.64,
        g: 0.64,
        b: 0.64,
        a: 0.78,
    };
    let accent = Color {
        r: 0.42,
        g: 0.62,
        b: 0.98,
        a: 1.0,
    };

    let lh = highlight::line_height_for(kinds.iter().map(|(_, k)| *k));
    let mut spans = Vec::new();
    if kinds.is_empty() {
        spans.push(
            Span::new(line.to_string())
                .size(15.0)
                .line_height(text::LineHeight::Absolute(Pixels(lh))),
        );
        return spans;
    }
    for (range, kind) in kinds {
        let end = range.end.min(line.len());
        let start = range.start.min(end);
        if start >= end {
            continue;
        }
        let piece = line[start..end].to_string();
        if formatted && highlight::is_hidden_markup(*kind, &piece) {
            continue;
        }
        let size = highlight::type_size(*kind);
        let color = match kind {
            MdKind::Frontmatter => Some(meta),
            MdKind::Marker | MdKind::Quote | MdKind::Strike => Some(dim),
            MdKind::Link
            | MdKind::Wiki
            | MdKind::List
            | MdKind::Image
            | MdKind::Code
            | MdKind::Table
            | MdKind::Html => Some(accent),
            _ => None,
        };
        let font = match kind {
            MdKind::Heading(_) | MdKind::Bold => Some(font::bold()),
            MdKind::Italic | MdKind::Quote | MdKind::Strike => Some(Font {
                style: cosmic::iced::font::Style::Italic,
                ..font::default()
            }),
            MdKind::Code | MdKind::Image | MdKind::Html => Some(font::mono()),
            _ => None,
        };
        let mut span = Span::new(piece)
            .size(size)
            .line_height(text::LineHeight::Absolute(Pixels(lh)));
        if let Some(color) = color {
            span = span.color(color);
        }
        if let Some(font) = font {
            span = span.font(font);
        }
        if matches!(kind, MdKind::Strike) {
            span = span.strikethrough(true);
        }
        if matches!(kind, MdKind::Wiki | MdKind::Link) {
            span = span.underline(true);
        }
        spans.push(span);
    }
    if spans.is_empty() {
        spans.push(
            Span::new(String::new())
                .size(15.0)
                .line_height(text::LineHeight::Absolute(Pixels(lh))),
        );
    }
    spans
}

fn pointer_in(cursor: mouse::Cursor, inner: Rectangle) -> Option<cosmic::iced::Point> {
    cursor.position_in(inner).or_else(|| {
        cursor.position().map(|p| cosmic::iced::Point::new(p.x - inner.x, p.y - inner.y))
    })
}

fn hit_line<P: Paragraph>(
    paragraphs: &[P],
    pos: cosmic::iced::Point,
    text: &str,
    kinds: &[Vec<(Range<usize>, MdKind)>],
    formatted: bool,
) -> Option<(usize, usize, Option<String>)> {
    let mut y = 0.0;
    let lines: Vec<&str> = text.split('\n').collect();
    for (i, para) in paragraphs.iter().enumerate() {
        let h = para.min_bounds().height.max(22.0);
        if pos.y < y + h {
            let local = cosmic::iced::Point::new(pos.x.max(0.0), (pos.y - y).max(0.0));
            let line = lines.get(i).copied().unwrap_or("");
            let col = para
                .hit_test(local)
                .map(|hit| hit.cursor())
                .unwrap_or_else(|| {
                    if formatted {
                        highlight::source_to_visual(
                            line,
                            kinds.get(i).map(Vec::as_slice).unwrap_or(&[]),
                            line.len(),
                        )
                    } else {
                        line.len()
                    }
                });
            let col = if formatted {
                let line_kinds = kinds.get(i).map(Vec::as_slice).unwrap_or(&[]);
                highlight::visual_to_source(line, line_kinds, col)
            } else {
                col
            };
            let col = clamp_column(line, col);
            let wiki = wiki_at(line, col);
            return Some((i, col, wiki));
        }
        y += h;
    }
    Some(caret_at_end(text))
}

pub(crate) fn caret_at_end(text: &str) -> (usize, usize, Option<String>) {
    let last = text.split('\n').count().saturating_sub(1);
    let col = text.split('\n').nth(last).map(|l| l.len()).unwrap_or(0);
    (last, col, None)
}

fn wiki_at(line: &str, col: usize) -> Option<String> {
    let col = col.min(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' && bytes.get(i + 1) == Some(&b'[')
            && let Some(rel) = line[i + 2..].find("]]") {
                let close = i + 2 + rel + 2;
                if col >= i && col <= close {
                    let inner = &line[i + 2..close - 2];
                    let target = inner.split('|').next().unwrap_or(inner).trim();
                    if !target.is_empty() {
                        return Some(target.to_string());
                    }
                }
                i = close;
                continue;
            }
        i += 1;
    }
    None
}

/// cosmic-text panics if the caret byte index is past the line (or mid-char).
pub(crate) fn clamp_column(line: &str, col: usize) -> usize {
    let mut col = col.min(line.len());
    while col > 0 && !line.is_char_boundary(col) {
        col -= 1;
    }
    col
}

/// Place the caret with no selection.
///
/// Iced's `Content::move_to` updates the caret but ignores `selection: None`,
/// so cosmic-text keeps the previous selection anchor. `Action::Move` is the
/// public API that actually clears it; then `move_to` can jump to `line`/`column`.
pub(crate) fn place_caret(content: &mut Content, line: usize, column: usize) {
    if content.cursor().selection.is_some() {
        content.perform(Action::Move(text_editor::Motion::Left));
    }
    content.move_to(text_editor::Cursor {
        position: text_editor::Position { line, column },
        selection: None,
    });
}

pub(crate) fn move_cursor(
    content: &mut Content,
    line: usize,
    column: usize,
    selection: Option<text_editor::Position>,
) {
    match selection {
        None => place_caret(content, line, column),
        Some(sel) => content.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column },
            selection: Some(sel),
        }),
    }
}

pub(crate) fn clamp_caret(content: &mut Content) {
    let cursor = content.cursor();
    let last = content.line_count().saturating_sub(1);
    let line = cursor.position.line.min(last);
    let text = content
        .line(line)
        .map(|l| l.text.into_owned())
        .unwrap_or_default();
    let column = clamp_column(&text, cursor.position.column);
    let selection = cursor.selection.map(|p| {
        let sl = p.line.min(last);
        let st = content
            .line(sl)
            .map(|l| l.text.into_owned())
            .unwrap_or_default();
        text_editor::Position {
            line: sl,
            column: clamp_column(&st, p.column),
        }
    });
    if line != cursor.position.line
        || column != cursor.position.column
        || selection != cursor.selection
    {
        move_cursor(content, line, column, selection);
    }
}

pub(crate) fn prepare_formatted_edit(content: &mut Content, action: &Action) {
    clamp_caret(content);
    let cursor = content.cursor();
    if cursor.selection.is_some() {
        return;
    }
    let skip = match action {
        Action::Edit(Edit::Backspace) => true,
        Action::Edit(Edit::Delete) => false,
        _ => return,
    };
    let line_idx = cursor.position.line;
    let line = content
        .line(line_idx)
        .map(|l| l.text.into_owned())
        .unwrap_or_default();
    let kinds = line_kinds(&content.text(), line_idx);
    let column = if skip {
        highlight::skip_hidden_before_backspace(&line, &kinds, cursor.position.column)
    } else {
        highlight::skip_hidden_before_delete(&line, &kinds, cursor.position.column)
    };
    let column = clamp_column(&line, column);
    if column != cursor.position.column {
        place_caret(content, line_idx, column);
    }
}

pub(crate) fn snap_formatted_caret(content: &mut Content, prefer_end: bool) {
    clamp_caret(content);
    let cursor = content.cursor();
    let last = content.line_count().saturating_sub(1);
    let line_idx = cursor.position.line.min(last);
    let line = content
        .line(line_idx)
        .map(|l| l.text.into_owned())
        .unwrap_or_default();
    let kinds = line_kinds(&content.text(), line_idx);
    let column = clamp_column(
        &line,
        highlight::snap_hidden_column(&line, &kinds, cursor.position.column, prefer_end),
    );
    if column != cursor.position.column {
        move_cursor(content, line_idx, column, cursor.selection);
    }
}

pub(crate) fn motion_prefer_end(motion: text_editor::Motion) -> bool {
    matches!(
        motion,
        text_editor::Motion::Right
            | text_editor::Motion::WordRight
            | text_editor::Motion::End
            | text_editor::Motion::DocumentEnd
            | text_editor::Motion::Down
            | text_editor::Motion::PageDown
    )
}

fn line_kinds(text: &str, line: usize) -> Vec<(Range<usize>, MdKind)> {
    highlight::styled_document(text)
        .into_iter()
        .nth(line)
        .unwrap_or_default()
}

fn binding_action(binding: Binding<Message>) -> Option<Action> {
    Some(match binding {
        Binding::Move(m) => Action::Move(m),
        Binding::Select(m) => Action::Select(m),
        Binding::SelectWord => Action::SelectWord,
        Binding::SelectLine => Action::SelectLine,
        Binding::SelectAll => Action::SelectAll,
        Binding::Insert(c) => Action::Edit(Edit::Insert(c)),
        Binding::Enter => Action::Edit(Edit::Enter),
        Binding::Backspace => Action::Edit(Edit::Backspace),
        Binding::Delete => Action::Edit(Edit::Delete),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{caret_at_end, clamp_column, place_caret};
    use cosmic::iced::widget::text_editor::{self, Content};

    #[test]
    fn clamp_column_empty_rejects_dummy_space() {
        assert_eq!(clamp_column("", 0), 0);
        assert_eq!(clamp_column("", 1), 0);
    }

    #[test]
    fn clamp_column_utf8_snaps_to_char_boundary() {
        assert_eq!(clamp_column("é", 1), 0);
        assert_eq!(clamp_column("é", 2), 2);
        assert_eq!(clamp_column("é", 99), 2);
    }

    #[test]
    fn click_below_text_goes_to_last_character() {
        assert_eq!(caret_at_end("hello"), (0, 5, None));
        assert_eq!(caret_at_end("hi\nthere"), (1, 5, None));
        assert_eq!(caret_at_end(""), (0, 0, None));
        assert_eq!(caret_at_end("a\n"), (1, 0, None));
    }

    #[test]
    fn place_caret_clears_selection_that_move_to_none_leaves() {
        let mut content = Content::with_text("hello world");
        content.move_to(text_editor::Cursor {
            position: text_editor::Position {
                line: 0,
                column: 11,
            },
            selection: Some(text_editor::Position { line: 0, column: 0 }),
        });
        assert!(content.cursor().selection.is_some());

        content.move_to(text_editor::Cursor {
            position: text_editor::Position { line: 0, column: 5 },
            selection: None,
        });
        assert!(
            content.cursor().selection.is_some(),
            "iced move_to(None) must still leave the old selection (the bug we wrap)"
        );

        place_caret(&mut content, 0, 5);
        assert!(content.cursor().selection.is_none());
        assert_eq!(content.cursor().position.line, 0);
        assert_eq!(content.cursor().position.column, 5);
    }
}
