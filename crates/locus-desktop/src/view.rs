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
use locus_core::view_model::{self, EditorView, FormView};
use locus_core::{Msg, Nav, TextEdit};

use crate::{App, Message, Screen, THUMB_GRID, THUMB_VIEW, thumb_key};

// ----- widget ids -----

pub const EDITOR_ID: &str = "editor";
pub const FILTER_ID: &str = "filter";
pub const INPUT_ID: &str = "input";
pub const PALETTE_ID: &str = "palette";
pub const PHOTOS_SCROLL: &str = "scroll-photos";
const NOTES_SCROLL: &str = "scroll-notes";
const TODO_SCROLL: &str = "scroll-todo";
const AGENDA_SCROLL: &str = "scroll-agenda";
const FILES_SCROLL: &str = "scroll-files";
const CONTACTS_SCROLL: &str = "scroll-contacts";
const PALETTE_SCROLL: &str = "scroll-palette";

pub fn form_id(i: usize) -> String {
    format!("form-{i}")
}

// ----- sizes -----

/// Widest the libcosmic nav bar gets; used until the grid reports its width.
pub const NAV_W: f32 = 280.0;
/// Horizontal room the page padding and scrollbar take from the grid.
pub const PAGE_PAD_X: f32 = 56.0;
/// Width of the list pane in Notes and Contacts.
const LIST_W: f32 = 300.0;
/// Content height of a two-line row and a one-line row. A list row is this
/// plus the list's vertical item padding plus a 1px divider.
const ROW2_H: f32 = 40.0;
const ROW1_H: f32 = 28.0;
/// Height of a group or day header in Todo and the agenda.
const HEADER_H: f32 = 28.0;
/// Height of the "hidden" caption under a collapsed group.
const NOTE_H: f32 = 20.0;
/// Target photo tile edge; the grid fits as many columns as the width allows.
const TILE: f32 = 168.0;
const GAP: f32 = 8.0;

fn scroll_msg(id: &'static str) -> impl Fn(cosmic::iced::widget::scrollable::Viewport) -> Message {
    move |vp| Message::Scrolled(id, vp)
}

fn core(msg: Msg) -> Message {
    Message::Core(msg)
}

fn set_text(value: String) -> Message {
    Message::Core(Msg::Text(TextEdit::Set(value)))
}

/// Distance from one list row's top to the next.
fn row_pitch(content_h: f32) -> f32 {
    content_h + 2.0 * f32::from(theme::spacing().space_xxs) + 1.0
}

pub fn photo_columns(width: f32) -> usize {
    ((width + GAP) / (TILE + GAP)).floor().max(1.0) as usize
}

/// Edge of a square tile so `cols` tiles fill `width`.
fn tile_edge(width: f32, cols: usize) -> f32 {
    let cols = cols.max(1) as f32;
    ((width - (cols - 1.0) * GAP) / cols).clamp(64.0, 2.0 * TILE)
}

/// Distance from one grid row's top to the next.
pub fn photo_pitch(width: f32, cols: usize) -> f32 {
    tile_edge(width, cols) + GAP
}

pub fn section_icon(section: Section) -> &'static str {
    match section {
        Section::Notes => "accessories-text-editor-symbolic",
        Section::Todo => "object-select-symbolic",
        Section::Calendar => "x-office-calendar-symbolic",
        Section::Photos => "image-x-generic-symbolic",
        Section::Files => "folder-symbolic",
        Section::Contacts => "system-users-symbolic",
    }
}

// ----- colors -----

fn danger() -> Color {
    theme::active().cosmic().destructive_text_color().into()
}

fn warning() -> Color {
    theme::active().cosmic().warning_text_color().into()
}

fn accent() -> Color {
    theme::active().cosmic().accent_text_color().into()
}

fn colored<'a>(text: impl Into<String>, color: Color) -> Element<'a, Message> {
    widget::text::body(text.into())
        .class(theme::Text::Color(color))
        .into()
}

fn caption_colored<'a>(text: impl Into<String>, color: Color) -> Element<'a, Message> {
    widget::text::caption(text.into())
        .class(theme::Text::Color(color))
        .into()
}

