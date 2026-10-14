//! Rendering. Every function here reads the view models from `locus-core` and
//! draws them; no decisions about app behaviour are made in this file, only
//! choices of which [`Msg`] a click or key means.

use iced::border::Radius;
use iced::widget::{
    Column, Row, button, column, container, image, mouse_area, opaque, row, scrollable, space,
    stack, text, text_editor, text_input,
};
use iced::{Border, Center, ContentFit, Element, Fill, Length, Shadow};
use locus_core::view_model::{self, EditorView, FormView};
use locus_core::{Focus, InputKind, Level, Msg, Section, TextEdit};

use crate::style::{self, Colors};
use crate::{
    FORM_IDS, GAP, HEADER_H, Message, PAD, PALETTE_ROW_H, ROW_H, SIDEBAR_W, Shell,
    TODO_ROW_H, photo_columns, tile_px,
};

/// Key hints for the status bar, for what has focus now.
fn hints(focus: Focus) -> &'static str {
    match focus {
        Focus::Editor => "Esc leave editor · Ctrl+S save · : palette (from the list)",
        Focus::Form => "Tab next field · Ctrl+Enter save · Esc cancel",
        Focus::Input(_) => "Enter confirm · Esc cancel",
        Focus::Palette => "Up/Down choose · Enter run · Esc close",
        Focus::Confirm => "y yes · n no",
        Focus::Help => "? or Esc closes",
        Focus::List | Focus::Detail => "j/k move · Enter open · n new · g n/t/c… go · ? help · : palette",
    }
}

/// Text edits from any text widget.
fn set_text(value: String) -> Message {
    Message::Core(Msg::Text(TextEdit::Set(value)))
}

fn core(msg: Msg) -> Message {
    Message::Core(msg)
}

fn scroll_msg(id: &'static str) -> impl Fn(scrollable::Viewport) -> Message + 'static {
    move |vp| Message::Scrolled(id, vp)
}


pub fn root(s: &Shell) -> Element<'_, Message> {
    if !s.logged_in {
        return login_screen(s);
    }
    let main = column![main_pane(s), status_bar(s)].width(Fill).height(Fill);
    // A section page (`/app/notes`) is that section's own app: no sidebar.
    let body = if view_model::show_sections(&s.app) {
        row![sidebar(s), main].height(Fill)
    } else {
        row![main].height(Fill)
    };
    let mut layers: Vec<Element<'_, Message>> = vec![body.into()];
    if let Some(overlay) = overlay(s) {
        layers.push(overlay);
    }
    stack(layers).width(Fill).height(Fill).into()
}

// ----- login -----

fn login_screen(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let status_color = level_color(c, view_model::status_level(&s.app));
    let card = container(
        column![
            text(view_model::window_title(&s.app)).size(20).color(c.text),
            text("Enter the web code for your ship.").size(14).color(c.muted),
            text_input("+code", &s.app.input.text)
                .id("login")
                .secure(true)
                .on_input(set_text)
                .padding(10)
                .size(14),
            text("Enter to sign in").size(12).color(c.muted),
            text(s.app.status.text.clone()).size(12).color(status_color),
        ]
        .spacing(12),
    )
    .padding(24)
    .width(360)
    .style(move |_| style::card(c));
    container(card)
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.bg)),
            ..container::Style::default()
        })
        .into()
}

// ----- chrome -----

fn sidebar(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let mut col = Column::new().spacing(4).padding(12);
    col = col.push(text("Locus").size(16).color(c.text));
    col = col.push(space::vertical().height(8));
    for (i, item) in view_model::sidebar(&s.app).into_iter().enumerate() {
        let section = Section::ALL[i];
        let badge = match item.badge {
            Some(n) if n > 0 => text(n.to_string()).size(12).color(c.muted),
            _ => text(""),
        };
        let content = row![
            text(item.label).size(14).color(c.text),
            badge,
            space::horizontal(),
            text(item.shortcut).size(12).color(c.muted),
        ]
        .spacing(8)
        .align_y(Center);
        col = col.push(
            button(content)
                .width(Fill)
                .padding([6, 8])
                .on_press(core(Msg::Switch(section)))
                .style(style::row_button(c, item.active)),
        );
    }
    container(col)
        .width(SIDEBAR_W)
        .height(Fill)
        .style(move |_| style::panel(c))
        .into()
}

