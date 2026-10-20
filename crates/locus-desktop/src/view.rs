//! Drawing. Every function reads `locus_core::view_model` (plain data) and
//! turns it into libcosmic widgets. Clicks become `Msg`s the core already
//! understands; this file makes no decisions about app behaviour.
//!
//! Rows have fixed heights so the shell can compute where the selected row
//! is ([`selection_rect`]) and scroll it into view.

use cosmic::iced::{Alignment, Border, Color, ContentFit, Length};
use cosmic::widget::{self, Id};
use cosmic::{Element, theme};
use locus_core::state::{CalMode, Focus, InputKind, Level, Section};
use locus_core::view_model::{self, EditorView, FormView, HistoryView};
use locus_core::{Date, Msg, Nav, TextEdit};

use crate::{App, Message, Screen, THUMB_GRID, THUMB_VIEW, thumb_key, view_media, view_messages};

// ----- widget ids -----

/// Names the editor in focus bookkeeping; the widget itself uses
/// `App::editor_id`.
pub const EDITOR_ID: &str = "editor";
pub const FILTER_ID: &str = "filter";
pub const INPUT_ID: &str = "input";
pub const PALETTE_ID: &str = "palette";
pub const PHOTOS_SCROLL: &str = "scroll-photos";
pub(crate) const NOTES_SCROLL: &str = "scroll-notes";
pub(crate) const HISTORY_SCROLL: &str = "scroll-history";
pub(crate) const REVISION_SCROLL: &str = "scroll-revision";
pub(crate) const TODO_SCROLL: &str = "scroll-todo";
pub(crate) const AGENDA_SCROLL: &str = "scroll-agenda";
pub(crate) const FILES_SCROLL: &str = "scroll-files";
pub(crate) const CONTACTS_SCROLL: &str = "scroll-contacts";
pub(crate) const PALETTE_SCROLL: &str = "scroll-palette";
pub(crate) const MOVE_SCROLL: &str = "scroll-move";
pub(crate) const MOVIES_SCROLL: &str = "scroll-movies";
pub(crate) const MUSIC_SCROLL: &str = "scroll-music";
pub(crate) const CONVERSATIONS_SCROLL: &str = "scroll-conversations";
pub(crate) const CONVERSATION_H: f32 = 56.0;

pub fn form_id(i: usize) -> String {
    format!("form-{i}")
}

// ----- sizes -----

/// Horizontal room the page padding and scrollbar take from the grid.
pub const PAGE_PAD_X: f32 = 56.0;
/// Width of the list pane in Notes and Contacts.
pub(crate) const LIST_W: f32 = 300.0;
/// Content height of a two-line row and a one-line row. A list row is this
/// plus the list's vertical item padding plus a 1px divider.
pub(crate) const ROW2_H: f32 = 40.0;
pub(crate) const ROW1_H: f32 = 28.0;
/// Height of a group or day header in Todo and the agenda.
pub(crate) const HEADER_H: f32 = 28.0;
/// Height of the "hidden" caption under a collapsed group.
pub(crate) const NOTE_H: f32 = 20.0;
/// Target photo tile edge; the grid fits as many columns as the width allows.
pub(crate) const TILE: f32 = 168.0;
pub(crate) const GAP: f32 = 8.0;

pub(crate) fn scroll_msg(
    id: &'static str,
) -> impl Fn(cosmic::iced::widget::scrollable::Viewport) -> Message {
    move |vp| Message::Scrolled(id, vp)
}

pub(crate) fn core(msg: Msg) -> Message {
    Message::Core(msg)
}

pub(crate) fn set_text(value: String) -> Message {
    Message::Core(Msg::Text(TextEdit::Set(value)))
}

/// Distance from one list row's top to the next.
pub(crate) fn row_pitch(content_h: f32) -> f32 {
    content_h + 2.0 * f32::from(theme::spacing().space_xxs) + 1.0
}

pub fn photo_columns(width: f32) -> usize {
    ((width + GAP) / (TILE + GAP)).floor().max(1.0) as usize
}

/// Edge of a square tile so `cols` tiles fill `width`.
pub(crate) fn tile_edge(width: f32, cols: usize) -> f32 {
    let cols = cols.max(1) as f32;
    ((width - (cols - 1.0) * GAP) / cols).clamp(64.0, 2.0 * TILE)
}

/// Distance from one grid row's top to the next.
pub fn photo_pitch(width: f32, cols: usize) -> f32 {
    tile_edge(width, cols) + GAP
}

// ----- colors -----

pub(crate) fn danger() -> Color {
    theme::active().cosmic().destructive_text_color().into()
}

pub(crate) fn warning() -> Color {
    theme::active().cosmic().warning_text_color().into()
}

pub(crate) fn accent() -> Color {
    theme::active().cosmic().accent_text_color().into()
}

/// Text for something shown but not available.
pub(crate) fn dimmed() -> Color {
    let mut c: Color = theme::active().cosmic().on_bg_color().into();
    c.a = 0.4;
    c
}

pub(crate) fn colored<'a>(text: impl Into<String>, color: Color) -> Element<'a, Message> {
    widget::text::body(text.into())
        .class(theme::Text::Color(color))
        .into()
}

pub(crate) fn caption_colored<'a>(text: impl Into<String>, color: Color) -> Element<'a, Message> {
    widget::text::caption(text.into())
        .class(theme::Text::Color(color))
        .into()
}

pub(crate) fn level_text<'a>(text: String, level: Level) -> Element<'a, Message> {
    match level {
        Level::Error => caption_colored(text, danger()),
        Level::Warn => caption_colored(text, warning()),
        Level::Info => widget::text::caption(text).into(),
    }
}

// ----- shared pieces -----

pub(crate) fn text_button(label: &'static str, msg: Msg) -> Element<'static, Message> {
    widget::button::text(label).on_press(core(msg)).into()
}

/// The standard button look, with dimmed text when disabled. The stock
/// style only dims the background, so a disabled button read as enabled.
pub(crate) fn tool_class() -> theme::Button {
    use widget::button::Catalog;
    let std = || theme::Button::Standard;
    theme::Button::Custom {
        active: Box::new(move |focused, t| t.active(focused, false, &std())),
        disabled: Box::new(move |t| {
            let mut style = t.disabled(&std());
            let mut dim: Color = t.cosmic().button.on.into();
            dim.a = 0.4;
            style.text_color = Some(dim);
            style
        }),
        hovered: Box::new(move |focused, t| t.hovered(focused, false, &std())),
        pressed: Box::new(move |focused, t| t.pressed(focused, false, &std())),
    }
}

/// A labelled toolbar button with a tooltip naming its shortcut; `None`
/// draws it disabled.
pub(crate) fn tool(
    label: &'static str,
    tip: &'static str,
    msg: Option<Msg>,
) -> Element<'static, Message> {
    let b = widget::button::standard(label)
        .class(tool_class())
        .on_press_maybe(msg.map(core));
    if tip.is_empty() {
        return b.into();
    }
    widget::tooltip(
        b,
        widget::text::caption(tip),
        widget::tooltip::Position::Bottom,
    )
    .into()
}