fn level_text<'a>(text: String, level: Level) -> Element<'a, Message> {
    match level {
        Level::Error => caption_colored(text, danger()),
        Level::Warn => caption_colored(text, warning()),
        Level::Info => widget::text::caption(text).into(),
    }
}

// ----- shared pieces -----

fn toolbar<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut row = widget::row::with_capacity(items.len())
        .spacing(theme::spacing().space_xxs)
        .align_y(Alignment::Center);
    for item in items {
        row = row.push(item);
    }
    row.into()
}

fn text_button(label: &'static str, msg: Msg) -> Element<'static, Message> {
    widget::button::text(label).on_press(core(msg)).into()
}

fn icon_button(icon: &'static str, tip: &'static str, msg: Msg) -> Element<'static, Message> {
    widget::tooltip(
        widget::button::icon(widget::icon::from_name(icon)).on_press(core(msg)),
        widget::text::caption(tip),
        widget::tooltip::Position::Bottom,
    )
    .into()
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(content)
        .padding(theme::spacing().space_m)
        .class(theme::Container::Card)
        .width(Length::Fill)
}

/// Centered title and hint for an empty pane.
fn empty<'a>(title: &str, hint: String) -> Element<'a, Message> {
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
fn empty_hint(app: &App, empty: &str) -> String {
    if app.ui.is_loading(app.ui.section) {
        "Loading...".into()
    } else if !app.ui.filter.is_empty() {
        format!("Nothing matches \"{}\". Esc clears the filter.", app.ui.filter)
    } else {
        empty.into()
    }
}

/// A list row: clicking an unselected row selects it, clicking the selected
/// row activates it (open, enter, edit).
fn list_item<'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
    selected: bool,
    on_select: Msg,
) -> widget::list::ListButton<'a, Message> {
    let msg = if selected { Msg::Activate } else { on_select };
    widget::list::button(
        widget::container(content)
            .height(Length::Fixed(height))
            .width(Length::Fill)
            .align_y(Alignment::Center),
    )
    .selected(selected)
    .on_press(core(msg))
}

fn two_lines<'a>(title: String, subtitle: String) -> Element<'a, Message> {
    widget::column::with_capacity(2)
        .push(widget::text::body(title))
        .push(widget::text::caption(subtitle))
        .into()
}

fn scroll<'a>(id: &'static str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    widget::scrollable(content)
        .id(Id::new(id))
        .on_scroll(scroll_msg(id))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

// ----- chrome -----

pub fn header_end() -> Vec<Element<'static, Message>> {
    vec![
        icon_button("list-add-symbolic", "New (n)", Msg::New),
        icon_button("view-refresh-symbolic", "Reload (Ctrl+R)", Msg::Reload),
        icon_button("system-search-symbolic", "Command palette (: or Ctrl+K)", Msg::OpenPalette),
        icon_button("help-browser-symbolic", "Keyboard shortcuts (?)", Msg::ToggleHelp),
    ]
}

pub fn footer(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let ui = &app.ui;
    widget::container(
        widget::row::with_capacity(2)
            .spacing(s.space_m)
            .align_y(Alignment::Center)
            .push(
                widget::container(level_text(
                    view_model::status_line(ui),
                    view_model::status_level(ui),
                ))
                .width(Length::Fill),
            )
            .push(widget::text::caption(hints(ui.focus))),
    )
    .padding([s.space_xxs, s.space_s])
    .width(Length::Fill)
    .into()
}

fn hints(focus: Focus) -> &'static str {
    match focus {
        Focus::Editor => "Esc leave editor · Ctrl+S save · Ctrl+K palette",
        Focus::Input(_) | Focus::Palette => "Enter confirm · Esc cancel",
        Focus::Form => "Tab next field · Ctrl+Enter save · Esc cancel",
        Focus::Confirm => "y yes · n no",
        Focus::Help => "Esc close",
        Focus::Detail => "h/l previous/next · Esc back",
        Focus::List => "j/k move · Enter open · n new · / filter · : palette · ? help",
    }
}