fn status_bar(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let line = row![
        text(view_model::status_line(&s.app))
            .size(12)
            .color(level_color(c, view_model::status_level(&s.app))),
        space::horizontal(),
        text(hints(s.app.focus)).size(12).color(c.muted),
    ]
    .spacing(8)
    .align_y(Center);
    container(line)
        .width(Fill)
        .height(28)
        .padding([0, 12])
        .align_y(Center)
        .style(move |_| style::panel(c))
        .into()
}

fn level_color(c: Colors, level: Level) -> iced::Color {
    match level {
        Level::Info => c.muted,
        Level::Warn => c.warn,
        Level::Error => c.error,
    }
}

fn main_pane(s: &Shell) -> Element<'_, Message> {
    let mut col = Column::new().spacing(8);
    if s.app.focus == Focus::Input(InputKind::Filter) {
        col = col.push(
            text_input("filter", &s.app.filter)
                .id("filter")
                .on_input(set_text)
                .padding(8)
                .size(14),
        );
    }
    let body = match s.app.section {
        Section::Notes => notes_pane(s),
        Section::Todo => todo_pane(s),
        Section::Calendar => calendar_pane(s),
        Section::Photos => photos_pane(s),
        Section::Files => files_pane(s),
        Section::Contacts => contacts_pane(s),
    };
    col = col.push(body);
    container(col)
        .padding(PAD)
        .width(Fill)
        .height(Fill)
        .into()
}

// ----- shared pieces -----

/// A two-line list row of fixed height. Clicking a row that is not selected
/// selects it; clicking the selected row activates it (opens or edits).
fn list_row<'a>(
    c: Colors,
    index: usize,
    title: &str,
    subtitle: &str,
    selected: bool,
) -> Element<'a, Message> {
    let msg = if selected {
        core(Msg::Activate)
    } else {
        core(Msg::Select(index))
    };
    button(
        column![
            text(title.to_string()).size(14).color(c.text),
            text(subtitle.to_string()).size(12).color(c.muted),
        ]
        .spacing(2),
    )
    .width(Fill)
    .height(ROW_H)
    .padding([6, 8])
    .on_press(msg)
    .style(style::row_button(c, selected))
    .into()
}

fn empty_card<'a>(c: Colors, title: &str, hint: &str) -> Element<'a, Message> {
    container(
        column![
            text(title.to_string()).size(16).color(c.text),
            text(hint.to_string()).size(14).color(c.muted),
        ]
        .spacing(8),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .center_y(Fill)
    .style(move |_| style::card(c))
    .into()
}

/// What an empty list should say: loading, no filter matches, or `empty`.
fn empty_hint(s: &Shell, empty: &str) -> String {
    if s.app.is_loading(s.app.section) {
        "Loading...".into()
    } else if !s.app.filter.is_empty() {
        format!("Nothing matches \"{}\". Esc clears the filter.", s.app.filter)
    } else {
        empty.into()
    }
}

fn muted_text<'a>(c: Colors, s: impl Into<String>) -> iced::widget::Text<'a> {
    text(s.into()).size(12).color(c.muted)
}

/// Inline event or contact form, shown above or beside the list.
fn form_card<'a>(c: Colors, f: &FormView) -> Element<'a, Message> {
    let mut col = Column::new().spacing(8);
    col = col.push(text(f.title).size(16).color(c.text));
    let len = f.fields.len();
    let active = f.fields.iter().position(|x| x.active).unwrap_or(0);
    for (i, field) in f.fields.iter().enumerate() {
        let id = FORM_IDS.get(i).copied().unwrap_or("form-x");
        // Typing goes to the core's active field. A field clicked with the
        // mouse is not active yet, so step the core there before the text.
        let steps = (i + len - active) % len.max(1);
        let on_input = move |value: String| {
            let mut msgs = vec![Msg::NextField; steps];
            msgs.push(Msg::Text(TextEdit::Set(value)));
            Message::Core(Msg::Batch(msgs))
        };
        col = col.push(
            column![
                muted_text(c, field.label),
                text_input("", &field.value)
                    .id(id)
                    .on_input(on_input)
                    .padding(8)
                    .size(14),
            ]
            .spacing(4),
        );
    }
    if let Some(err) = &f.error {
        col = col.push(text(err.clone()).size(12).color(c.error));
    }
    col = col.push(muted_text(c, "Tab next field · Ctrl+Enter save · Esc cancel"));
    container(col)
        .padding(16)
        .width(Fill)
        .max_width(560)
        .style(move |_| style::card(c))
        .into()
}

// ----- notes -----

fn notes_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let rows = view_model::notes_list(&s.app);
    let list: Element<'_, Message> = if rows.is_empty() {
        muted_text(c, empty_hint(s, "No notes yet. Press n to create one.")).into()
    } else {
        let mut col = Column::new();
        for (i, r) in rows.iter().enumerate() {
            let title = if r.is_dir {
                format!("{}/", r.title)
            } else {
                r.title.clone()
            };
            col = col.push(list_row(c, i, &title, &r.subtitle, r.selected));
        }
        col.into()
    };
    let left = container(
        column![
            muted_text(c, view_model::notes_breadcrumb(&s.app)),
            scrollable(list)
                .id("notes")
                .spacing(4)
                .on_scroll(scroll_msg("notes"))
                .height(Fill),
        ]
        .spacing(8),
    )
    .width(300)
    .height(Fill);

    let right: Element<'_, Message> = match view_model::editor(&s.app) {
        Some(ed) => editor_pane(s, ed),
        None => empty_card(c, "No note open", "Pick a note and press Enter to open it."),
    };
    row![left, right].spacing(16).height(Fill).into()
}