/// A list row: a click sends `select`, a double click also `open`.
pub(crate) fn row_area<'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
    selected: bool,
    select: Msg,
    open: Msg,
) -> Element<'a, Message> {
    let radius = theme::active().cosmic().radius_s();
    let fill = widget::container(content)
        .height(Length::Fixed(height))
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .padding([0, theme::spacing().space_xxs])
        .class(theme::Container::custom(move |t| {
            let cosmic = t.cosmic();
            widget::container::Style {
                background: selected.then(|| {
                    let mut c: Color = cosmic.accent_color().into();
                    c.a = 0.2;
                    c.into()
                }),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }));
    widget::mouse_area(fill)
        .on_press(core(select.clone()))
        .on_double_click(core(Msg::Batch(vec![select, open])))
        .into()
}

pub(crate) fn icon_button(
    icon: &'static str,
    tip: &'static str,
    msg: Msg,
) -> Element<'static, Message> {
    widget::tooltip(
        widget::button::icon(widget::icon::from_name(icon)).on_press(core(msg)),
        widget::text::caption(tip),
        widget::tooltip::Position::Bottom,
    )
    .into()
}

pub(crate) fn card<'a>(
    content: impl Into<Element<'a, Message>>,
) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(theme::spacing().space_m)
        .class(theme::Container::Card)
        .width(Length::Fill)
}

/// Centered title and hint for an empty pane.
pub(crate) fn empty<'a>(title: &str, hint: String) -> Element<'a, Message> {
    let s = theme::spacing();
    widget::container(
        widget::column::with_capacity(2)
            .spacing(s.space_xxs)
            .align_x(Alignment::Center)
            .push(widget::text::title4(title.to_string()))
            .push(widget::text::caption(hint)),
    )
    .center(Length::Fill)
    .into()
}

/// What an empty list should say: loading, no filter matches, or `empty`.
pub(crate) fn empty_hint(app: &App, empty: &str) -> String {
    if app.ui.is_loading(app.ui.section) {
        "Loading...".into()
    } else if !app.ui.filter.is_empty() {
        format!("Nothing matches \"{}\".", app.ui.filter)
    } else {
        empty.into()
    }
}

/// A list row: clicking an unselected row selects it, clicking the selected
/// row activates it (open, enter, edit).
pub(crate) fn two_lines<'a>(title: String, subtitle: String) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .push(widget::text::body(title))
        .push(widget::text::caption(subtitle))
        .into()
}

pub(crate) fn scroll<'a>(
    id: &'static str,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    widget::scrollable(content)
        .id(Id::new(id))
        .on_scroll(scroll_msg(id))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Rows of a list drawn beyond each edge of the viewport, so a short scroll
/// does not wait for the next frame to show a row.
const OVERSCAN: usize = 6;

/// A list of `count` rows of one height, scrolled inside `id`. Only the rows
/// near the viewport are built (`build` is called for them alone), and spacers
/// stand in for the rest, so a list of thousands costs what its screenful
/// costs. Rows must all have `row_h` height.
pub(crate) fn virtual_list<'a>(
    app: &App,
    id: &'static str,
    count: usize,
    row_h: f32,
    build: impl Fn(usize) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let (offset, height) = app
        .viewports
        .get(id)
        .map(|&(offset, height, _)| (offset, height))
        .unwrap_or((0.0, app.default_view_height()));
    let first = ((offset / row_h).floor() as usize)
        .saturating_sub(OVERSCAN)
        .min(count);
    let last = (((offset + height) / row_h).ceil() as usize + OVERSCAN).min(count);
    let mut col = widget::column::with_capacity(last - first + 2);
    col = col.push(spacer(first as f32 * row_h));
    for i in first..last {
        col = col.push(build(i));
    }
    col = col.push(spacer((count - last) as f32 * row_h));
    scroll(id, col)
}

/// An empty block of `h` pixels, for the rows a virtual list is not drawing.
pub(crate) fn spacer<'a>(h: f32) -> Element<'a, Message> {
    widget::Space::new()
        .width(Length::Fill)
        .height(Length::Fixed(h.max(0.0)))
        .into()
}

/// A stack of pieces of mixed heights scrolled inside `id`, where `heights[i]`
/// is the height of the piece `build(i)`. As in [`virtual_list`], only the
/// pieces near the viewport are built, and spacers stand in for the rest. Pieces
/// must have exactly the heights given.
pub(crate) fn virtual_column<'a>(
    app: &App,
    id: &'static str,
    heights: &[f32],
    build: impl Fn(usize) -> Element<'a, Message>,
) -> Element<'a, Message> {
    let (offset, height) = app
        .viewports
        .get(id)
        .map(|&(offset, height, _)| (offset, height))
        .unwrap_or((0.0, app.default_view_height()));
    let w = view_model::row_window(heights, offset, height, OVERSCAN);
    let mut col = widget::column::with_capacity(w.last - w.first + 2);
    col = col.push(spacer(w.above));
    for i in w.first..w.last {
        col = col.push(build(i));
    }
    col = col.push(spacer(w.below));
    scroll(id, col)
}

/// `el` with a `gap`-pixel spacer under it, for a piece that ends a group.
fn with_gap<'a>(el: Element<'a, Message>, gap: f32) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .width(Length::Fill)
        .push(el)
        .push(spacer(gap))
        .into()
}

// ----- chrome -----

pub fn header_end() -> Vec<Element<'static, Message>> {
    vec![
        icon_button(
            "view-refresh-symbolic",
            "Reload (Ctrl+R or F5)",
            Msg::Reload,
        ),
        icon_button(
            "system-search-symbolic",
            "Command palette (Ctrl+K)",
            Msg::OpenPalette,
        ),
        icon_button(
            "help-browser-symbolic",
            "Keyboard shortcuts (F1)",
            Msg::ToggleHelp,
        ),
    ]
}

pub fn footer(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let ui = &app.ui;
    widget::container(level_text(
        view_model::status_line(ui),
        view_model::status_level(ui),
    ))
    .padding([s.space_xxs, s.space_s])
    .width(Length::Fill)
    .into()
}