pub fn content(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    match app.screen {
        Screen::Connecting => widget::container(
            card(widget::text::body(format!("Connecting to {}...", app.server)))
                .max_width(420.0),
        )
        .center(Length::Fill)
        .into(),
        Screen::Login => login(app),
        Screen::Main => {
            let mut col = widget::column::with_capacity(2)
                .spacing(s.space_xs)
                .height(Length::Fill);
            if app.ui.focus == Focus::Input(InputKind::Filter) || !app.ui.filter.is_empty() {
                col = col.push(
                    widget::search_input("Filter", app.ui.filter.as_str())
                        .id(Id::new(FILTER_ID))
                        .on_input(set_text)
                        .on_clear(core(Msg::Batch(vec![
                            Msg::FocusFilter,
                            Msg::Text(TextEdit::Set(String::new())),
                        ])))
                        .width(Length::Fill),
                );
            }
            col = col.push(match app.ui.section {
                Section::Notes => notes(app),
                Section::Todo => todo(app),
                Section::Calendar => calendar(app),
                Section::Photos => photos(app),
                Section::Files => files(app),
                Section::Contacts => contacts(app),
            });
            widget::container(col)
                .padding([s.space_xxs, s.space_s, s.space_xs, s.space_s])
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }
}

fn login(app: &App) -> Element<'_, Message> {
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
        .push(
            widget::settings::item_row(vec![
                widget::text::body("Server").width(Length::Fixed(96.0)).into(),
                widget::text_input("http://surface:8090", app.server.as_str())
                    .on_input(Message::ServerEdit)
                    .on_submit(|_| core(Msg::Submit))
                    .width(Length::Fill)
                    .into(),
            ]),
        )
        .push(
            widget::settings::item_row(vec![
                widget::text::body("Web code").width(Length::Fixed(96.0)).into(),
                widget::secure_input("sampel-ticlyt-migfun-falmel", code, None, true)
                    .id(Id::new(INPUT_ID))
                    .on_input(set_text)
                    .width(Length::Fill)
                    .into(),
            ]),
        )
        .push(
            widget::row::with_capacity(2)
                .align_y(Alignment::Center)
                .push(
                    widget::container(level_text(
                        app.ui.status.text.clone(),
                        app.ui.status.level,
                    ))
                    .width(Length::Fill),
                )
                .push(widget::button::suggested("Connect").on_press(core(Msg::Submit))),
        );
    widget::container(card(form).padding(s.space_l).max_width(520.0))
        .center(Length::Fill)
        .into()
}

// ----- notes -----

fn notes(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let rows = view_model::notes_list(&app.ui);
    let list: Element<'_, Message> = if rows.is_empty() {
        empty("No notes", empty_hint(app, "Press n to create one."))
    } else {
        let mut list = widget::list_column();
        for (i, row) in rows.into_iter().enumerate() {
            let title = if row.is_dir {
                format!("{}/", row.title)
            } else {
                row.title
            };
            list = list.add(list_item(
                two_lines(title, row.subtitle),
                ROW2_H,
                row.selected,
                Msg::Select(i),
            ));
        }
        scroll(NOTES_SCROLL, list)
    };
    let left = widget::column::with_capacity(2)
        .spacing(s.space_xxs)
        .width(Length::Fixed(LIST_W))
        .height(Length::Fill)
        .push(widget::text::caption(view_model::notes_breadcrumb(&app.ui)))
        .push(list);
    let right: Element<'_, Message> = match view_model::editor(&app.ui) {
        Some(editor) => editor_pane(app, editor),
        None => empty("No note open", "Select a note and press Enter.".into()),
    };
    widget::row::with_capacity(2)
        .spacing(s.space_s)
        .push(left)
        .push(widget::container(right).width(Length::Fill).height(Length::Fill))
        .into()
}

fn editor_pane(app: &App, editor: EditorView) -> Element<'_, Message> {
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
    let header = widget::row::with_capacity(4)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(widget::text::title4(editor.title).width(Length::Fill))
        .push(status)
        .push(icon_button("edit-symbolic", "Rename (r)", Msg::Rename))
        .push(icon_button("user-trash-symbolic", "Delete (d)", Msg::Delete));
    let text = widget::text_editor::text_editor(&app.content)
        .id(Id::new(EDITOR_ID))
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