fn editor_pane(s: &Shell, ed: EditorView) -> Element<'_, Message> {
    let c = s.colors;
    let (status, color) = if ed.conflict {
        ("Changed elsewhere. Your text is kept; server copy reloaded.", c.warn)
    } else if ed.saving {
        ("Saving...", c.muted)
    } else if ed.dirty {
        ("Unsaved changes", c.muted)
    } else {
        ("Saved", c.muted)
    };
    let header = row![
        text(ed.title.clone()).size(16).color(c.text),
        space::horizontal(),
        text(status).size(12).color(color),
    ]
    .spacing(8)
    .align_y(Center);
    let editor = text_editor(&s.editor)
        .id("editor")
        .on_action(Message::Editor)
        .padding(12)
        .size(14)
        .height(Fill);
    container(column![header, editor].spacing(8).padding(12))
        .width(Fill)
        .height(Fill)
        .style(move |_| style::card(c))
        .into()
}

// ----- todo -----

fn todo_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let mut col = Column::new();
    let mut index = 0usize;
    let groups = view_model::todo_groups(&s.app);
    if groups.iter().all(|g| g.count == 0) {
        return muted_text(
            c,
            empty_hint(
                s,
                "No tasks. Press n to add one, or write - [ ] lines in a note.",
            ),
        )
        .into();
    }
    for group in groups {
        let title = if group.collapsed {
            format!("{} (hidden, z shows it)", group.title)
        } else {
            group.title.to_string()
        };
        col = col.push(
            container(
                row![
                    text(title).size(12).color(c.muted),
                    space::horizontal(),
                    muted_text(c, group.count.to_string()),
                ]
                .align_y(Center),
            )
            .height(HEADER_H)
            .padding([0, 8])
            .align_y(Center)
            .width(Fill),
        );
        for task in &group.items {
            let mark = if task.done { "[x]" } else { "[ ]" };
            let text_color = if task.done { c.muted } else { c.text };
            let due_color = if task.overdue { c.error } else { c.muted };
            let content = row![
                text(mark).size(14).color(c.muted),
                column![
                    text(task.text.clone()).size(14).color(text_color),
                    muted_text(c, task.source_note_title.clone()),
                ]
                .width(Fill),
                text(task.due_label.clone()).size(12).color(due_color),
            ]
            .spacing(12)
            .align_y(Center);
            let msg = if task.selected {
                core(Msg::Activate)
            } else {
                core(Msg::Select(index))
            };
            col = col.push(
                button(content)
                    .width(Fill)
                    .height(TODO_ROW_H)
                    .padding([4, 8])
                    .on_press(msg)
                    .style(style::row_button(c, task.selected)),
            );
            index += 1;
        }
    }
    scrollable(col)
        .id("todo")
        .spacing(4)
        .on_scroll(scroll_msg("todo"))
        .width(Fill)
        .height(Fill)
        .into()
}