/// The app's toolbar: Back and where you are, the search box, then the
/// app's actions.
pub(crate) fn app_toolbar(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let ui = &app.ui;
    let item = view_model::has_item_selected(ui);
    let deletable = view_model::can_delete(ui);
    let when = |ok: bool, msg: Msg| ok.then_some(msg);
    let mut bar = widget::row::with_capacity(12)
        .spacing(s.space_xxs)
        .align_y(Alignment::Center);
    if matches!(ui.section, Section::Notes | Section::Files) {
        bar = bar.push(widget::tooltip(
            widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                .on_press_maybe(view_model::can_go_up(ui).then(|| core(Msg::Nav(Nav::Left)))),
            widget::text::caption("Up one level (Backspace)"),
            widget::tooltip::Position::Bottom,
        ));
    }
    match ui.section {
        Section::Notes => {
            bar = bar
                .push(widget::text::title4(view_model::notes_breadcrumb(ui)).width(Length::Fill));
        }
        Section::Files => bar = bar.push(files_path(app)),
        // The Calendar app's two tabs sit where its title would be.
        Section::Calendar | Section::Todo => {
            bar = bar.push(tool_tab(
                "Calendar",
                ui.section == Section::Calendar,
                Msg::Switch(Section::Calendar),
            ));
            bar = bar.push(tool_tab(
                "Todo",
                ui.section == Section::Todo,
                Msg::Switch(Section::Todo),
            ));
            bar = bar.push(widget::space::horizontal().width(Length::Fill));
        }
        other => bar = bar.push(widget::text::title4(other.label()).width(Length::Fill)),
    }
    bar = bar.push(
        widget::search_input("Search", ui.filter.as_str())
            .id(Id::new(FILTER_ID))
            .on_input(|value| {
                core(Msg::Batch(vec![
                    Msg::FocusFilter,
                    Msg::Text(TextEdit::Set(value)),
                ]))
            })
            .on_clear(core(Msg::Batch(vec![
                Msg::FocusFilter,
                Msg::Text(TextEdit::Set(String::new())),
            ])))
            .width(Length::Fixed(220.0)),
    );
    let actions: Vec<Element<'_, Message>> = match ui.section {
        Section::Notes if !view_model::can_go_up(ui) => vec![
            tool("New notebook", "", Some(Msg::NewNotebook)),
            tool("New note", "Ctrl+N", Some(Msg::New)),
            tool(
                "Rename",
                "F2",
                when(view_model::can_rename(ui), Msg::Rename),
            ),
            tool("Delete", "Delete", when(deletable, Msg::Delete)),
        ],
        Section::Notes => vec![
            tool("New note", "Ctrl+N", Some(Msg::New)),
            tool("New folder", "", Some(Msg::NewFolder)),
            tool(
                "Rename",
                "F2",
                when(view_model::can_rename(ui), Msg::Rename),
            ),
            tool("Move", "", when(view_model::can_move(ui), Msg::Move)),
            tool(
                history_label(ui),
                "",
                when(view_model::can_show_history(ui), Msg::History),
            ),
            tool("Delete", "Delete", when(deletable, Msg::Delete)),
        ],
        Section::Movies => vec![
            tool("Upload", "Ctrl+O", Some(Msg::Upload)),
            tool("Play", "Enter", when(item, Msg::Activate)),
            tool("Rename", "F2", when(item, Msg::Rename)),
            tool("Move", "", when(item, Msg::Move)),
            tool("Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Music => vec![
            tool("Upload", "Ctrl+O", Some(Msg::Upload)),
            tool("Play", "Enter", when(item, Msg::Activate)),
            tool("Rename", "F2", when(item, Msg::Rename)),
            tool("Move", "", when(item, Msg::Move)),
            tool("Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Messages => vec![
            tool("New conversation", "Ctrl+N", Some(Msg::New)),
            tool("Delete message", "Delete", when(deletable, Msg::Delete)),
            tool("Refresh", "F5", Some(Msg::Reload)),
        ],
        Section::Todo => vec![
            tool("Add task", "Ctrl+N", Some(Msg::New)),
            tool("Open note", "Enter", when(item, Msg::Activate)),
            tool(
                if ui.todo.show_done {
                    "Hide done"
                } else {
                    "Show done"
                },
                "",
                Some(Msg::ToggleDoneTasks),
            ),
        ],
        Section::Calendar => vec![
            tool("New event", "Ctrl+N", Some(Msg::New)),
            tool("Edit", "F2", when(item, Msg::Edit)),
            tool("Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Photos => vec![
            tool("Upload", "Ctrl+O", Some(Msg::Upload)),
            tool(
                "Open",
                "Enter",
                when(item && ui.focus != Focus::Detail, Msg::Activate),
            ),
            tool("Rename", "F2", when(item, Msg::Rename)),
            tool("Move", "", when(item, Msg::Move)),
            tool("Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Files => vec![
            tool("Upload", "Ctrl+O", Some(Msg::Upload)),
            tool("New folder", "Ctrl+N", Some(Msg::New)),
            tool("Rename", "F2", when(item, Msg::Rename)),
            tool("Move", "", when(item, Msg::Move)),
            tool(
                "Copy path",
                "Ctrl+C",
                when(view_model::has_file_row_selected(ui), Msg::Yank),
            ),
            tool("Delete", "Delete", when(deletable, Msg::Delete)),
        ],
        Section::Contacts => vec![
            tool("New contact", "Ctrl+N", Some(Msg::New)),
            tool("Edit", "F2", when(item, Msg::Edit)),
            tool("Delete", "Delete", when(item, Msg::Delete)),
        ],
    };
    for action in actions {
        bar = bar.push(action);
    }
    bar.into()
}

/// One tab of the Calendar app: a flat button, highlighted when it is the one shown.
pub(crate) fn tool_tab(label: &'static str, current: bool, msg: Msg) -> Element<'static, Message> {
    let b = if current {
        widget::button::suggested(label)
    } else {
        widget::button::standard(label)
    };
    b.on_press_maybe((!current).then(|| core(msg))).into()
}

/// The Files path as clickable crumbs: each one goes that many levels up.
pub(crate) fn files_path(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let crumbs = view_model::breadcrumb(&app.ui);
    let depth = crumbs.len().saturating_sub(1);
    let mut path = widget::row::with_capacity(crumbs.len() * 2)
        .spacing(s.space_xxxs)
        .align_y(Alignment::Center)
        .width(Length::Fill);
    for (j, crumb) in crumbs.into_iter().enumerate() {
        if j > 1 {
            path = path.push(widget::text::caption("/"));
        }
        let up = (j < depth).then(|| core(Msg::Batch(vec![Msg::Nav(Nav::Left); depth - j])));
        // The default text-button padding spread `/ ZZ test` wide apart.
        path = path.push(
            widget::button::text(crumb.label)
                .padding([0, s.space_xxs])
                .on_press_maybe(up),
        );
    }
    path.into()
}

pub fn content(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    match app.screen {
        Screen::Connecting => widget::container(
            card(widget::text::body(format!(
                "Connecting to {}...",
                app.server
            )))
            .max_width(420.0),
        )
        .center(Length::Fill)
        .into(),
        Screen::Login => login(app),
        Screen::Main => {
            let mut col = widget::column::with_capacity(2)
                .spacing(s.space_s)
                .height(Length::Fill)
                .push(app_toolbar(app));
            col = col.push(match app.ui.section {
                Section::Notes => notes(app),
                Section::Todo => todo(app),
                Section::Calendar => calendar(app),
                Section::Photos => photos(app),
                Section::Files => files(app),
                Section::Contacts => contacts(app),
                Section::Movies => view_media::movies(app),
                Section::Music => view_media::music(app),
                Section::Messages => view_messages::messages(app),
            });
            widget::container(col)
                .padding([s.space_xxs, s.space_s, s.space_xs, s.space_s])
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }
}

pub(crate) fn login(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let code = view_model::input_prompt(&app.ui)
        .map(|v| v.text)
        .unwrap_or_default();
    let form = widget::column::with_capacity(8)
        .spacing(s.space_s)
        .push(widget::text::title3("Sign in to Locus"))
        .push(widget::text::caption(
            "The Locus gateway address (or the ship's own URL), and the ship's web code.",
        ))
        .push(widget::settings::item_row(vec![
            widget::text::body("Server")
                .width(Length::Fixed(96.0))
                .into(),
            widget::text_input("http://surface:8090", app.server.as_str())
                .on_input(Message::ServerEdit)
                .on_submit(|_| core(Msg::Submit))
                .width(Length::Fill)
                .into(),
        ]))
        .push(widget::settings::item_row(vec![
            widget::text::body("Web code")
                .width(Length::Fixed(96.0))
                .into(),
            widget::secure_input("sampel-ticlyt-migfun-falmel", code, None, true)
                .id(Id::new(INPUT_ID))
                .on_input(set_text)
                .width(Length::Fill)
                .into(),
        ]))
        .push(
            widget::row::with_capacity(2)
                .align_y(Alignment::Center)
                .push(
                    widget::container(level_text(app.ui.status.text.clone(), app.ui.status.level))
                        .width(Length::Fill),
                )
                .push(widget::button::suggested("Connect").on_press(core(Msg::Submit))),
        );
    widget::container(card(form).padding(s.space_l).max_width(520.0))
        .center(Length::Fill)
        .into()
}

// ----- notes -----

/// A note or notebook: its title over its subtitle, and a chevron for a folder.
fn note_row<'a>(row: &view_model::Row, i: usize) -> Element<'a, Message> {
    let content = widget::row::with_capacity(2)
        .align_y(Alignment::Center)
        .push(
            widget::container(two_lines(row.title.clone(), row.subtitle.clone()))
                .width(Length::Fill),
        );
    let content = if row.is_dir {
        content.push(widget::icon::from_name("go-next-symbolic").size(16).icon())
    } else {
        content
    };
    row_area(content, ROW2_H, row.selected, Msg::Select(i), Msg::Activate)
}

pub(crate) fn notes(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let rows = view_model::notes_list(&app.ui);
    let list: Element<'_, Message> = if rows.is_empty() {
        empty("No notes", empty_hint(app, "Click New note to write one."))
    } else {
        let count = rows.len();
        virtual_list(app, NOTES_SCROLL, count, ROW2_H, move |i| {
            note_row(&rows[i], i)
        })
    };
    let left = widget::container(list)
        .width(Length::Fixed(LIST_W))
        .height(Length::Fill);
    let right: Element<'_, Message> = match (
        view_model::note_history(&app.ui),
        view_model::editor(&app.ui),
    ) {
        (Some(history), _) => history_pane(history),
        (None, Some(editor)) => editor_pane(app, editor),
        (None, None) => empty("No note open", "Double-click a note to open it.".into()),
    };
    widget::row::with_capacity(2)
        .spacing(s.space_s)
        .push(left)
        .push(
            widget::container(right)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .into()
}

pub(crate) fn editor_pane(app: &App, editor: EditorView) -> Element<'_, Message> {
    let s = theme::spacing();
    let status = if editor.conflict {
        caption_colored("Changed elsewhere; your text is kept", warning())
    } else if editor.saving {
        widget::text::caption("Saving...").into()
    } else if editor.dirty {
        widget::text::caption("Unsaved").into()
    } else {
        widget::text::caption("Saved").into()
    };
    // Rename and Delete live in the toolbar: they act on the selected row,
    // which need not be the open note.
    let header = widget::row::with_capacity(2)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(widget::text::title4(editor.title).width(Length::Fill))
        .push(status);
    let text = widget::text_editor::text_editor(&app.content)
        .id(app.editor_id.clone())
        .on_action(Message::Editor)
        .padding(s.space_s)
        .height(Length::Fill);
    widget::column::with_capacity(2)
        .spacing(s.space_xs)
        .push(header)
        .push(text)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// The toolbar label of the History toggle.
pub(crate) fn history_label(ui: &locus_core::App) -> &'static str {
    if ui.notes.history.is_some() {
        "Close history"
    } else {
        "History"
    }
}

/// A note's saved revisions: the list on the left, the selected one's text
/// on the right. Read only.
pub(crate) fn history_pane(history: HistoryView) -> Element<'static, Message> {
    let s = theme::spacing();
    let header = widget::row::with_capacity(3)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(widget::text::title4(format!("History of {}", history.title)).width(Length::Fill))
        .push(tool(
            "Restore this version",
            "",
            history.can_restore.then_some(Msg::RestoreRevision),
        ))
        .push(tool("Close", "Escape", Some(Msg::History)));
    let body: Element<'static, Message> = if history.loading {
        empty("History", "Loading...".into())
    } else if history.rows.is_empty() {
        empty(
            "No saved revisions",
            "Earlier versions of this note appear here.".into(),
        )
    } else {
        let mut list = widget::list_column();
        for (i, row) in history.rows.into_iter().enumerate() {
            let content = two_lines(row.label, row.detail);
            list = list.add(row_area(
                content,
                ROW2_H,
                row.selected,
                Msg::SelectRevision(i),
                Msg::SelectRevision(i),
            ));
        }
        let text = widget::column::with_capacity(2)
            .spacing(s.space_xs)
            .push(widget::text::heading(history.rev_title))
            .push(widget::text::body(history.body));
        widget::row::with_capacity(2)
            .spacing(s.space_s)
            .push(
                widget::container(scroll(HISTORY_SCROLL, list))
                    .width(Length::Fixed(LIST_W))
                    .height(Length::Fill),
            )
            .push(scroll(
                REVISION_SCROLL,
                widget::container(text).padding(s.space_s),
            ))
            .into()
    };
    widget::column::with_capacity(2)
        .spacing(s.space_xs)
        .push(header)
        .push(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ----- todo -----

/// Todo groups shown: empty groups are left out.
pub(crate) fn todo_groups(app: &App) -> Vec<view_model::Group> {
    view_model::todo_groups(&app.ui)
        .into_iter()
        .filter(|g| g.count > 0)
        .collect()
}

/// One row of the Todo list, flattened out of its group so that each row can
/// be placed by height. `i` is the task's index across all shown groups.
enum TodoPiece {
    Header {
        title: &'static str,
        count: usize,
    },
    Hidden,
    Task {
        i: usize,
        task: view_model::TaskRow,
        last: bool,
    },
}

/// The Todo list as pieces with their heights, in order. A heading and a
/// hidden note carry the gap under them, and the last task of a group carries
/// the gap after the group.
fn todo_pieces(app: &App) -> Vec<(TodoPiece, f32)> {
    let sp = f32::from(theme::spacing().space_xs);
    let mut pieces = Vec::new();
    let mut index = 0usize;
    for group in todo_groups(app) {
        pieces.push((
            TodoPiece::Header {
                title: group.title,
                count: group.count,
            },
            HEADER_H + sp,
        ));
        if group.collapsed {
            pieces.push((TodoPiece::Hidden, NOTE_H + sp));
            continue;
        }
        let n = group.items.len();
        for (k, task) in group.items.into_iter().enumerate() {
            let last = k + 1 == n;
            let h = ROW2_H + if last { sp } else { 0.0 };
            pieces.push((
                TodoPiece::Task {
                    i: index,
                    task,
                    last,
                },
                h,
            ));
            index += 1;
        }
    }
    pieces
}

fn todo_piece<'a>(piece: &TodoPiece) -> Element<'a, Message> {
    let s = theme::spacing();
    let sp = f32::from(s.space_xs);
    match piece {
        TodoPiece::Header { title, count } => {
            let heading: Element<'a, Message> = if title.eq_ignore_ascii_case("overdue") {
                caption_colored(*title, danger())
            } else {
                widget::text::heading(*title).into()
            };
            let head: Element<'a, Message> = widget::container(
                widget::row::with_capacity(2)
                    .spacing(s.space_xs)
                    .align_y(Alignment::Center)
                    .push(heading)
                    .push(widget::text::caption(count.to_string())),
            )
            .height(Length::Fixed(HEADER_H))
            .align_y(Alignment::Center)
            .into();
            with_gap(head, sp)
        }
        TodoPiece::Hidden => {
            let note: Element<'a, Message> = widget::container(widget::text::caption(
                "Hidden. Click Show done to see them.",
            ))
            .height(Length::Fixed(NOTE_H))
            .into();
            with_gap(note, sp)
        }
        TodoPiece::Task { i, task, last } => {
            let i = *i;
            let check = widget::checkbox(task.done)
                .on_toggle(move |_| core(Msg::Batch(vec![Msg::Select(i), Msg::ToggleTask])));
            let text: Element<'a, Message> = if task.done {
                widget::text::body(task.text.clone())
                    .class(theme::Text::Color(dim()))
                    .into()
            } else {
                widget::text::body(task.text.clone()).into()
            };
            let due: Element<'a, Message> = if task.overdue {
                caption_colored(task.due_label.clone(), danger())
            } else {
                widget::text::caption(task.due_label.clone()).into()
            };
            let content = widget::row::with_capacity(3)
                .spacing(s.space_s)
                .align_y(Alignment::Center)
                .push(check)
                .push(
                    widget::column::with_capacity(2)
                        .width(Length::Fill)
                        .push(text)
                        .push(widget::text::caption(task.source_note_title.clone())),
                )
                .push(due);
            let row = row_area(
                content,
                ROW2_H,
                task.selected,
                Msg::Select(i),
                Msg::Activate,
            );
            if *last { with_gap(row, sp) } else { row }
        }
    }
}