// ----- todo -----

/// Todo groups shown: empty groups are left out.
fn todo_groups(app: &App) -> Vec<view_model::Group> {
    view_model::todo_groups(&app.ui)
        .into_iter()
        .filter(|g| g.count > 0)
        .collect()
}

fn todo(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let groups = todo_groups(app);
    if groups.is_empty() {
        return empty(
            "No tasks",
            empty_hint(app, "Press n to add one, or write - [ ] lines in a note."),
        );
    }
    let mut col = widget::column::with_capacity(groups.len() * 2).spacing(s.space_xs);
    let mut index = 0usize;
    for group in groups {
        let title: Element<'_, Message> = if group.title.eq_ignore_ascii_case("overdue") {
            caption_colored(group.title, danger())
        } else {
            widget::text::heading(group.title).into()
        };
        col = col.push(
            widget::container(
                widget::row::with_capacity(2)
                    .spacing(s.space_xs)
                    .align_y(Alignment::Center)
                    .push(title)
                    .push(widget::text::caption(group.count.to_string())),
            )
            .height(Length::Fixed(HEADER_H))
            .align_y(Alignment::Center),
        );
        if group.collapsed {
            col = col.push(
                widget::container(widget::text::caption("Hidden. Press z to show."))
                    .height(Length::Fixed(NOTE_H)),
            );
            continue;
        }
        let mut list = widget::list_column();
        for task in group.items {
            let i = index;
            index += 1;
            let check = widget::checkbox(task.done)
                .on_toggle(move |_| core(Msg::Batch(vec![Msg::Select(i), Msg::ToggleTask])));
            let text: Element<'_, Message> = if task.done {
                widget::text::body(task.text)
                    .class(theme::Text::Color(dim()))
                    .into()
            } else {
                widget::text::body(task.text).into()
            };
            let due: Element<'_, Message> = if task.overdue {
                caption_colored(task.due_label, danger())
            } else {
                widget::text::caption(task.due_label).into()
            };
            let content = widget::row::with_capacity(3)
                .spacing(s.space_s)
                .align_y(Alignment::Center)
                .push(check)
                .push(
                    widget::column::with_capacity(2)
                        .width(Length::Fill)
                        .push(text)
                        .push(widget::text::caption(task.source_note_title)),
                )
                .push(due);
            list = list.add(list_item(content, ROW2_H, task.selected, Msg::Select(i)));
        }
        col = col.push(list);
    }
    scroll(TODO_SCROLL, col)
}

fn dim() -> Color {
    let mut c: Color = theme::active().cosmic().on_bg_color().into();
    c.a = 0.5;
    c
}

// ----- calendar -----

fn calendar(app: &App) -> Element<'_, Message> {
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
        .push(icon_button("go-previous-symbolic", "Previous month ([)", Msg::ShiftMonth(-1)))
        .push(
            widget::container(widget::text::title4(month.title.clone()))
                .width(Length::Fixed(180.0))
                .align_x(Alignment::Center),
        )
        .push(icon_button("go-next-symbolic", "Next month (])", Msg::ShiftMonth(1)))
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
            col = col
                .push(widget::text::caption(view_model::selected_day_label(&app.ui)))
                .push(agenda(app));
        }
        CalMode::Month => {
            col = col.push(month_grid(month)).push(day_events(app));
        }
    }
    col.into()
}

fn event_row<'a>(time: String, title: String) -> Element<'a, Message> {
    widget::row::with_capacity(2)
        .spacing(s_s())
        .align_y(Alignment::Center)
        .push(widget::text::caption(time).width(Length::Fixed(100.0)))
        .push(widget::text::body(title).width(Length::Fill))
        .into()
}

fn s_s() -> u16 {
    theme::spacing().space_s
}

