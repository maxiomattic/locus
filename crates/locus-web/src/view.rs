//! Rendering. Every function here reads the view models from `locus-core` and
//! draws them; no decisions about app behaviour are made in this file, only
//! choices of which [`Msg`] a click or key means.

use iced::border::Radius;
use iced::widget::{
    Column, Row, button, checkbox, column, container, image, mouse_area, opaque, row, scrollable,
    space, stack, text, text_editor, text_input, tooltip,
};
use iced::{Border, Center, ContentFit, Element, Fill, Length, Shadow};
use locus_core::view_model::{self, EditorView, FormView};
use locus_core::{Focus, InputKind, Level, Msg, Section, TextEdit};

use crate::style::{self, Colors};
use crate::{
    FORM_IDS, GAP, HEADER_H, Message, PAD, PALETTE_ROW_H, ROW_H, Shell, TODO_ROW_H,
    photo_columns, tile_px,
};

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
    // `/app/` lists the apps; each app is its own page (`/app/notes`).
    if s.app.locked.is_none() {
        return launcher(s);
    }
    let body = column![toolbar(s), main_pane(s), status_bar(s)]
        .width(Fill)
        .height(Fill);
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
            row![
                space::horizontal(),
                button(text("Sign in").size(14))
                    .padding([6, 16])
                    .on_press(core(Msg::Submit))
                    .style(style::primary_button(c)),
            ],
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

/// `/app/`: one tile per app. Each opens that app's own page.
fn launcher(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let mut grid = Row::new().spacing(16);
    for section in Section::ALL {
        let tile = column![
            text(section.label()).size(18).color(c.text),
            muted_text(c, app_blurb(section)),
        ]
        .spacing(6);
        grid = grid.push(
            button(tile)
                .width(180)
                .height(110)
                .padding(16)
                .on_press(Message::Launch(section))
                .style(style::tile_button(c)),
        );
    }
    let page = column![
        text("Locus").size(24).color(c.text),
        muted_text(c, "Pick an app. Each one opens in its own tab."),
        grid.wrap(),
    ]
    .spacing(16)
    .max_width(1200);
    container(page)
        .padding(32)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(c.bg)),
            ..container::Style::default()
        })
        .into()
}

fn app_blurb(section: Section) -> &'static str {
    match section {
        Section::Notes => "Notebooks and notes",
        Section::Todo => "Tasks from your notes",
        Section::Calendar => "Events and agenda",
        Section::Photos => "Photo library",
        Section::Files => "Your files",
        Section::Contacts => "People and ships",
    }
}

/// A toolbar button with a label and a tooltip naming its shortcut.
/// `None` for `msg` draws it disabled.
fn tool<'a>(c: Colors, label: &'a str, tip: &'a str, msg: Option<Msg>) -> Element<'a, Message> {
    let b = button(text(label).size(13))
        .padding([5, 10])
        .on_press_maybe(msg.map(core))
        .style(style::quiet_button(c));
    if tip.is_empty() {
        return b.into();
    }
    tooltip(
        b,
        container(text(tip).size(12).color(c.text))
            .padding([4, 8])
            .style(move |_| style::card(c)),
        tooltip::Position::Bottom,
    )
    .into()
}