pub(crate) fn todo(app: &App) -> Element<'_, Message> {
    let pieces = todo_pieces(app);
    if pieces.is_empty() {
        return empty(
            "No tasks",
            empty_hint(app, "Click Add task, or write - [ ] lines in a note."),
        );
    }
    let heights: Vec<f32> = pieces.iter().map(|&(_, h)| h).collect();
    virtual_column(app, TODO_SCROLL, &heights, move |i| {
        todo_piece(&pieces[i].0)
    })
}

pub(crate) fn dim() -> Color {
    let mut c: Color = theme::active().cosmic().on_bg_color().into();
    c.a = 0.5;
    c
}

// ----- calendar -----

pub(crate) fn calendar(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let mode = app.ui.calendar.mode;
    let month = view_model::month_grid(&app.ui);
    let mode_button = |label: &'static str, m: CalMode| -> Element<'_, Message> {
        let b = if mode == m {
            widget::button::suggested(label)
        } else {
            widget::button::standard(label)
        };
        b.on_press_maybe((mode != m).then(|| core(Msg::CalendarMode)))
            .into()
    };
    let header = widget::row::with_capacity(8)
        .spacing(s.space_xxs)
        .align_y(Alignment::Center)
        .push(icon_button(
            "go-previous-symbolic",
            "Previous month (Page Up)",
            Msg::ShiftMonth(-1),
        ))
        .push(
            widget::container(widget::text::title4(month.title.clone()))
                .width(Length::Fixed(180.0))
                .align_x(Alignment::Center),
        )
        .push(icon_button(
            "go-next-symbolic",
            "Next month (Page Down)",
            Msg::ShiftMonth(1),
        ))
        .push(text_button("Today", Msg::Today))
        .push(widget::space::horizontal())
        .push(mode_button("Agenda", CalMode::Agenda))
        .push(mode_button("Month", CalMode::Month));

    let mut col = widget::column::with_capacity(5)
        .spacing(s.space_xs)
        .height(Length::Fill)
        .push(header);
    if let Some(form) = view_model::form(&app.ui).filter(|f| f.title.contains("event")) {
        col = col.push(form_card(form));
    }
    match mode {
        CalMode::Agenda => {
            if let Some(label) = view_model::agenda_day_label(&app.ui) {
                col = col.push(widget::text::caption(label));
            }
            col = col.push(agenda(app));
        }
        CalMode::Month => {
            col = col.push(month_grid(month)).push(day_events(app));
        }
    }
    col.into()
}