fn agenda(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let days = view_model::agenda(&app.ui);
    if days.is_empty() {
        return empty(
            "No events",
            empty_hint(app, "Nothing in the next 30 days. Press n to add one."),
        );
    }
    let mut col = widget::column::with_capacity(days.len() * 2).spacing(s.space_xs);
    for day in days {
        let mut label = day.label.clone();
        if day.is_today {
            label.push_str(" · Today");
        }
        let heading: Element<'_, Message> = if day.is_selected {
            widget::text::heading(label)
                .class(theme::Text::Color(accent()))
                .into()
        } else {
            widget::text::heading(label).into()
        };
        col = col.push(
            widget::container(heading)
                .height(Length::Fixed(HEADER_H))
                .align_y(Alignment::Center),
        );
        let mut list = widget::list_column();
        for (j, ev) in day.events.into_iter().enumerate() {
            let select = if day.is_selected {
                Msg::Select(j)
            } else {
                Msg::Batch(vec![Msg::SelectDay(day.date), Msg::Select(j)])
            };
            list = list.add(list_item(
                event_row(ev.time_label, ev.title),
                ROW1_H,
                ev.selected,
                select,
            ));
        }
        col = col.push(list);
    }
    scroll(AGENDA_SCROLL, col)
}

/// The selected day's events under the month grid, so `e` and `d` have a
/// visible target.
fn day_events(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let mut col = widget::column::with_capacity(2)
        .spacing(s.space_xxs)
        .push(widget::text::heading(view_model::selected_day_label(&app.ui)));
    let day = view_model::agenda(&app.ui)
        .into_iter()
        .next()
        .filter(|d| d.is_selected);
    match day {
        None => col = col.push(widget::text::caption("No events. Press n to add one.")),
        Some(day) => {
            let mut list = widget::list_column();
            for (j, ev) in day.events.into_iter().enumerate() {
                list = list.add(list_item(
                    event_row(ev.time_label, ev.title),
                    ROW1_H,
                    ev.selected,
                    Msg::Select(j),
                ));
            }
            col = col.push(scroll(AGENDA_SCROLL, list));
        }
    }
    col.height(Length::Fill).into()
}

fn month_grid<'a>(month: view_model::MonthView) -> Element<'a, Message> {
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
fn form_card<'a>(form: FormView) -> Element<'a, Message> {
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
            .push(widget::text::caption("Tab next field · Ctrl+Enter save · Esc cancel").width(Length::Fill))
            .push(widget::button::standard("Cancel").on_press(core(Msg::Escape)))
            .push(widget::button::suggested("Save").on_press(core(Msg::SaveForm))),
    );
    card(grid).max_width(640.0).into()
}

// ----- photos -----

fn photos(app: &App) -> Element<'_, Message> {
    if app.ui.focus == Focus::Detail {
        return viewer(app);
    }
    let cols = app.ui.photos.columns.max(1);
    let grid = view_model::photo_grid(&app.ui, cols);
    if grid.rows.is_empty() {
        return empty("No photos", empty_hint(app, "Press u to upload photos."));
    }
    let tile = tile_edge(app.photo_width(), cols);
    let radius = theme::active().cosmic().radius_s();
    let mut col = widget::column::with_capacity(grid.rows.len()).spacing(GAP);
    for (r, cells) in grid.rows.into_iter().enumerate() {
        let mut row = widget::row::with_capacity(cols).spacing(GAP);
        for (c, cell) in cells.into_iter().enumerate() {
            let index = r * cols + c;
            let thumb = cell
                .hash
                .as_deref()
                .and_then(|sha| app.thumbs.get(&thumb_key(sha, THUMB_GRID)));
            let inner: Element<'_, Message> = match thumb {
                Some(Some(handle)) => widget::image(handle.clone())
                    .content_fit(ContentFit::Cover)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
                _ => widget::container(widget::text::caption(cell.title))
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
            let msg = if selected {
                Msg::Activate
            } else {
                Msg::Select(index)
            };
            row = row.push(widget::mouse_area(framed).on_press(core(msg)));
        }
        col = col.push(row);
    }
    scroll(PHOTOS_SCROLL, col)
}