/// The app's toolbar: Back and where you are on the left, the search box,
/// the app's actions, then Reload and the shortcuts list.
fn toolbar(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let app = &s.app;
    let item = view_model::has_item_selected(app);
    let when = |ok: bool, msg: Msg| ok.then_some(msg);
    let mut bar = Row::new().spacing(6).align_y(Center);
    if matches!(app.section, Section::Notes | Section::Files) {
        bar = bar.push(tool(
            c,
            "Back",
            "Up one level (Backspace)",
            when(view_model::can_go_up(app), Msg::Nav(locus_core::Nav::Left)),
        ));
    }
    let place = match app.section {
        Section::Notes => view_model::notes_breadcrumb(app),
        Section::Files => {
            // The root crumb is "/" itself, so the trail reads `/obsidian/Notes`.
            let crumbs: Vec<String> = view_model::breadcrumb(app)
                .into_iter()
                .skip(1)
                .map(|crumb| crumb.label)
                .collect();
            format!("/{}", crumbs.join("/"))
        }
        other => other.label().to_string(),
    };
    bar = bar.push(text(place).size(15).color(c.text));
    bar = bar.push(space::horizontal());
    bar = bar.push(
        text_input("Search", &app.filter)
            .id("filter")
            .on_input(|value| {
                Message::Core(Msg::Batch(vec![Msg::FocusFilter, Msg::Text(TextEdit::Set(value))]))
            })
            .padding([5, 10])
            .size(13)
            .width(220),
    );
    let actions: Vec<Element<'_, Message>> = match app.section {
        Section::Notes if !view_model::can_go_up(app) => vec![
            tool(c, "New notebook", "", Some(Msg::NewNotebook)),
            tool(c, "New note", "Ctrl+N or Alt+N", Some(Msg::New)),
        ],
        Section::Notes => vec![
            tool(c, "New note", "Ctrl+N or Alt+N", Some(Msg::New)),
            tool(c, "New folder", "", Some(Msg::NewFolder)),
            tool(c, "Rename", "F2", when(item, Msg::Rename)),
            tool(c, "Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Todo => {
            let done = if app.todo.show_done { "Hide done" } else { "Show done" };
            vec![
                tool(c, "Add task", "Ctrl+N or Alt+N", Some(Msg::New)),
                tool(c, "Open note", "Enter", when(item, Msg::Activate)),
                tool(c, done, "", Some(Msg::ToggleDoneTasks)),
            ]
        }
        Section::Calendar => vec![
            tool(c, "New event", "Ctrl+N or Alt+N", Some(Msg::New)),
            tool(c, "Edit", "F2", when(item, Msg::Edit)),
            tool(c, "Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Photos => vec![
            tool(c, "Upload", "Ctrl+O", Some(Msg::Upload)),
            tool(c, "Open", "Enter", when(item && app.focus != Focus::Detail, Msg::Activate)),
            tool(c, "Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Files => vec![
            tool(c, "Upload", "Ctrl+O", Some(Msg::Upload)),
            tool(c, "New folder", "Ctrl+N or Alt+N", Some(Msg::New)),
            tool(c, "Rename", "F2", when(item, Msg::Rename)),
            tool(c, "Move", "", when(item, Msg::MoveFile)),
            tool(c, "Copy path", "Ctrl+C", when(item, Msg::Yank)),
            tool(c, "Delete", "Delete", when(item, Msg::Delete)),
        ],
        Section::Contacts => vec![
            tool(c, "New contact", "Ctrl+N or Alt+N", Some(Msg::New)),
            tool(c, "Edit", "F2", when(item, Msg::Edit)),
            tool(c, "Delete", "Delete", when(item, Msg::Delete)),
        ],
    };
    for action in actions {
        bar = bar.push(action);
    }
    bar = bar.push(tool(c, "Reload", "Ctrl+R or F5", Some(Msg::Reload)));
    bar = bar.push(tool(c, "Shortcuts", "F1", Some(Msg::ToggleHelp)));
    container(bar)
        .width(Fill)
        .padding([8, 12])
        .style(move |_| style::panel(c))
        .into()
}

fn status_bar(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let line = row![
        text(view_model::status_line(&s.app))
            .size(12)
            .color(level_color(c, view_model::status_level(&s.app))),
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
    let body = match s.app.section {
        Section::Notes => notes_pane(s),
        Section::Todo => todo_pane(s),
        Section::Calendar => calendar_pane(s),
        Section::Photos => photos_pane(s),
        Section::Files => files_pane(s),
        Section::Contacts => contacts_pane(s),
    };
    container(body)
        .padding(PAD)
        .width(Fill)
        .height(Fill)
        .into()
}

// ----- shared pieces -----

/// A list row: a click sends `select`, a double click also `open`.
fn click_row<'a>(
    c: Colors,
    content: impl Into<Element<'a, Message>>,
    height: f32,
    selected: bool,
    select: Msg,
    open: Msg,
) -> Element<'a, Message> {
    mouse_area(
        container(content)
            .width(Fill)
            .height(height)
            .padding([4, 8])
            .center_y(height)
            .style(move |_| style::row_fill(c, selected)),
    )
    .on_press(core(select.clone()))
    .on_double_click(core(Msg::Batch(vec![select, open])))
    .interaction(iced::mouse::Interaction::Pointer)
    .into()
}

/// A two-line list row of fixed height that sends `msg` when clicked. A
/// row that leads somewhere (a notebook or folder) ends in a chevron.
fn list_row<'a>(
    c: Colors,
    msg: Message,
    title: &str,
    subtitle: &str,
    selected: bool,
    leads_in: bool,
) -> Element<'a, Message> {
    let chevron = if leads_in { "›" } else { "" };
    button(
        row![
            column![
                text(title.to_string()).size(14).color(c.text),
                text(subtitle.to_string()).size(12).color(c.muted),
            ]
            .spacing(2)
            .width(Fill),
            text(chevron).size(18).color(c.muted),
        ]
        .align_y(Center),
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
        format!("Nothing matches \"{}\".", s.app.filter)
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
    col = col.push(
        row![
            space::horizontal(),
            button(text("Cancel").size(14))
                .padding([6, 14])
                .on_press(core(Msg::Escape))
                .style(style::quiet_button(c)),
            button(text("Save").size(14))
                .padding([6, 14])
                .on_press(core(Msg::SaveForm))
                .style(style::primary_button(c)),
        ]
        .spacing(8),
    );
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
        muted_text(c, empty_hint(s, "No notes here yet. Click New note to write one.")).into()
    } else {
        let mut col = Column::new();
        for (i, r) in rows.iter().enumerate() {
            // One click opens a notebook, folder or note.
            let open = core(Msg::Batch(vec![Msg::Select(i), Msg::Activate]));
            col = col.push(list_row(c, open, &r.title, &r.subtitle, r.selected, r.is_dir));
        }
        col.into()
    };
    let left = container(
        scrollable(list)
            .id("notes")
            .spacing(4)
            .on_scroll(scroll_msg("notes"))
            .height(Fill),
    )
    .width(300)
    .height(Fill);

    let right: Element<'_, Message> = match view_model::editor(&s.app) {
        Some(ed) => editor_pane(s, ed),
        None => empty_card(c, "No note open", "Click a note to open it."),
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
                "No tasks. Click Add task, or write - [ ] lines in a note.",
            ),
        )
        .into();
    }
    for group in groups.into_iter().filter(|g| g.count > 0) {
        let title = if group.collapsed {
            format!("{} (hidden; Show done shows it)", group.title)
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
            let i = index;
            let mark = checkbox(task.done)
                .on_toggle(move |_| core(Msg::Batch(vec![Msg::Select(i), Msg::ToggleTask])))
                .size(16);
            let text_color = if task.done { c.muted } else { c.text };
            let due_color = if task.overdue { c.error } else { c.muted };
            let content = row![
                mark,
                column![
                    text(task.text.clone()).size(14).color(text_color),
                    muted_text(c, task.source_note_title.clone()),
                ]
                .width(Fill),
                text(task.due_label.clone()).size(12).color(due_color),
            ]
            .spacing(12)
            .align_y(Center);
            col = col.push(click_row(c, content, TODO_ROW_H, task.selected, Msg::Select(index), Msg::Activate));
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
        button(text("‹ Previous").size(14))
            .padding([4, 10])
            .on_press(core(Msg::ShiftMonth(-1)))
            .style(style::quiet_button(c)),
        text(month.title.clone()).size(16).color(c.text),
        button(text("Next ›").size(14))
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
        col = col.push(muted_text(c, "No events in the next 30 days. Click New event to add one."));
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
            let select = Msg::Batch(vec![Msg::SelectDay(day.date), Msg::Select(i)]);
            let content = row![
                text(ev.time_label.clone()).size(12).color(c.muted).width(110),
                text(ev.title.clone()).size(14).color(c.text).width(Fill),
            ]
            .spacing(8)
            .align_y(Center);
            col = col.push(click_row(c, content, ROW_H, ev.selected, select, Msg::Edit));
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

/// The selected day's events, under the month grid, so Edit and Delete have
/// a visible target.
fn day_events(s: &Shell) -> Element<'_, Message> {
    let c = s.colors;
    let mut col = Column::new().spacing(2);
    col = col.push(muted_text(c, view_model::selected_day_label(&s.app)));
    let today = view_model::agenda(&s.app)
        .into_iter()
        .next()
        .filter(|d| d.is_selected);
    match today {
        None => col = col.push(muted_text(c, "No events. Click New event to add one.")),
        Some(day) => {
            for (i, ev) in day.events.iter().enumerate() {
                let content = row![
                    text(ev.time_label.clone()).size(12).color(c.muted).width(110),
                    text(ev.title.clone()).size(14).color(c.text).width(Fill),
                ]
                .spacing(8)
                .align_y(Center);
                col = col.push(click_row(c, content, 36.0, ev.selected, Msg::Select(i), Msg::Edit));
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
        let hint = empty_hint(s, "Click Upload to add photos.");
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
            let framed = container(inner)
                .width(tile)
                .height(tile)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(c.panel)),
                    border: Border {
                        color: if selected { c.accent } else { iced::Color::TRANSPARENT },
                        width: 2.0,
                        radius: Radius::from(6.0),
                    },
                    ..container::Style::default()
                });
            r = r.push(
                mouse_area(framed)
                    .on_press(core(Msg::Select(index)))
                    .on_double_click(core(Msg::Batch(vec![Msg::Select(index), Msg::Activate])))
                    .interaction(iced::mouse::Interaction::Pointer),
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
        tool(c, "‹ All photos", "Esc", Some(Msg::Escape)),
        text(detail.title.clone()).size(16).color(c.text),
        space::horizontal(),
        muted_text(c, detail.position.clone()),
        tool(c, "Previous", "Left", Some(Msg::Nav(locus_core::Nav::Left))),
        tool(c, "Next", "Right", Some(Msg::Nav(locus_core::Nav::Right))),
    ]
    .spacing(8)
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
            empty_hint(s, "Empty folder. Click Upload to add files here."),
        ));
    }
    for (i, r) in rows.iter().enumerate() {
        let (name_color, size) = if r.is_dir {
            (c.accent, "Folder".to_string())
        } else {
            (c.text, r.size_label.clone())
        };
        let content = row![
            text(r.name.clone()).size(14).color(name_color).width(Fill),
            muted_text(c, size).width(96),
            muted_text(c, r.modified_label.clone()).width(120),
        ]
        .spacing(8)
        .align_y(Center);
        col = col.push(click_row(c, content, ROW_H, r.selected, Msg::Select(i), Msg::Activate));
    }
    column![
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
        muted_text(c, empty_hint(s, "No contacts yet. Click New contact to add one.")).into()
    } else {
        let mut col = Column::new();
        for (i, r) in rows.iter().enumerate() {
            let content = column![
                text(r.name.clone()).size(14).color(c.text),
                text(r.subtitle.clone()).size(12).color(c.muted),
            ]
            .spacing(2);
            col = col.push(click_row(c, content, ROW_H, r.selected, Msg::Select(i), Msg::Edit));
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
            text(format!("Keyboard shortcuts · {}", s.app.section.label())).size(16).color(c.text),
            row![column_of(left), column_of(right)].spacing(32),
            row![
                space::horizontal(),
                button(text("Close").size(14))
                    .padding([6, 14])
                    .on_press(core(Msg::ToggleHelp))
                    .style(style::quiet_button(c)),
            ],
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
                    button(text("Delete").size(14))
                        .padding([6, 14])
                        .on_press(core(Msg::ConfirmAnswer(true)))
                        .style(style::primary_button(c)),
                    button(text("Cancel").size(14))
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
                row![
                    space::horizontal(),
                    button(text("Cancel").size(14))
                        .padding([6, 14])
                        .on_press(core(Msg::Escape))
                        .style(style::quiet_button(c)),
                    button(text("OK").size(14))
                        .padding([6, 14])
                        .on_press(core(Msg::Submit))
                        .style(style::primary_button(c)),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(16)
        .width(480)
        .style(move |_| style::card(c))
        .into(),
    )
}