pub(crate) fn event_row<'a>(time: String, title: String) -> Element<'a, Message> {
    widget::row::with_capacity(2)
        .spacing(s_s())
        .align_y(Alignment::Center)
        .push(widget::text::caption(time).width(Length::Fixed(100.0)))
        .push(widget::text::body(title).width(Length::Fill))
        .into()
}

pub(crate) fn s_s() -> u16 {
    theme::spacing().space_s
}

/// One row of the agenda, flattened out of its day so that each row can be
/// placed by height.
enum AgendaPiece {
    Day {
        label: String,
        selected: bool,
    },
    /// `j` is the event's index in its day. `date` is the day a click must
    /// select first, or `None` when the day is already selected.
    Event {
        time: String,
        title: String,
        selected: bool,
        j: usize,
        date: Option<Date>,
        last: bool,
    },
}

/// The agenda as pieces with their heights, in order. A day heading carries
/// the gap under it, and the last event of a day carries the gap after the day.
fn agenda_pieces(app: &App) -> Vec<(AgendaPiece, f32)> {
    let sp = f32::from(theme::spacing().space_xs);
    let mut pieces = Vec::new();
    for day in view_model::agenda(&app.ui) {
        let mut label = day.label.clone();
        if day.is_today {
            label.push_str(" · Today");
        }
        pieces.push((
            AgendaPiece::Day {
                label,
                selected: day.is_selected,
            },
            HEADER_H + sp,
        ));
        let n = day.events.len();
        let date = if day.is_selected {
            None
        } else {
            Some(day.date)
        };
        for (j, ev) in day.events.into_iter().enumerate() {
            let last = j + 1 == n;
            let h = ROW1_H + if last { sp } else { 0.0 };
            let piece = AgendaPiece::Event {
                time: ev.time_label,
                title: ev.title,
                selected: ev.selected,
                j,
                date,
                last,
            };
            pieces.push((piece, h));
        }
    }
    pieces
}

fn agenda_piece<'a>(piece: &AgendaPiece) -> Element<'a, Message> {
    let sp = f32::from(theme::spacing().space_xs);
    match piece {
        AgendaPiece::Day { label, selected } => {
            let heading: Element<'a, Message> = if *selected {
                widget::text::heading(label.clone())
                    .class(theme::Text::Color(accent()))
                    .into()
            } else {
                widget::text::heading(label.clone()).into()
            };
            let head: Element<'a, Message> = widget::container(heading)
                .height(Length::Fixed(HEADER_H))
                .align_y(Alignment::Center)
                .into();
            with_gap(head, sp)
        }
        AgendaPiece::Event {
            time,
            title,
            selected,
            j,
            date,
            last,
        } => {
            let select = match *date {
                Some(date) => Msg::Batch(vec![Msg::SelectDay(date), Msg::Select(*j)]),
                None => Msg::Select(*j),
            };
            let row = row_area(
                event_row(time.clone(), title.clone()),
                ROW1_H,
                *selected,
                select,
                Msg::Edit,
            );
            if *last { with_gap(row, sp) } else { row }
        }
    }
}

