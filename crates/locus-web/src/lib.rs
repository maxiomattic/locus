//! Browser shell for the Locus apps.
//!
//! This crate contains no app logic. It translates browser input into
//! [`locus_core::Msg`], runs the [`locus_core::Effect`]s the core returns, and
//! draws the view models. Everything else lives in `locus-core`.
//!
//! The wasm32 build is the only one: the whole crate is gated on the target,
//! so `cargo` on the host builds an empty library.

#![cfg(target_arch = "wasm32")]

mod platform;
mod style;
mod view;

use std::collections::{HashMap, HashSet};

use iced::widget::{image, operation, scrollable, text_editor};
use iced::{Event, Size, Subscription, Task, Theme, event, keyboard, window};
use locus_client::Client;
use locus_core::{
    App, Effect, Focus, InputKind, Key, Mods, Msg, Section, keymap, view_model,
};

use style::Colors;

/// Left sidebar width.
pub const SIDEBAR_W: f32 = 200.0;
/// Inner padding of the main pane.
pub const PAD: f32 = 16.0;
/// Gap between photo tiles and between rows.
pub const GAP: f32 = 8.0;
/// Target photo tile width; the grid fits as many as the pane allows.
pub const TILE_TARGET: f32 = 160.0;
/// Room for the scrollbar beside a scrollable pane.
pub const SCROLLBAR_W: f32 = 12.0;
/// Fixed heights for scrollable rows. Scroll offsets are computed from these.
pub const ROW_H: f32 = 52.0;
pub const TODO_ROW_H: f32 = 44.0;
pub const HEADER_H: f32 = 32.0;
pub const PALETTE_ROW_H: f32 = 36.0;

/// Ids of the form fields, by index. Forms have at most this many fields.
pub const FORM_IDS: [&str; 8] = [
    "form-0", "form-1", "form-2", "form-3", "form-4", "form-5", "form-6", "form-7",
];