fn viewer(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let Some(detail) = view_model::photo_detail(&app.ui) else {
        return empty("No photo", "Press Escape to go back.".into());
    };
    // The grid thumbnail stands in until the large one arrives.
    let handle = detail.hash.as_deref().and_then(|sha| {
        app.thumbs
            .get(&thumb_key(sha, THUMB_VIEW))
            .cloned()
            .flatten()
            .or_else(|| app.thumbs.get(&thumb_key(sha, THUMB_GRID)).cloned().flatten())
    });
    let picture: Element<'_, Message> = match handle {
        Some(handle) => widget::image(handle)
            .content_fit(ContentFit::Contain)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None if detail.hash.is_none() => empty("No preview", "This photo has no stored image.".into()),
        None => empty("Loading", "Fetching the photo.".into()),
    };
    let top = widget::row::with_capacity(5)
        .spacing(s.space_xs)
        .align_y(Alignment::Center)
        .push(icon_button("go-previous-symbolic", "Back to grid (Esc)", Msg::Escape))
        .push(widget::text::title4(detail.title).width(Length::Fill))
        .push(widget::text::caption(detail.position))
        .push(icon_button("go-up-symbolic", "Previous (h)", Msg::Nav(Nav::Left)))
        .push(icon_button("go-down-symbolic", "Next (l)", Msg::Nav(Nav::Right)));
    let mut meta = detail.path;
    if !detail.tags.is_empty() {
        meta.push_str("  ·  ");
        meta.push_str(&detail.tags.join(" · "));
    }
    widget::column::with_capacity(3)
        .spacing(s.space_xs)
        .height(Length::Fill)
        .push(top)
        .push(widget::container(picture).center(Length::Fill))
        .push(widget::text::caption(meta))
        .into()
}

// ----- files -----

fn files(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let crumbs = view_model::breadcrumb(&app.ui);
    let depth = crumbs.len().saturating_sub(1);
    let mut path = widget::row::with_capacity(crumbs.len() * 2)
        .spacing(s.space_xxxs)
        .align_y(Alignment::Center);
    for (j, crumb) in crumbs.into_iter().enumerate() {
        if j > 1 {
            path = path.push(widget::text::caption("/"));
        }
        // Each crumb is that many levels up from the current folder.
        let up = (j < depth).then(|| core(Msg::Batch(vec![Msg::Nav(Nav::Left); depth - j])));
        path = path.push(widget::button::text(crumb.label).on_press_maybe(up));
    }
    let tools = toolbar(vec![
        path.width(Length::Fill).into(),
        icon_button("document-send-symbolic", "Upload here (u)", Msg::Upload),
        icon_button("folder-new-symbolic", "New folder (n)", Msg::New),
        icon_button("edit-symbolic", "Rename (r)", Msg::Rename),
        icon_button("go-jump-symbolic", "Move (m)", Msg::MoveFile),
        icon_button("edit-copy-symbolic", "Copy path (y)", Msg::Yank),
        icon_button("user-trash-symbolic", "Delete (d)", Msg::Delete),
    ]);
    let rows = view_model::file_rows(&app.ui);
    let body: Element<'_, Message> = if rows.is_empty() {
        empty(
            "Empty folder",
            empty_hint(app, "Press u to upload here, n for a new folder."),
        )
    } else {
        let mut list = widget::list_column();
        for (i, row) in rows.into_iter().enumerate() {
            let icon = if row.is_dir {
                "folder-symbolic"
            } else {
                "text-x-generic-symbolic"
            };
            let content = widget::row::with_capacity(4)
                .spacing(s.space_s)
                .align_y(Alignment::Center)
                .push(widget::icon::from_name(icon).size(16).icon())
                .push(widget::text::body(row.name).width(Length::Fill))
                .push(widget::text::caption(row.size_label).width(Length::Fixed(90.0)))
                .push(widget::text::caption(row.modified_label).width(Length::Fixed(100.0)));
            list = list.add(list_item(content, ROW1_H, row.selected, Msg::Select(i)));
        }
        let header = widget::row::with_capacity(3)
            .spacing(s.space_s)
            .padding([0, s.space_m])
            .push(widget::text::caption("Name").width(Length::Fill))
            .push(widget::text::caption("Size").width(Length::Fixed(90.0)))
            .push(widget::text::caption("Modified").width(Length::Fixed(100.0)));
        widget::column::with_capacity(2)
            .spacing(s.space_xxs)
            .push(header)
            .push(scroll(FILES_SCROLL, list))
            .into()
    };
    widget::column::with_capacity(2)
        .spacing(s.space_xs)
        .height(Length::Fill)
        .push(tools)
        .push(body)
        .into()
}