pub(crate) fn agenda(app: &App) -> Element<'_, Message> {
    let pieces = agenda_pieces(app);
    if pieces.is_empty() {
        return empty(
            "No events",
            empty_hint(
                app,
                "Nothing in the next 30 days. Click New event to add one.",
            ),
        );
    }
    let heights: Vec<f32> = pieces.iter().map(|&(_, h)| h).collect();
    virtual_column(app, AGENDA_SCROLL, &heights, move |i| {
        agenda_piece(&pieces[i].0)
    })
}

/// The selected day's events under the month grid, so Edit and Delete have
/// a visible target.
pub(crate) fn day_events(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let mut col =
        widget::column::with_capacity(2)
            .spacing(s.space_xxs)
            .push(widget::text::heading(view_model::selected_day_label(
                &app.ui,
            )));
    let day = view_model::agenda(&app.ui)
        .into_iter()
        .next()
        .filter(|d| d.is_selected);
    match day {
        None => {
            col = col.push(widget::text::caption(
                "No events. Click New event to add one.",
            ))
        }
        Some(day) => {
            let mut list = widget::list_column();
            for (j, ev) in day.events.into_iter().enumerate() {
                list = list.add(row_area(
                    event_row(ev.time_label, ev.title),
                    ROW1_H,
                    ev.selected,
                    Msg::Select(j),
                    Msg::Edit,
                ));
            }
            col = col.push(scroll(AGENDA_SCROLL, list));
        }
    }
    col.height(Length::Fill).into()
}

pub(crate) fn month_grid<'a>(month: view_model::MonthView) -> Element<'a, Message> {
    let s = theme::spacing();
    let mut col = widget::column::with_capacity(7).spacing(s.space_xxxs);
    let mut head = widget::row::with_capacity(7).spacing(s.space_xxxs);
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        head = head.push(
            widget::container(widget::text::caption(name))
                .width(Length::Fill)
                .align_x(Alignment::Center),
        );
    }
    col = col.push(head);
    for week in month.weeks {
        let mut row = widget::row::with_capacity(7).spacing(s.space_xxxs);
        for cell in week {
            let day: Element<'a, Message> = if cell.is_today && !cell.is_selected {
                widget::text::heading(cell.day.to_string())
                    .class(theme::Text::Color(accent()))
                    .into()
            } else if !cell.in_month {
                widget::text::body(cell.day.to_string())
                    .class(theme::Text::Color(dim()))
                    .into()
            } else {
                widget::text::body(cell.day.to_string()).into()
            };
            let dots = widget::text::caption("•".repeat(cell.event_count.min(4)));
            let content = widget::column::with_capacity(2)
                .align_x(Alignment::Center)
                .width(Length::Fill)
                .push(day)
                .push(dots);
            let class = if cell.is_selected {
                theme::Button::Suggested
            } else {
                theme::Button::Text
            };
            row = row.push(
                widget::button::custom(content)
                    .class(class)
                    .width(Length::Fill)
                    .height(Length::Fixed(52.0))
                    .on_press(core(Msg::SelectDay(cell.date))),
            );
        }
        col = col.push(row);
    }
    col.into()
}

/// The inline event or contact form.
pub(crate) fn form_card<'a>(form: FormView) -> Element<'a, Message> {
    let s = theme::spacing();
    let len = form.fields.len();
    let active = form.fields.iter().position(|f| f.active).unwrap_or(0);
    let mut grid = widget::column::with_capacity(len + 3)
        .spacing(s.space_xs)
        .push(widget::text::title4(form.title));
    for (i, field) in form.fields.into_iter().enumerate() {
        // Typing goes to the core's active field. A field clicked with the
        // mouse is not active yet, so step the core there before the text.
        let steps = (i + len - active) % len.max(1);
        let input = widget::text_input("", field.value)
            .id(Id::new(form_id(i)))
            .on_input(move |value| {
                let mut msgs = vec![Msg::NextField; steps];
                msgs.push(Msg::Text(TextEdit::Set(value)));
                core(Msg::Batch(msgs))
            })
            .width(Length::Fill);
        grid = grid.push(
            widget::row::with_capacity(2)
                .spacing(s.space_s)
                .align_y(Alignment::Center)
                .push(widget::text::body(field.label).width(Length::Fixed(110.0)))
                .push(input),
        );
    }
    if let Some(error) = form.error {
        grid = grid.push(colored(error, danger()));
    }
    grid = grid.push(
        widget::row::with_capacity(3)
            .spacing(s.space_xs)
            .align_y(Alignment::Center)
            .push(widget::space::horizontal())
            .push(widget::button::standard("Cancel").on_press(core(Msg::Escape)))
            .push(widget::button::suggested("Save").on_press(core(Msg::SaveForm))),
    );
    card(grid).max_width(640.0).into()
}

// ----- photos -----

pub(crate) fn photos(app: &App) -> Element<'_, Message> {
    if app.ui.focus == Focus::Detail {
        return viewer(app);
    }
    let cols = app.ui.photos.columns.max(1);
    let grid = view_model::photo_grid(&app.ui, cols);
    if grid.rows.is_empty() {
        return empty("No photos", empty_hint(app, "Click Upload to add photos."));
    }
    // Only the rows on screen are built; each one carries the gap below it, so
    // a row is exactly one pitch tall.
    let tile = tile_edge(app.photo_width(), cols);
    let radius = theme::active().cosmic().radius_s();
    let count = grid.rows.len();
    let rows = grid.rows;
    virtual_list(
        app,
        PHOTOS_SCROLL,
        count,
        photo_pitch(app.photo_width(), cols),
        move |r| photo_row(app, &rows[r], r, cols, tile, radius),
    )
}

/// One row of the photo grid: `cols` tiles, then the gap below them.
fn photo_row<'a>(
    app: &'a App,
    cells: &[view_model::PhotoCell],
    r: usize,
    cols: usize,
    tile: f32,
    radius: [f32; 4],
) -> Element<'a, Message> {
    let mut row = widget::row::with_capacity(cols).spacing(GAP);
    for (c, cell) in cells.iter().enumerate() {
        let index = r * cols + c;
        let thumb = cell
            .hash
            .as_deref()
            .and_then(|sha| app.thumbs.get(&thumb_key(sha, THUMB_GRID)));
        let inner: Element<'a, Message> = match thumb {
            Some(Some(handle)) => widget::image(handle.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            _ => widget::container(widget::text::caption(cell.title.clone()))
                .padding(theme::spacing().space_xs)
                .center(Length::Fill)
                .into(),
        };
        let selected = cell.selected;
        let framed = widget::container(inner)
            .width(Length::Fixed(tile))
            .height(Length::Fixed(tile))
            .padding(3)
            .class(theme::Container::custom(move |t| {
                let cosmic = t.cosmic();
                widget::container::Style {
                    background: Some(Color::from(cosmic.bg_component_color()).into()),
                    border: Border {
                        color: if selected {
                            cosmic.accent_color().into()
                        } else {
                            Color::TRANSPARENT
                        },
                        width: 3.0,
                        radius: radius.into(),
                    },
                    ..Default::default()
                }
            }));
        row = row.push(
            widget::mouse_area(framed)
                .on_press(core(Msg::Select(index)))
                .on_double_click(core(Msg::Batch(vec![Msg::Select(index), Msg::Activate]))),
        );
    }
    widget::column::with_capacity(2)
        .push(row)
        .push(spacer(GAP))
        .into()
}