/// Everything the shell holds between messages.
pub struct Shell {
    app: App,
    client: Client,
    colors: Colors,
    theme: Theme,
    logged_in: bool,
    window: Size,
    /// Decoded images, keyed `sha@size`.
    thumbs: HashMap<String, image::Handle>,
    /// Keys already requested, so each thumbnail is fetched once.
    requested: HashSet<String>,
    editor: text_editor::Content,
    /// The note the editor widget shows, and the body it was last synced to.
    editor_note: Option<u64>,
    editor_synced: String,
    /// Last known scroll state per scrollable id: `(offset_y, height)`.
    viewports: HashMap<&'static str, (f32, f32)>,
    /// The iced widget currently holding keyboard focus, as we last set it.
    focused: Option<&'static str>,
    /// Last selected row as `(scrollable id, top in px)`, to scroll on change.
    last_rect: Option<(&'static str, u32)>,
}

/// Everything that reaches [`Shell::update`].
#[derive(Debug, Clone)]
pub enum Message {
    /// A message from a widget or a click.
    Core(Msg),
    /// Results of an effect; each one goes back through the core.
    Msgs(Vec<Msg>),
    /// A key press from the window.
    Key(Key, Mods),
    /// Result of the session check at boot.
    Session(bool),
    Resized(Size),
    Editor(text_editor::Action),
    Scrolled(&'static str, scrollable::Viewport),
    Thumb(String, Result<Vec<u8>, String>),
    /// The URL hash changed: another page sent a cross-link.
    Hash(String),
}

/// Photo grid columns for a main pane of `width` pixels.
pub fn photo_columns(width: f32) -> usize {
    let inner = photo_inner(width);
    ((inner + GAP) / (TILE_TARGET + GAP)).floor().max(1.0) as usize
}

/// Width available to the photo grid.
fn photo_inner(width: f32) -> f32 {
    (width - 2.0 * PAD - SCROLLBAR_W).max(1.0)
}

/// Square tile edge for a main pane of `width` pixels, filling the row.
pub fn tile_px(width: f32) -> f32 {
    let cols = photo_columns(width) as f32;
    ((photo_inner(width) - (cols - 1.0) * GAP) / cols).max(48.0)
}

impl Shell {
    fn boot() -> (Shell, Task<Message>) {
        let colors = if platform::prefers_light() {
            Colors::light()
        } else {
            Colors::dark()
        };
        let client = platform::client();
        let (locked, target) = platform::route();
        let mut shell = Shell {
            app: App::new(),
            client: client.clone(),
            colors,
            theme: colors.theme(),
            logged_in: false,
            window: Size::new(1280.0, 800.0),
            thumbs: HashMap::new(),
            requested: HashSet::new(),
            editor: text_editor::Content::new(),
            editor_note: None,
            editor_synced: String::new(),
            viewports: HashMap::new(),
            focused: None,
            last_rect: None,
        };
        shell.app.focus_section(locked);
        if let Some(target) = target {
            // `/app/#note:3` opens Notes; a section page keeps its section.
            if locked.is_none()
                && let Some(section) = target.section()
            {
                shell.app.section = section;
            }
            shell.app.pending_target = Some(target);
            platform::clear_hash();
        }
        platform::claim_tab(locked, &view_model::window_title(&shell.app));
        // Show the login card until the session check says otherwise.
        let mut tasks = vec![shell.apply(Msg::BeginLogin)];
        tasks.push(Task::future(async move {
            Message::Session(locus_effects::session_ok(&client).await)
        }));
        tasks.push(
            window::latest()
                .then(|id| match id {
                    Some(id) => window::size(id),
                    None => Task::none(),
                })
                .map(Message::Resized),
        );
        (shell, Task::batch(tasks))
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn subscription(_: &Shell) -> Subscription<Message> {
        Subscription::batch([
            event::listen_raw(on_event),
            window::resize_events().map(|(_, size)| Message::Resized(size)),
            Subscription::run(platform::hash_changes).map(Message::Hash),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Core(msg) => self.apply(msg),
            Message::Msgs(msgs) => {
                let mut tasks = Vec::with_capacity(msgs.len());
                for msg in msgs {
                    tasks.push(self.apply(msg));
                }
                Task::batch(tasks)
            }
            Message::Key(key, mods) => match self.key_msg(key, mods) {
                Some(msg) => self.apply(msg),
                None => Task::none(),
            },
            Message::Session(ok) => {
                if ok {
                    self.logged_in = true;
                    // The page's section (or Notes); a pending `#target`
                    // survives this switch and opens once it has loaded.
                    let section = self.app.section;
                    self.apply(Msg::Switch(section))
                } else {
                    self.apply(Msg::BeginLogin)
                }
            }
            Message::Resized(size) => {
                self.window = size;
                self.apply(Msg::Layout {
                    photo_columns: photo_columns(self.pane_w()),
                })
            }
            Message::Hash(hash) => {
                let Some(target) = locus_core::Target::parse(hash.trim_start_matches('#')) else {
                    return Task::none();
                };
                platform::clear_hash();
                if !self.logged_in {
                    self.app.pending_target = Some(target);
                    return Task::none();
                }
                let section = target
                    .section()
                    .or(self.app.locked)
                    .unwrap_or(self.app.section);
                self.apply(Msg::Open {
                    section,
                    target: Some(target),
                })
            }
            Message::Editor(action) => self.editor_action(action),
            Message::Scrolled(id, viewport) => {
                self.viewports.insert(
                    id,
                    (viewport.absolute_offset().y, viewport.bounds().height),
                );
                self.sync()
            }
            Message::Thumb(key, result) => {
                if let Ok(bytes) = result {
                    self.thumbs.insert(key, image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
        }
    }

    /// Run one core message, then the effects it returns, then sync the shell.
    /// Effects return their results as `Message::Msgs`, so this recurses only
    /// through the message loop, never inside one call.
    fn apply(&mut self, msg: Msg) -> Task<Message> {
        if matches!(msg, Msg::LoggedIn) {
            self.logged_in = true;
        }
        if let Msg::LoadFailed { error, .. } | Msg::WriteFailed { error, .. } = &msg
            && (error.contains("HTTP 401") || error.contains("HTTP 403"))
        {
            // The session expired: show the login card again.
            self.logged_in = false;
        }
        let effects = locus_core::update(&mut self.app, msg, platform::clock());
        let mut tasks: Vec<Task<Message>> =
            effects.into_iter().map(|e| self.effect(e)).collect();
        if !self.logged_in && self.app.focus != Focus::Input(InputKind::Login) {
            tasks.push(self.apply(Msg::BeginLogin));
        }
        tasks.push(self.sync());
        Task::batch(tasks)
    }

    fn effect(&self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::CopyToClipboard(text) => return iced::clipboard::write::<Message>(text),
            Effect::OpenSection(section, target) => {
                platform::open_section(section, target);
                return Task::none();
            }
            _ => {}
        }
        let client = self.client.clone();
        Task::future(async move {
            Message::Msgs(locus_effects::run(&client, &platform::WebPlatform, effect).await)
        })
    }

    /// Keys the core should see. While a text widget has focus, plain
    /// characters and Backspace are typing, so they never reach the keymap.
    fn key_msg(&self, key: Key, mods: Mods) -> Option<Msg> {
        let typing = matches!(
            self.app.focus,
            Focus::Input(_) | Focus::Form | Focus::Palette
        );
        if typing
            && !mods.ctrl
            && !mods.alt
            && matches!(key, Key::Char(_) | Key::Backspace | Key::Delete)
        {
            return None;
        }
        keymap(&self.app, key, mods)
    }

    fn editor_action(&mut self, action: text_editor::Action) -> Task<Message> {
        let edit = action.is_edit();
        let mut tasks = Vec::new();
        if self.app.focus != Focus::Editor {
            // A click into the editor takes focus from the list.
            tasks.push(self.apply(Msg::EditorFocus));
        }
        self.editor.perform(action);
        if edit {
            let text = self.editor.text();
            self.editor_synced = text.clone();
            tasks.push(self.apply(Msg::EditorChanged(text)));
        }
        tasks.push(self.sync());
        Task::batch(tasks)
    }

    /// Bring the widget state in line with the core after every change:
    /// editor content, iced focus, list scroll, and thumbnail requests.
    fn sync(&mut self) -> Task<Message> {
        match &self.app.editor {
            Some(ed) => {
                if self.editor_note != Some(ed.note_id) || self.editor_synced != ed.body {
                    self.editor = text_editor::Content::with_text(&ed.body);
                    self.editor_synced = ed.body.clone();
                    self.editor_note = Some(ed.note_id);
                }
            }
            None => self.editor_note = None,
        }

        let mut tasks = Vec::new();
        let want = self.focus_id();
        if want != self.focused {
            tasks.push(match want {
                Some(id) => operation::focus::<Message>(id),
                None => iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                ),
            });
            self.focused = want;
        }

        if let Some((id, top, height)) = self.selection_rect() {
            let top_px = top.round() as u32;
            if self.last_rect != Some((id, top_px)) {
                self.last_rect = Some((id, top_px));
                let (offset, view_h) = self
                    .viewports
                    .get(id)
                    .copied()
                    .unwrap_or((0.0, self.window.height));
                let target = if top < offset {
                    Some(top)
                } else if top + height > offset + view_h {
                    Some(top + height - view_h)
                } else {
                    None
                };
                if let Some(y) = target {
                    tasks.push(operation::scroll_to::<Message>(
                        id,
                        scrollable::AbsoluteOffset {
                            x: None,
                            y: Some(y.max(0.0)),
                        },
                    ));
                }
            }
        }

        if self.logged_in {
            tasks.extend(self.request_thumbs());
        }
        Task::batch(tasks)
    }

    /// Which widget should hold keyboard focus for the core's focus.
    fn focus_id(&self) -> Option<&'static str> {
        match self.app.focus {
            Focus::Input(InputKind::Filter) => Some("filter"),
            Focus::Input(InputKind::Login) => Some("login"),
            Focus::Input(_) => Some("prompt"),
            Focus::Form => {
                let active = self.app.form.as_ref()?.active;
                FORM_IDS.get(active).copied()
            }
            Focus::Palette => Some("palette"),
            Focus::Editor => Some("editor"),
            _ => None,
        }
    }

    /// The selected row: `(scrollable id, top, height)` in content pixels.
    fn selection_rect(&self) -> Option<(&'static str, f32, f32)> {
        let app = &self.app;
        if app.focus == Focus::Palette {
            let i = view_model::palette_rows(app).iter().position(|r| r.selected)?;
            return Some(("palette-list", i as f32 * PALETTE_ROW_H, PALETTE_ROW_H));
        }
        match app.section {
            Section::Notes => {
                let i = view_model::notes_list(app).iter().position(|r| r.selected)?;
                Some(("notes", i as f32 * ROW_H, ROW_H))
            }
            Section::Todo => {
                let mut y = 0.0;
                for group in view_model::todo_groups(app) {
                    y += HEADER_H;
                    for task in &group.items {
                        if task.selected {
                            return Some(("todo", y, TODO_ROW_H));
                        }
                        y += TODO_ROW_H;
                    }
                }
                None
            }
            Section::Calendar => {
                if app.calendar.mode != locus_core::CalMode::Agenda {
                    return None;
                }
                let mut y = 0.0;
                for day in view_model::agenda(app) {
                    y += HEADER_H;
                    for event in &day.events {
                        if event.selected {
                            return Some(("agenda", y, ROW_H));
                        }
                        y += ROW_H;
                    }
                }
                None
            }
            Section::Photos => {
                if app.focus == Focus::Detail {
                    return None;
                }
                let cols = photo_columns(self.pane_w());
                let grid = view_model::photo_grid(app, cols);
                let row_h = tile_px(self.pane_w()) + GAP;
                Some(("photos", (grid.selected / cols) as f32 * row_h, row_h))
            }
            Section::Files => {
                let i = view_model::file_rows(app).iter().position(|r| r.selected)?;
                Some(("files", i as f32 * ROW_H, ROW_H))
            }
            Section::Contacts => {
                let i = view_model::contact_rows(app).iter().position(|r| r.selected)?;
                Some(("contacts", i as f32 * ROW_H, ROW_H))
            }
        }
    }

    /// Fetch the thumbnails the visible photo cells need, once each.
    fn request_thumbs(&mut self) -> Vec<Task<Message>> {
        let app = &self.app;
        let mut wanted: Vec<(String, String, u32)> = Vec::new();
        if app.section == Section::Photos {
            if app.focus == Focus::Detail {
                if let Some(hash) = view_model::photo_detail(app).and_then(|d| d.hash) {
                    wanted.push((format!("{hash}@1024"), hash, 1024));
                }
            } else {
                let cols = photo_columns(self.pane_w());
                let grid = view_model::photo_grid(app, cols);
                let (offset, view_h) = self
                    .viewports
                    .get("photos")
                    .copied()
                    .unwrap_or((0.0, self.window.height));
                let row_h = tile_px(self.pane_w()) + GAP;
                let first = (offset / row_h).floor().max(0.0) as usize;
                let last = ((offset + view_h) / row_h).ceil() as usize + 1;
                for row in grid.rows.iter().skip(first).take(last.saturating_sub(first)) {
                    for cell in row {
                        if let Some(hash) = &cell.hash {
                            wanted.push((format!("{hash}@256"), hash.clone(), 256));
                        }
                    }
                }
            }
        }

        let mut tasks = Vec::new();
        for (key, sha, size) in wanted {
            if self.thumbs.contains_key(&key) || !self.requested.insert(key.clone()) {
                continue;
            }
            let client = self.client.clone();
            tasks.push(Task::future(async move {
                let result = locus_effects::thumb(&client, &sha, size).await;
                Message::Thumb(key, result)
            }));
        }
        tasks
    }

    /// Width of the main pane: the window, less the sidebar when it shows.
    pub fn pane_w(&self) -> f32 {
        let sidebar = if view_model::show_sections(&self.app) {
            SIDEBAR_W
        } else {
            0.0
        };
        self.window.width - sidebar
    }

    /// Thumbnail for a hash at a size, if it has arrived.
    pub fn thumb(&self, hash: &str, size: u32) -> Option<&image::Handle> {
        self.thumbs.get(&format!("{hash}@{size}"))
    }
}

/// Window events that matter: key presses. Captured events are included on
/// purpose, because a focused text widget captures the keys the core also needs.
fn on_event(event: Event, _status: event::Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    let mods = platform::key_modifiers().unwrap_or(Mods {
        ctrl: modifiers.control(),
        alt: modifiers.alt(),
        shift: modifiers.shift(),
        logo: modifiers.logo(),
    });
    let key = if mods.alt || mods.ctrl {
        // With a modifier the logical key can be a layout symbol (Option+1 is
        // `¡` on a Mac, Ctrl+K is `л` on a Russian layout); bindings mean the
        // key position, so fall back to it.
        latin_key(&key, physical_key).or_else(|| map_key(&key))?
    } else {
        map_key(&key)?
    };
    Some(Message::Key(key, mods))
}

/// Digits and letters by key position, for modifier combos.
fn latin_key(key: &keyboard::Key, physical: keyboard::key::Physical) -> Option<Key> {
    use keyboard::key::{Code, Physical};
    let digit = match physical {
        Physical::Code(Code::Digit1) => Some('1'),
        Physical::Code(Code::Digit2) => Some('2'),
        Physical::Code(Code::Digit3) => Some('3'),
        Physical::Code(Code::Digit4) => Some('4'),
        Physical::Code(Code::Digit5) => Some('5'),
        Physical::Code(Code::Digit6) => Some('6'),
        _ => None,
    };
    digit
        .or_else(|| key.to_latin(physical).filter(char::is_ascii_alphanumeric))
        .map(|c| Key::Char(c.to_ascii_lowercase()))
}

fn map_key(key: &keyboard::Key) -> Option<Key> {
    use keyboard::key::Named;
    Some(match key {
        keyboard::Key::Character(s) => Key::Char(s.chars().next()?),
        keyboard::Key::Named(named) => match named {
            Named::Enter => Key::Enter,
            Named::Escape => Key::Escape,
            Named::Tab => Key::Tab,
            Named::Backspace => Key::Backspace,
            Named::Delete => Key::Delete,
            Named::ArrowUp => Key::Up,
            Named::ArrowDown => Key::Down,
            Named::ArrowLeft => Key::Left,
            Named::ArrowRight => Key::Right,
            Named::Home => Key::Home,
            Named::End => Key::End,
            Named::PageUp => Key::PageUp,
            Named::PageDown => Key::PageDown,
            Named::Space => Key::Char(' '),
            _ => return None,
        },
        _ => return None,
    })
}

/// Start the app. Called by wasm-bindgen when the module loads.
pub fn run() -> iced::Result {
    iced::application(Shell::boot, Shell::update, view::root)
        .title(|s: &Shell| view_model::window_title(&s.app))
        .theme(Shell::theme)
        .subscription(Shell::subscription)
        .run()
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    platform::track_key_modifiers();
    if let Err(e) = run() {
        web_sys::console::error_1(&e.to_string().into());
    }
}