// ----- contacts -----

fn contacts(app: &App) -> Element<'_, Message> {
    let s = theme::spacing();
    let rows = view_model::contact_rows(&app.ui);
    let list: Element<'_, Message> = if rows.is_empty() {
        empty("No contacts", empty_hint(app, "Press n to add one."))
    } else {
        let mut list = widget::list_column();
        for (i, row) in rows.into_iter().enumerate() {
            list = list.add(list_item(
                two_lines(row.name, row.subtitle),
                ROW2_H,
                row.selected,
                Msg::Select(i),
            ));
        }
        scroll(CONTACTS_SCROLL, list)
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
                        .push(icon_button("edit-symbolic", "Edit (e)", Msg::Edit))
                        .push(icon_button("user-trash-symbolic", "Delete (d)", Msg::Delete)),
                )
                .push(
                    widget::list_column()
                        .add(field("Email", detail.email))
                        .add(field("Ship", detail.ship)),
                )
                .into()
        } else {
            empty("No contact selected", "Pick a contact to see its details.".into())
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
            let prompt = view_model::confirm_prompt(ui)?;
            Some(
                widget::dialog()
                    .title("Confirm")
                    .body(prompt)
                    .primary_action(
                        widget::button::destructive("Yes").on_press(core(Msg::ConfirmAnswer(true))),
                    )
                    .secondary_action(
                        widget::button::standard("No").on_press(core(Msg::ConfirmAnswer(false))),
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
        Focus::Input(kind) if kind != InputKind::Filter && kind != InputKind::Login => {
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

fn capitalize(s: &str) -> String {
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
    let sp = f32::from(theme::spacing().space_xs);
    let rect = |id, i: usize, h: f32| Some((id, i as f32 * row_pitch(h), row_pitch(h)));
    if ui.focus == Focus::Palette {
        let i = view_model::palette_rows(ui).iter().position(|r| r.selected)?;
        return rect(PALETTE_SCROLL, i, ROW1_H);
    }
    match ui.section {
        Section::Notes => {
            let i = view_model::notes_list(ui).iter().position(|r| r.selected)?;
            rect(NOTES_SCROLL, i, ROW2_H)
        }
        Section::Files => {
            let i = view_model::file_rows(ui).iter().position(|r| r.selected)?;
            rect(FILES_SCROLL, i, ROW1_H)
        }
        Section::Contacts => {
            let i = view_model::contact_rows(ui).iter().position(|r| r.selected)?;
            rect(CONTACTS_SCROLL, i, ROW2_H)
        }
        Section::Todo => {
            let pitch = row_pitch(ROW2_H);
            let mut y = 0.0;
            for group in todo_groups(app) {
                y += HEADER_H + sp;
                if group.collapsed {
                    y += NOTE_H + sp;
                    continue;
                }
                if let Some(k) = group.items.iter().position(|t| t.selected) {
                    return Some((TODO_SCROLL, y + k as f32 * pitch, pitch));
                }
                y += group.items.len() as f32 * pitch - 1.0 + sp;
            }
            None
        }
        Section::Calendar => {
            let pitch = row_pitch(ROW1_H);
            if ui.calendar.mode == CalMode::Month {
                return Some((AGENDA_SCROLL, ui.calendar.sel as f32 * pitch, pitch));
            }
            let mut y = 0.0;
            for day in view_model::agenda(ui) {
                y += HEADER_H + sp;
                if let Some(k) = day.events.iter().position(|e| e.selected) {
                    return Some((AGENDA_SCROLL, y + k as f32 * pitch, pitch));
                }
                y += day.events.len() as f32 * pitch - 1.0 + sp;
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