pub(crate) fn viewer(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let Some(detail) = view_model::photo_detail(&app.ui) else {
        return empty("No photo", "Nothing to show.".into());
    };
    if detail.is_video {
        return view_media::video_viewer(app, &detail);
    }
    // The grid thumbnail stands in until the large one arrives.
    let handle = detail.hash.as_deref().and_then(|sha| {
        app.thumbs
            .get(&thumb_key(sha, THUMB_VIEW))
            .cloned()
            .flatten()
            .or_else(|| {
                app.thumbs
                    .get(&thumb_key(sha, THUMB_GRID))
                    .cloned()
                    .flatten()
            })
    });
    let picture: Element<'_, Message> = match handle {
        Some(handle) => widget::image(handle)
            .content_fit(ContentFit::Contain)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None if detail.hash.is_none() => {
            empty("No preview", "This photo has no stored image.".into())
        }
        // A failed fetch is stored as `None`.
        None if detail.hash.as_deref().is_some_and(|sha| {
            matches!(app.thumbs.get(&thumb_key(sha, THUMB_VIEW)), Some(None))
        }) =>
        {
            empty(
                "Could not load the photo",
                "The server did not send it.".into(),
            )
        }
        None => empty("Loading", "Fetching the photo.".into()),
    };
    let top = widget::row::with_capacity(5)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(tool("All photos", "Esc", Some(Msg::Escape)))
        .push(widget::text::title4(detail.title).width(Length::Fill))
        .push(widget::text::caption(detail.position))
        .push(tool("Previous", "Left", Some(Msg::Nav(Nav::Left))))
        .push(tool("Next", "Right", Some(Msg::Nav(Nav::Right))));
    let meta = detail.meta;
    widget::column::with_capacity(3)
        .spacing(s.space_xs)
        .height(Length::Fill)
        .push(top)
        .push(widget::container(picture).center(Length::Fill))
        .push(widget::text::caption(meta))
        .into()
}

// ----- files -----

/// One file or folder: icon, name, size, and when it changed.
fn file_row<'a>(row: &view_model::FileRow, i: usize) -> Element<'a, Message> {
    let s = theme::spacing();
    let icon = if row.is_dir {
        "folder-symbolic"
    } else {
        "text-x-generic-symbolic"
    };
    let size = if row.is_dir {
        "Folder".to_string()
    } else {
        row.size_label.clone()
    };
    let content = widget::row::with_capacity(4)
        .spacing(s.space_s)
        .align_y(Alignment::Center)
        .push(widget::icon::from_name(icon).size(16).icon())
        .push(widget::text::body(row.name.clone()).width(Length::Fill))
        .push(widget::text::caption(size).width(Length::Fixed(90.0)))
        .push(widget::text::caption(row.modified_label.clone()).width(Length::Fixed(100.0)));
    row_area(content, ROW1_H, row.selected, Msg::Select(i), Msg::Activate)
}

pub(crate) fn files(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let rows = view_model::file_rows(&app.ui);
    let body: Element<'_, Message> = if rows.is_empty() {
        empty(
            "Empty folder",
            empty_hint(app, "Click Upload to add files here."),
        )
    } else {
        let count = rows.len();
        let header = widget::row::with_capacity(3)
            .spacing(s.space_s)
            .padding([0, s.space_m])
            .push(widget::text::caption("Name").width(Length::Fill))
            .push(widget::text::caption("Size").width(Length::Fixed(90.0)))
            .push(widget::text::caption("Modified").width(Length::Fixed(100.0)));
        widget::column::with_capacity(2)
            .spacing(s.space_xxs)
            .push(header)
            .push(virtual_list(app, FILES_SCROLL, count, ROW1_H, move |i| {
                file_row(&rows[i], i)
            }))
            .into()
    };
    body
}

// ----- contacts -----

pub(crate) fn contacts(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let rows = view_model::contact_rows(&app.ui);
    let list: Element<'_, Message> = if rows.is_empty() {
        empty(
            "No contacts",
            empty_hint(app, "Click New contact to add one."),
        )
    } else {
        let count = rows.len();
        virtual_list(app, CONTACTS_SCROLL, count, ROW2_H, move |i| {
            let row = &rows[i];
            row_area(
                two_lines(row.name.clone(), row.subtitle.clone()),
                ROW2_H,
                row.selected,
                Msg::Select(i),
                Msg::Edit,
            )
        })
    };
    let left = widget::container(list)
        .width(Length::Fixed(LIST_W))
        .height(Length::Fill);
    let right: Element<'_, Message> =
        if let Some(form) = view_model::form(&app.ui).filter(|f| f.title.contains("contact")) {
            form_card(form)
        } else if let Some(detail) = view_model::contact_detail(&app.ui) {
            let field = |label: &'static str, value: String| -> Element<'_, Message> {
                widget::settings::item(label, widget::text::body(value)).into()
            };
            widget::column::with_capacity(2)
                .spacing(s.space_s)
                .push(
                    widget::row::with_capacity(3)
                        .spacing(s.space_xs)
                        .align_y(Alignment::Center)
                        .push(widget::text::title3(detail.name).width(Length::Fill))
                        .push(tool("Edit", "F2", Some(Msg::Edit)))
                        .push(tool("Delete", "Delete", Some(Msg::Delete))),
                )
                .push(
                    widget::list_column()
                        .add(field("Email", detail.email))
                        .add(field("Ship", detail.ship)),
                )
                .into()
        } else {
            empty(
                "No contact selected",
                "Pick a contact to see its details.".into(),
            )
        };
    widget::row::with_capacity(2)
        .spacing(s.space_s)
        .push(left)
        .push(
            widget::container(right)
                .width(Length::Fill)
                .height(Length::Fill)
                .max_width(720.0),
        )
        .into()
}

// ----- overlays -----