// ----- calendar -----

fn calendar_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let agenda_mode = s.app.calendar.mode == locus_core::CalMode::Agenda;
    let month = view_model::month_grid(&s.app);
    let mode_label = if agenda_mode { "Month view" } else { "Agenda view" };
    let header = row![
        button(text("<").size(14))
            .padding([4, 10])
            .on_press(core(Msg::ShiftMonth(-1)))
            .style(style::quiet_button(c)),
        text(month.title.clone()).size(16).color(c.text),
        button(text(">").size(14))
            .padding([4, 10])
            .on_press(core(Msg::ShiftMonth(1)))
            .style(style::quiet_button(c)),
        button(text("Today").size(14))
            .padding([4, 10])
            .on_press(core(Msg::Today))
            .style(style::quiet_button(c)),
        space::horizontal(),
        button(text(mode_label).size(14))
            .padding([4, 10])
            .on_press(core(Msg::CalendarMode))
            .style(style::quiet_button(c)),
    ]
    .spacing(8)
    .align_y(Center);

    let mut col = Column::new().spacing(8).height(Fill);
    col = col.push(header);
    if let Some(form) = view_model::form(&s.app).filter(|f| f.title.contains("event")) {
        col = col.push(form_card(c, &form));
    }
    if agenda_mode {
        col = col.push(text(view_model::selected_day_label(&s.app)).size(12).color(c.muted));
        col = col.push(agenda_list(s));
    } else {
        col = col.push(month_grid(s, &month));
        col = col.push(day_events(s));
    }
    col.into()
}