pub fn overlay(app: &App) -> Option<Element<'_, Message>> {
    let s = theme::spacing();
    let ui = &app.ui;
    match ui.focus {
        Focus::Confirm => {
            let confirm = view_model::confirm_view(ui)?;
            let yes = if confirm.destructive {
                widget::button::destructive(confirm.button)
            } else {
                widget::button::suggested(confirm.button)
            };
            Some(
                widget::dialog()
                    .title(confirm.prompt)
                    .body(confirm.detail)
                    .primary_action(yes.on_press(core(Msg::ConfirmAnswer(true))))
                    .secondary_action(
                        widget::button::standard("Cancel")
                            .on_press(core(Msg::ConfirmAnswer(false))),
                    )
                    .into(),
            )
        }
        Focus::Palette => {
            let rows = view_model::palette_rows(ui);
            let list: Element<'_, Message> = if rows.is_empty() {
                widget::text::caption("No matches").into()
            } else {
                let mut list = widget::list_column();
                for (i, row) in rows.into_iter().enumerate() {
                    let content = widget::row::with_capacity(3)
                        .spacing(s.space_s)
                        .align_y(Alignment::Center)
                        .push(widget::text::body(row.label))
                        .push(widget::text::caption(row.detail).width(Length::Fill))
                        .push(widget::text::caption(row.hint));
                    list = list.add(
                        widget::list::button(
                            widget::container(content)
                                .height(Length::Fixed(ROW1_H))
                                .align_y(Alignment::Center),
                        )
                        .selected(row.selected)
                        .on_press(core(Msg::Batch(vec![Msg::Select(i), Msg::Submit]))),
                    );
                }
                scroll(PALETTE_SCROLL, list)
            };
            let control = widget::column::with_capacity(2)
                .spacing(s.space_s)
                .push(
                    widget::search_input("Type a command or a name", view_model::palette_query(ui))
                        .id(Id::new(PALETTE_ID))
                        .on_input(set_text)
                        .width(Length::Fill),
                )
                .push(widget::container(list).height(Length::Fixed(360.0)));
            Some(
                widget::dialog()
                    .title("Command palette")
                    .control(control)
                    .width(Length::Fixed(600.0))
                    .into(),
            )
        }
        Focus::Help => {
            let mut list = widget::list_column();
            for (keys, what) in view_model::help(ui) {
                list = list.add(
                    widget::row::with_capacity(2)
                        .spacing(s.space_m)
                        .push(widget::text::body(keys).width(Length::Fixed(220.0)))
                        .push(widget::text::body(what)),
                );
            }
            Some(
                widget::dialog()
                    .title(format!("{} shortcuts", ui.section.label()))
                    .control(widget::container(widget::scrollable(list)).max_height(440.0))
                    .primary_action(
                        widget::button::standard("Close").on_press(core(Msg::ToggleHelp)),
                    )
                    .width(Length::Fixed(620.0))
                    .into(),
            )
        }
        Focus::Input(InputKind::MoveNote | InputKind::MoveNoteFolder) => {
            let picker = view_model::move_picker(ui)?;
            let list: Element<'_, Message> = if picker.rows.is_empty() {
                widget::text::caption("No matching folders").into()
            } else {
                let mut list = widget::list_column();
                for (i, row) in picker.rows.into_iter().enumerate() {
                    // The current place is shown dimmed and cannot be picked.
                    list = list.add(if row.enabled {
                        row_area(
                            widget::text::body(row.label),
                            ROW1_H,
                            row.selected,
                            Msg::Select(i),
                            Msg::Submit,
                        )
                    } else {
                        widget::container(colored(row.label, dimmed()))
                            .height(Length::Fixed(ROW1_H))
                            .width(Length::Fill)
                            .align_y(Alignment::Center)
                            .padding([0, s.space_xxs])
                            .into()
                    });
                }
                scroll(MOVE_SCROLL, list)
            };
            let control = widget::column::with_capacity(2)
                .spacing(s.space_s)
                .push(
                    widget::search_input("Filter folders", picker.filter)
                        .id(Id::new(INPUT_ID))
                        .on_input(set_text)
                        .width(Length::Fill),
                )
                .push(widget::container(list).height(Length::Fixed(320.0)));
            Some(
                widget::dialog()
                    .title(picker.title)
                    .control(control)
                    .primary_action(
                        widget::button::suggested("Move")
                            .on_press_maybe(picker.can_move.then(|| core(Msg::Submit))),
                    )
                    .secondary_action(
                        widget::button::standard("Cancel").on_press(core(Msg::Escape)),
                    )
                    .width(Length::Fixed(600.0))
                    .into(),
            )
        }
        // The message draft is a field in the thread, not a dialog.
        Focus::Input(kind)
            if !matches!(
                kind,
                InputKind::Filter | InputKind::Login | InputKind::MessageDraft
            ) =>
        {
            let prompt = view_model::input_prompt(ui)?;
            let control = widget::text_input(prompt.label, prompt.text)
                .id(Id::new(INPUT_ID))
                .on_input(set_text)
                .width(Length::Fill);
            Some(
                widget::dialog()
                    .title(capitalize(prompt.label))
                    .control(control)
                    .primary_action(widget::button::suggested("OK").on_press(core(Msg::Submit)))
                    .secondary_action(
                        widget::button::standard("Cancel").on_press(core(Msg::Escape)),
                    )
                    .into(),
            )
        }
        _ => None,
    }
}

pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

// ----- scroll geometry -----

/// The selected row as `(scrollable id, top, height)` in content pixels,
/// mirroring the layout above.
pub fn selection_rect(app: &App) -> Option<(&'static str, f32, f32)> {
    let ui = &app.ui;
    // A list_column draws each row with a gap and a divider, which row_pitch models.
    let listed = |id, i: usize, h: f32| Some((id, i as f32 * row_pitch(h), row_pitch(h)));
    // A virtual_list draws its rows exactly `h` apart, with no gap.
    let fixed = |id, i: usize, h: f32| Some((id, i as f32 * h, h));
    if ui.focus == Focus::Palette {
        let i = view_model::palette_rows(ui)
            .iter()
            .position(|r| r.selected)?;
        return listed(PALETTE_SCROLL, i, ROW1_H);
    }
    if let Some(picker) = view_model::move_picker(ui) {
        let i = picker.rows.iter().position(|r| r.selected)?;
        return listed(MOVE_SCROLL, i, ROW1_H);
    }
    match ui.section {
        Section::Notes => {
            let i = view_model::notes_list(ui).iter().position(|r| r.selected)?;
            fixed(NOTES_SCROLL, i, ROW2_H)
        }
        Section::Files => {
            let i = view_model::file_rows(ui).iter().position(|r| r.selected)?;
            fixed(FILES_SCROLL, i, ROW1_H)
        }
        Section::Contacts => {
            let i = view_model::contact_rows(ui)
                .iter()
                .position(|r| r.selected)?;
            fixed(CONTACTS_SCROLL, i, ROW2_H)
        }
        Section::Movies | Section::Music => {
            let i = view_model::media_rows(ui, ui.section)
                .iter()
                .position(|r| r.selected)?;
            fixed(
                if ui.section == Section::Music {
                    MUSIC_SCROLL
                } else {
                    MOVIES_SCROLL
                },
                i,
                ROW2_H,
            )
        }
        Section::Messages => {
            let i = ui.messages.sel;
            fixed(CONVERSATIONS_SCROLL, i, CONVERSATION_H)
        }
        Section::Todo => {
            let mut y = 0.0;
            for (piece, h) in todo_pieces(app) {
                if matches!(&piece, TodoPiece::Task { task, .. } if task.selected) {
                    return Some((TODO_SCROLL, y, ROW2_H));
                }
                y += h;
            }
            None
        }
        Section::Calendar => {
            let pitch = row_pitch(ROW1_H);
            if ui.calendar.mode == CalMode::Month {
                return Some((AGENDA_SCROLL, ui.calendar.sel as f32 * pitch, pitch));
            }
            let mut y = 0.0;
            for (piece, h) in agenda_pieces(app) {
                if matches!(&piece, AgendaPiece::Event { selected: true, .. }) {
                    return Some((AGENDA_SCROLL, y, ROW1_H));
                }
                y += h;
            }
            None
        }
        Section::Photos => {
            if ui.focus == Focus::Detail {
                return None;
            }
            let cols = ui.photos.columns.max(1);
            let pitch = photo_pitch(app.photo_width(), cols);
            Some((PHOTOS_SCROLL, (ui.photos.sel / cols) as f32 * pitch, pitch))
        }
    }
}