fn agenda_list(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let days = view_model::agenda(&s.app);
    let mut col = Column::new();
    if days.is_empty() {
        col = col.push(muted_text(c, "No events in the next 30 days. Press n to add one."));
    }
    for day in days {
        let label_color = if day.is_today { c.accent } else { c.muted };
        let mut label = day.label.clone();
        if day.is_today {
            label.push_str("  · today");
        }
        col = col.push(
            container(text(label).size(12).color(label_color))
                .height(HEADER_H)
                .align_y(Center)
                .padding([0, 8]),
        );
        for (i, ev) in day.events.iter().enumerate() {
            let msg = if ev.selected {
                core(Msg::Activate)
            } else if day.is_selected {
                core(Msg::Select(i))
            } else {
                Message::Core(Msg::Batch(vec![
                    Msg::SelectDay(day.date),
                    Msg::Select(i),
                ]))
            };
            let content = row![
                text(ev.time_label.clone()).size(12).color(c.muted).width(110),
                text(ev.title.clone()).size(14).color(c.text).width(Fill),
            ]
            .spacing(8)
            .align_y(Center);
            col = col.push(
                button(content)
                    .width(Fill)
                    .height(ROW_H)
                    .padding([4, 8])
                    .on_press(msg)
                    .style(style::row_button(c, ev.selected)),
            );
        }
    }
    scrollable(col)
        .id("agenda")
        .spacing(4)
        .on_scroll(scroll_msg("agenda"))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The selected day's events, under the month grid, so `e` and `d` have a
/// visible target.
fn day_events(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let mut col = Column::new().spacing(2);
    col = col.push(muted_text(c, view_model::selected_day_label(&s.app)));
    let today = view_model::agenda(&s.app)
        .into_iter()
        .next()
        .filter(|d| d.is_selected);
    match today {
        None => col = col.push(muted_text(c, "No events. Press n to add one.")),
        Some(day) => {
            for (i, ev) in day.events.iter().enumerate() {
                let msg = if ev.selected {
                    core(Msg::Activate)
                } else {
                    core(Msg::Select(i))
                };
                col = col.push(
                    button(
                        row![
                            text(ev.time_label.clone()).size(12).color(c.muted).width(110),
                            text(ev.title.clone()).size(14).color(c.text).width(Fill),
                        ]
                        .spacing(8)
                        .align_y(Center),
                    )
                    .width(Fill)
                    .padding([6, 8])
                    .on_press(msg)
                    .style(style::row_button(c, ev.selected)),
                );
            }
        }
    }
    scrollable(col).width(Fill).height(Fill).into()
}

fn month_grid<'a>(s: &Shell, month: &view_model::MonthView) -> Element<'a, Message> {
    let c = s.colors;
    let mut weekdays = Row::new().spacing(4);
    for name in ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"] {
        weekdays = weekdays.push(
            container(muted_text(c, name))
                .width(Fill)
                .center_x(Fill),
        );
    }
    let mut grid = Column::new().spacing(4);
    for week in month.weeks.iter() {
        let mut r = Row::new().spacing(4);
        for cell in week.iter() {
            let dots = "•".repeat(cell.event_count.min(3));
            let fg = if cell.is_selected {
                c.on_accent
            } else if cell.in_month {
                c.text
            } else {
                c.muted
            };
            let label = column![
                text(cell.day.to_string()).size(14).color(fg),
                text(dots).size(10).color(if cell.is_selected { c.on_accent } else { c.accent }),
            ]
            .align_x(Center);
            let selected = cell.is_selected;
            let today = cell.is_today;
            r = r.push(
                button(container(label).width(Fill).center_x(Fill))
                    .width(Fill)
                    .height(56)
                    .padding(4)
                    .on_press(core(Msg::SelectDay(cell.date)))
                    .style(move |_, status| button::Style {
                        background: if selected {
                            Some(iced::Background::Color(c.accent))
                        } else if matches!(status, button::Status::Hovered) {
                            Some(iced::Background::Color(style::with_alpha(c.text, 0.06)))
                        } else {
                            None
                        },
                        text_color: fg,
                        border: Border {
                            color: if today { c.accent } else { iced::Color::TRANSPARENT },
                            width: if today { 1.0 } else { 0.0 },
                            radius: Radius::from(6.0),
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    }),
            );
        }
        grid = grid.push(r);
    }
    column![weekdays, grid].spacing(8).width(Fill).into()
}

// ----- photos -----

fn photos_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    if s.app.focus == Focus::Detail {
        return viewer(s);
    }
    let cols = photo_columns(s.pane_w());
    let tile = tile_px(s.pane_w());
    let grid = view_model::photo_grid(&s.app, cols);
    if grid.rows.is_empty() {
        let hint = empty_hint(s, "Press u to upload photos.");
        return empty_card(c, "No photos", &hint);
    }
    let mut col = Column::new().spacing(GAP);
    for (ri, cells) in grid.rows.iter().enumerate() {
        let mut r = Row::new().spacing(GAP);
        for (ci, cell) in cells.iter().enumerate() {
            let index = ri * cols + ci;
            let inner: Element<'_, Message> = match cell.hash.as_deref().and_then(|h| s.thumb(h, 256)) {
                Some(handle) => image(handle.clone())
                    .width(Fill)
                    .height(Fill)
                    .content_fit(ContentFit::Cover)
                    .into(),
                None => container(muted_text(c, cell.title.clone()))
                    .width(Fill)
                    .height(Fill)
                    .center_x(Fill)
                    .center_y(Fill)
                    .into(),
            };
            let selected = cell.selected;
            let msg = if selected {
                core(Msg::Activate)
            } else {
                core(Msg::Select(index))
            };
            r = r.push(
                button(inner)
                    .width(tile)
                    .height(tile)
                    .padding(0)
                    .on_press(msg)
                    .style(move |_, status| button::Style {
                        background: Some(iced::Background::Color(c.panel)),
                        text_color: c.text,
                        border: Border {
                            color: if selected {
                                c.accent
                            } else if matches!(status, button::Status::Hovered) {
                                c.border
                            } else {
                                iced::Color::TRANSPARENT
                            },
                            width: 2.0,
                            radius: Radius::from(6.0),
                        },
                        shadow: Shadow::default(),
                        snap: false,
                    }),
            );
        }
        col = col.push(r);
    }
    scrollable(col)
        .id("photos")
        .on_scroll(scroll_msg("photos"))
        .width(Fill)
        .height(Fill)
        .into()
}

fn viewer(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let Some(detail) = view_model::photo_detail(&s.app) else {
        return empty_card(c, "No photo", "Nothing to show.");
    };
    // The grid's small thumbnail stands in until the large one arrives.
    let handle = detail
        .hash
        .as_deref()
        .and_then(|h| s.thumb(h, 1024).or_else(|| s.thumb(h, 256)));
    let picture: Element<'_, Message> = match handle {
        Some(handle) => image(handle.clone())
            .width(Fill)
            .height(Fill)
            .content_fit(ContentFit::Contain)
            .into(),
        None if detail.hash.is_none() => muted_text(c, "No preview for this photo").into(),
        None => muted_text(c, "Loading...").into(),
    };
    let caption = row![
        text(detail.title.clone()).size(16).color(c.text),
        space::horizontal(),
        muted_text(c, detail.position.clone()),
    ]
    .align_y(Center);
    let mut meta = detail.path.clone();
    if !detail.tags.is_empty() {
        meta.push_str("  ·  ");
        meta.push_str(&detail.tags.join(" · "));
    }
    column![
        caption,
        container(picture)
            .width(Fill)
            .height(Fill)
            .center_x(Fill)
            .center_y(Fill),
        muted_text(c, meta),
    ]
    .spacing(8)
    .height(Fill)
    .into()
}

// ----- files -----

fn files_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    // The root crumb is "/" itself, so the trail reads `/obsidian/Notes`.
    let crumbs: Vec<String> = view_model::breadcrumb(&s.app)
        .into_iter()
        .skip(1)
        .map(|crumb| crumb.label)
        .collect();
    let trail = format!("/{}", crumbs.join("/"));
    let header = row![
        muted_text(c, "Name").width(Fill),
        muted_text(c, "Size").width(96),
        muted_text(c, "Modified").width(120),
    ]
    .spacing(8)
    .padding([0, 8]);

    let rows = view_model::file_rows(&s.app);
    let mut col = Column::new();
    if rows.is_empty() {
        col = col.push(muted_text(
            c,
            empty_hint(s, "Empty folder. Press u to upload here, n for a new folder."),
        ));
    }
    for (i, r) in rows.iter().enumerate() {
        let name = if r.is_dir {
            format!("{}/", r.name)
        } else {
            r.name.clone()
        };
        let content = row![
            text(name).size(14).color(c.text).width(Fill),
            muted_text(c, r.size_label.clone()).width(96),
            muted_text(c, r.modified_label.clone()).width(120),
        ]
        .spacing(8)
        .align_y(Center);
        let msg = if r.selected {
            core(Msg::Activate)
        } else {
            core(Msg::Select(i))
        };
        col = col.push(
            button(content)
                .width(Fill)
                .height(ROW_H)
                .padding([4, 8])
                .on_press(msg)
                .style(style::row_button(c, r.selected)),
        );
    }
    column![
        muted_text(c, trail),
        header,
        scrollable(col)
            .id("files")
            .spacing(4)
            .on_scroll(scroll_msg("files"))
            .width(Fill)
            .height(Fill),
    ]
    .spacing(8)
    .height(Fill)
    .into()
}

// ----- contacts -----

fn contacts_pane(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let rows = view_model::contact_rows(&s.app);
    let list: Element<'_, Message> = if rows.is_empty() {
        muted_text(c, empty_hint(s, "No contacts yet. Press n to add one.")).into()
    } else {
        let mut col = Column::new();
        for (i, r) in rows.iter().enumerate() {
            col = col.push(list_row(c, i, &r.name, &r.subtitle, r.selected));
        }
        col.into()
    };
    let left = container(
        scrollable(list)
            .id("contacts")
            .spacing(4)
            .on_scroll(scroll_msg("contacts"))
            .height(Fill),
    )
    .width(300)
    .height(Fill);

    let right: Element<'_, Message> = if let Some(form) =
        view_model::form(&s.app).filter(|f| f.title.contains("contact"))
    {
        form_card(c, &form)
    } else if let Some(detail) = view_model::contact_detail(&s.app) {
        container(
            column![
                text(detail.name).size(16).color(c.text),
                muted_text(c, detail.email),
                muted_text(c, detail.ship),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(Fill)
        .style(move |_| style::card(c))
        .into()
    } else {
        empty_card(c, "No contact selected", "Pick a contact to see its details.")
    };
    row![left, right].spacing(16).height(Fill).into()
}

// ----- overlays -----

fn overlay(s: &Shell) -> Option<Element<'_, Message>> {
    let card = match s.app.focus {
        Focus::Palette => palette_card(s),
        Focus::Help => help_card(s),
        Focus::Confirm => confirm_card(s)?,
        Focus::Input(kind) if kind != InputKind::Filter && kind != InputKind::Login => {
            prompt_card(s)?
        }
        _ => return None,
    };
    let c = s.colors;
    // A click outside the card means Escape (close or cancel); the card is
    // opaque so clicks on it neither close it nor reach the app beneath.
    let scrim = mouse_area(
        container(space::horizontal())
            .width(Fill)
            .height(Fill)
            .style(move |_| style::scrim(c)),
    )
    .on_press(core(Msg::Escape));
    let centered = container(opaque(card))
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill);
    Some(stack![scrim, centered].width(Fill).height(Fill).into())
}

fn palette_card(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let rows = view_model::palette_rows(&s.app);
    let list: Element<'_, Message> = if rows.is_empty() {
        muted_text(c, "No matches").into()
    } else {
        let mut col = Column::new();
        for (i, r) in rows.iter().enumerate() {
            let content = row![
                text(r.label.clone()).size(14).color(c.text),
                muted_text(c, r.detail.clone()),
                space::horizontal(),
                muted_text(c, r.hint),
            ]
            .spacing(8)
            .align_y(Center);
            col = col.push(
                button(content)
                    .width(Fill)
                    .height(PALETTE_ROW_H)
                    .padding([4, 8])
                    .on_press(Message::Core(Msg::Batch(vec![
                        Msg::Select(i),
                        Msg::Submit,
                    ])))
                    .style(style::row_button(c, r.selected)),
            );
        }
        col.into()
    };
    let body = column![
        text_input("Type a command or a name", view_model::palette_query(&s.app))
            .id("palette")
            .on_input(set_text)
            .padding(10)
            .size(14),
        scrollable(list)
            .id("palette-list")
            .spacing(4)
            .on_scroll(scroll_msg("palette-list"))
            .height(Length::Fixed(360.0))
            .width(Fill),
    ]
    .spacing(8);
    container(body)
        .padding(12)
        .width(560)
        .style(move |_| style::card(c))
        .into()
}

fn help_card(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let rows = view_model::help(&s.app);
    let half = rows.len().div_ceil(2);
    let (left, right) = rows.split_at(half);
    let column_of = |items: &[(String, String)]| {
        let mut col = Column::new().spacing(6).width(Fill);
        for (keys, what) in items {
            col = col.push(
                row![
                    text(keys.clone()).size(12).color(c.accent).width(170),
                    text(what.clone()).size(14).color(c.text).width(Fill),
                ]
                .spacing(8)
                .align_y(Center),
            );
        }
        col
    };
    container(
        column![
            text(format!("Keys · {}", s.app.section.label())).size(16).color(c.text),
            row![column_of(left), column_of(right)].spacing(32),
            muted_text(c, "? or Esc closes"),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .max_width(720)
    .style(move |_| style::card(c))
    .into()
}

fn confirm_card(s: &Shell) -> Option<Element<'_, Message>> {
    let c = s.colors;
    let prompt = view_model::confirm_prompt(&s.app)?;
    Some(
        container(
            column![
                text(prompt).size(14).color(c.text),
                row![
                    button(text("Yes (y)").size(14))
                        .padding([6, 14])
                        .on_press(core(Msg::ConfirmAnswer(true)))
                        .style(style::primary_button(c)),
                    button(text("No (n)").size(14))
                        .padding([6, 14])
                        .on_press(core(Msg::ConfirmAnswer(false)))
                        .style(style::quiet_button(c)),
                ]
                .spacing(8),
            ]
            .spacing(16),
        )
        .padding(16)
        .width(420)
        .style(move |_| style::card(c))
        .into(),
    )
}

fn prompt_card(s: &Shell) -> Option<Element<'_, Message>> {
    let c = s.colors;
    let p = view_model::input_prompt(&s.app)?;
    Some(
        container(
            column![
                muted_text(c, p.label),
                text_input("", &p.text)
                    .id("prompt")
                    .on_input(set_text)
                    .padding(10)
                    .size(14),
                muted_text(c, "Enter to confirm · Esc to cancel"),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(480)
        .style(move |_| style::card(c))
        .into(),
    )
}

