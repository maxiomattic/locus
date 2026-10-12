//! The Locus desktop shell (libcosmic).
//!
//! The shell holds no app logic. Every intent becomes a `locus_core::Msg` for
//! `locus_core::update`; the returned `Effect`s run through
//! `locus_effects::run` and their results come back as messages. This file
//! owns the libcosmic plumbing (nav bar, header, footer, subscriptions) and the
//! glue that keeps widget state (editor text, keyboard focus, scroll position,
//! thumbnails) in line with the core. Drawing is in `view.rs`.

mod config;
mod keys;
mod platform;
mod view;

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::widget::scrollable::Viewport;
use cosmic::iced::{Event, Size, Subscription, clipboard, event, keyboard, window};
use cosmic::prelude::*;
use cosmic::widget::{nav_bar, text_editor};
use cosmic::{Element, executor, widget};
use locus_client::Client;
use locus_core::state::{App as Ui, Focus, InputKind};
use locus_core::{Effect, Key, Mods, Msg, Section, Target, TextEdit, view_model};
use url::Url;

/// Local UTC offset, read in `main` before any thread exists (the `time`
/// crate refuses to read it once the process is multi-threaded).
static TZ_OFFSET: OnceLock<i32> = OnceLock::new();

/// Server shown when no settings exist yet: the gateway on the home network.
const DEFAULT_SERVER: &str = "http://surface:8090";
/// Refresh interval for the core's clock (day rollover, relative dates).
const TICK: Duration = Duration::from_secs(30);
/// Thumbnail edge for the grid and for the viewer.
const THUMB_GRID: u32 = 256;
const THUMB_VIEW: u32 = 1024;

const USAGE: &str = "usage: locus [notes|todo|calendar|photos|files|contacts] [--open <target>]
  locus              all sections in one window
  locus <section>    that section as its own app (own window, own app id)
  --open <target>    show note:<id>, file:<id>, contact:<id>, new or upload";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let flags = match Flags::parse(std::env::args().skip(1)) {
        Ok(flags) => flags,
        Err(msg) => {
            eprintln!("locus: {msg}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let tz = time::UtcOffset::current_local_offset()
        .map(|o| o.whole_seconds())
        .unwrap_or(0);
    let _ = TZ_OFFSET.set(tz);
    let settings = Settings::default()
        .antialiasing(true)
        .client_decorations(true)
        .resizable(Some(8.0))
        .size(Size::new(1200.0, 780.0))
        .size_limits(cosmic::iced::Limits::NONE.min_width(640.0).min_height(420.0));
    // One `cosmic::Application` type per app id: libcosmic takes the id from
    // a const. Section windows are single-instance per id, so a second
    // `locus notes` focuses the running one and hands it the target.
    macro_rules! launch {
        ($($n:literal),*) => {
            match flags.section.map_or(0, |s| s.index() + 1) {
                0 => cosmic::app::run::<Window<0>>(settings, flags)?,
                $($n => cosmic::app::run_single_instance::<Window<$n>>(settings, flags)?,)*
                _ => unreachable!("Section::index is below 6"),
            }
        };
    }
    launch!(1, 2, 3, 4, 5, 6);
    Ok(())
}

/// The app id COSMIC files a window under: one for the all-sections window
/// and one per section, so each section is its own app (taskbar entry,
/// tiling, `.desktop` file). Index 0 is all sections, 1..=6 follow
/// [`Section::ALL`].
pub(crate) const fn app_id(n: u8) -> &'static str {
    match n {
        1 => "org.locus.Notes",
        2 => "org.locus.Todo",
        3 => "org.locus.Calendar",
        4 => "org.locus.Photos",
        5 => "org.locus.Files",
        6 => "org.locus.Contacts",
        _ => "org.locus.Desktop",
    }
}

/// Command-line launch: which section to lock to, and what to open in it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Flags {
    section: Option<Section>,
    target: Option<Target>,
    /// The single-instance hand-off: `open` plus the target, sent over D-Bus
    /// to an already running window with the same app id.
    action: String,
    args: Vec<String>,
}

impl Flags {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Flags, String> {
        let mut flags = Flags {
            action: "open".into(),
            ..Flags::default()
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                "--open" => {
                    let raw = args.next().ok_or("--open needs a target")?;
                    let target =
                        Target::parse(&raw).ok_or_else(|| format!("bad target {raw:?}"))?;
                    flags.target = Some(target);
                    flags.args = vec![target.to_string()];
                }
                other if flags.section.is_none() && !other.starts_with('-') => {
                    flags.section = Some(
                        Section::from_slug(other)
                            .ok_or_else(|| format!("unknown section {other:?}"))?,
                    );
                }
                other => return Err(format!("unexpected argument {other:?}")),
            }
        }
        Ok(flags)
    }
}

impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        self.target.is_some().then_some(&self.action)
    }

    fn args(&self) -> Vec<&str> {
        self.args.iter().map(String::as_str).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    /// Probing a saved session.
    Connecting,
    Login,
    Main,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    /// A click or other widget intent.
    Core(Msg),
    /// A key press from the window.
    Key(Key, Mods),
    /// Results of one effect, in order.
    Fx(Vec<Msg>),
    Editor(text_editor::Action),
    /// Thumbnail for `key` (see [`thumb_key`]); `None` when it failed.
    Thumb(String, Option<widget::image::Handle>),
    /// Session probe finished: `true` when the saved session still works.
    Probed(bool),
    ServerEdit(String),
    Scrolled(&'static str, Viewport),
    Resized(Size),
    Tick,
}

pub(crate) struct App {
    core: Core,
    nav: nav_bar::Model,
    /// All app state. Only `locus_core::update` changes it.
    ui: Ui,
    screen: Screen,
    /// The server address in the login form.
    server: String,
    /// The ship code from settings or the environment, tried once at startup.
    startup_code: String,
    client: Option<Client>,
    /// Settings to write once a login typed into the form succeeds.
    save_on_login: Option<config::Settings>,
    /// The pending login came from saved settings or the environment, so it
    /// is not written back.
    auto_login: bool,
    /// A login request is in flight; the form waits for its answer.
    login_pending: bool,
    tz: i32,
    window: Size,
    /// Decoded thumbnails by [`thumb_key`]; `None` when the fetch failed.
    thumbs: HashMap<String, Option<widget::image::Handle>>,
    thumbs_pending: HashSet<String>,
    /// The editor widget's text, kept equal to the core's editor body.
    content: text_editor::Content,
    synced_note: Option<u64>,
    synced_body: String,
    /// The widget id keyboard focus was last moved to for the core's focus.
    focused: Option<String>,
    /// Last known `(offset_y, height, width)` per scrollable id.
    viewports: HashMap<&'static str, (f32, f32, f32)>,
    /// The selection rectangle last scrolled to, so scrolling by hand sticks.
    last_rect: Option<(&'static str, u32)>,
}

impl App {
    fn now(&self) -> locus_core::Now {
        let unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        locus_effects::now(unix, self.tz)
    }

    /// Feed one message to the core, run its effects, and bring the widgets in line.
    pub(crate) fn dispatch(&mut self, msg: Msg) -> Task<Message> {
        match &msg {
            Msg::LoggedIn => self.logged_in(),
            Msg::LoginFailed { .. } => self.login_pending = false,
            Msg::LoadFailed { error, .. } | Msg::WriteFailed { error, .. }
                if error.contains("HTTP 401") || error.contains("HTTP 403") =>
            {
                // The session expired or was revoked: back to the login form.
                self.screen = Screen::Login;
            }
            _ => {}
        }
        let now = self.now();
        let fx = locus_core::update(&mut self.ui, msg, now);
        let mut tasks: Vec<Task<Message>> = fx.into_iter().map(|e| self.effect(e)).collect();
        if self.screen == Screen::Login
            && !self.login_pending
            && self.ui.focus != Focus::Input(InputKind::Login)
        {
            let fx = locus_core::update(&mut self.ui, Msg::BeginLogin, now);
            tasks.extend(fx.into_iter().map(|e| self.effect(e)));
        }
        tasks.push(self.sync());
        Task::batch(tasks)
    }

    fn logged_in(&mut self) {
        self.screen = Screen::Main;
        self.login_pending = false;
        if let Some(settings) = self.save_on_login.take()
            && let Err(e) = config::save(&settings)
        {
            eprintln!("locus: could not save settings: {e}");
        }
    }

    fn effect(&mut self, effect: Effect) -> Task<Message> {
        match effect {
            Effect::CopyToClipboard(text) => clipboard::write::<Message>(text).map(cosmic::Action::App),
            Effect::Login { code } => self.login(code),
            Effect::OpenSection(section, target) => self.open_section(section, target),
            other => match self.client.clone() {
                Some(client) => run_effect(client, other),
                None => Task::none(),
            },
        }
    }

    /// Open or focus `section`'s own window and show `target` there: run
    /// `locus <section> [--open <target>]`. A running window with that app
    /// id takes the hand-off over D-Bus and the new process exits. On
    /// Wayland an activation token goes along, so the compositor lets the
    /// other window take focus.
    fn open_section(&mut self, section: Section, target: Option<Target>) -> Task<Message> {
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let window = self.core.main_window_id();
        if !wayland || window.is_none() {
            spawn_section(section, target, None);
            return Task::none();
        }
        use cosmic::iced::runtime::platform_specific::{self, wayland::activation};
        let app_id = app_id(section.index() as u8 + 1).to_string();
        cosmic::iced::runtime::task::oneshot(move |channel| {
            cosmic::iced::runtime::Action::PlatformSpecific(platform_specific::Action::Wayland(
                platform_specific::wayland::Action::Activation(activation::Action::RequestToken {
                    app_id: Some(app_id),
                    window,
                    channel,
                }),
            ))
        })
        .then(move |token| {
            spawn_section(section, target, token);
            Task::none()
        })
    }

    /// Another launch of this window's app id handed over (single instance):
    /// show its target. libcosmic raises the window itself.
    fn on_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Message> {
        let cosmic::dbus_activation::Details::ActivateAction { action, args } = msg.msg else {
            return Task::none();
        };
        let Some(target) = (action == "open")
            .then(|| args.first().and_then(|a| Target::parse(a)))
            .flatten()
        else {
            return Task::none();
        };
        if self.screen != Screen::Main {
            // Not logged in yet: open it once the section has loaded.
            self.ui.pending_target = Some(target);
            return Task::none();
        }
        let section = target
            .section()
            .or(self.ui.locked)
            .unwrap_or(self.ui.section);
        self.dispatch(Msg::Open {
            section,
            target: Some(target),
        })
    }

    /// Build a client for the form's server and send the code to the ship.
    fn login(&mut self, code: String) -> Task<Message> {
        let auto = std::mem::take(&mut self.auto_login);
        let server = self.server.trim().to_string();
        let client = Url::parse(&server)
            .map_err(|e| e.to_string())
            .and_then(|url| Client::new(url).map_err(|e| e.to_string()));
        let client = match client {
            Ok(client) => client,
            Err(error) => {
                return cosmic::task::message(Message::Fx(vec![Msg::LoginFailed {
                    error: format!("bad server address: {error}"),
                }]));
            }
        };
        self.save_on_login = (!auto).then(|| config::Settings {
            server,
            code: code.trim().to_string(),
        });
        self.client = Some(client.clone());
        self.login_pending = true;
        run_effect(client, Effect::Login { code })
    }

    /// Bring the header, nav bar, editor widget, keyboard focus, scroll
    /// position and thumbnails in line with the core. Runs after every dispatch.
    fn sync(&mut self) -> Task<Message> {
        let section = self.ui.section;
        let title = if self.screen == Screen::Main {
            section.label().to_string()
        } else {
            view_model::window_title(&self.ui)
        };
        self.core.set_header_title(title);
        let target = self
            .nav
            .iter()
            .find(|e| self.nav.data::<Section>(*e) == Some(&section));
        if let Some(entity) = target
            && !self.nav.is_active(entity)
        {
            self.nav.activate(entity);
        }

        self.sync_editor();

        let mut tasks = Vec::new();
        let want = self.focus_id();
        if want != self.focused {
            tasks.push(match &want {
                Some(id) => widget::text_input::focus(widget::Id::new(id.clone())),
                None => unfocus(),
            });
            self.focused = want;
        }

        if self.screen == Screen::Main {
            tasks.push(self.follow_selection());
            tasks.push(self.request_thumbs());
        }
        Task::batch(tasks)
    }

    /// The widget id that should hold keyboard focus for the core's focus.
    fn focus_id(&self) -> Option<String> {
        if self.screen == Screen::Connecting {
            return None;
        }
        match self.ui.focus {
            Focus::Editor => Some(view::EDITOR_ID.into()),
            Focus::Input(InputKind::Filter) => Some(view::FILTER_ID.into()),
            Focus::Input(_) => Some(view::INPUT_ID.into()),
            Focus::Form => self.ui.form.as_ref().map(|f| view::form_id(f.active)),
            Focus::Palette => Some(view::PALETTE_ID.into()),
            _ => None,
        }
    }

    /// Keep `content` equal to the core's editor body. The widget is only
    /// replaced when the core's text changed by some other route (opening a
    /// note, a reload, a conflict), never on the user's own edits.
    fn sync_editor(&mut self) {
        match &self.ui.editor {
            Some(ed) => {
                if self.synced_note != Some(ed.note_id) || self.synced_body != ed.body {
                    self.content = text_editor::Content::with_text(&ed.body);
                    self.synced_note = Some(ed.note_id);
                    self.synced_body = ed.body.clone();
                }
            }
            None => {
                if self.synced_note.take().is_some() {
                    self.content = text_editor::Content::new();
                    self.synced_body.clear();
                }
            }
        }
    }

    /// Scroll the current list just enough to show the selected row, once
    /// per selection change, so scrolling by hand is not undone.
    fn follow_selection(&mut self) -> Task<Message> {
        let Some((id, top, height)) = view::selection_rect(self) else {
            return Task::none();
        };
        let key = (id, top.round() as u32);
        if self.last_rect == Some(key) {
            return Task::none();
        }
        self.last_rect = Some(key);
        let (offset, view_h, _) = self
            .viewports
            .get(id)
            .copied()
            .unwrap_or((0.0, self.default_view_height(), 0.0));
        let y = if top < offset {
            top
        } else if top + height > offset + view_h {
            top + height - view_h
        } else {
            return Task::none();
        };
        cosmic::iced::widget::scrollable::scroll_to(
            widget::Id::new(id),
            cosmic::iced::widget::scrollable::AbsoluteOffset {
                x: None,
                y: Some(y.max(0.0)),
            },
        )
    }

    /// A conservative guess at a list's height before it has reported one.
    fn default_view_height(&self) -> f32 {
        (self.window.height - 260.0).max(120.0)
    }

    /// Width the photo grid gets: the reported one, or a guess from the window.
    pub(crate) fn photo_width(&self) -> f32 {
        if let Some((_, _, w)) = self.viewports.get(view::PHOTOS_SCROLL)
            && *w > 0.0
        {
            return *w;
        }
        let nav = if self.core.is_condensed()
            || !self.core.nav_bar_active()
            || !view_model::show_sections(&self.ui)
        {
            0.0
        } else {
            view::NAV_W
        };
        self.window.width - nav - view::PAGE_PAD_X
    }

    fn report_layout(&mut self) -> Task<Message> {
        let photo_columns = view::photo_columns(self.photo_width());
        if photo_columns == self.ui.photos.columns {
            return Task::none();
        }
        self.dispatch(Msg::Layout { photo_columns })
    }

    /// Fetch the thumbnails the visible grid rows (or the viewer) need, once each.
    fn request_thumbs(&mut self) -> Task<Message> {
        if self.ui.section != Section::Photos {
            return Task::none();
        }
        let Some(client) = self.client.clone() else {
            return Task::none();
        };
        let mut wanted: Vec<(String, u32)> = Vec::new();
        if self.ui.focus == Focus::Detail {
            if let Some(sha) = view_model::photo_detail(&self.ui).and_then(|d| d.hash) {
                wanted.push((sha, THUMB_VIEW));
            }
        } else {
            let cols = self.ui.photos.columns.max(1);
            let pitch = view::photo_pitch(self.photo_width(), cols);
            let (offset, view_h, _) = self
                .viewports
                .get(view::PHOTOS_SCROLL)
                .copied()
                .unwrap_or((0.0, self.default_view_height() + 200.0, 0.0));
            let first = (offset / pitch).floor().max(0.0) as usize;
            let last = ((offset + view_h) / pitch).ceil() as usize + 1;
            let grid = view_model::photo_grid(&self.ui, cols);
            for row in grid.rows.iter().skip(first).take(last.saturating_sub(first)) {
                wanted.extend(row.iter().filter_map(|c| c.hash.clone().map(|h| (h, THUMB_GRID))));
            }
        }
        let mut tasks = Vec::new();
        for (sha, size) in wanted {
            let key = thumb_key(&sha, size);
            if self.thumbs.contains_key(&key) || !self.thumbs_pending.insert(key.clone()) {
                continue;
            }
            let client = client.clone();
            tasks.push(cosmic::task::future(async move {
                let decoded = match locus_effects::thumb(&client, &sha, size).await {
                    Ok(bytes) => tokio::task::spawn_blocking(move || platform::decode(&bytes))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string())),
                    Err(e) => Err(e),
                };
                if let Err(e) = &decoded {
                    eprintln!("locus: thumbnail {sha}@{size}: {e}");
                }
                Message::Thumb(key, decoded.ok())
            }));
        }
        Task::batch(tasks)
    }

    /// Keys the core should see. While a text widget has focus, plain
    /// characters and Backspace/Delete are typing, so they never reach the
    /// keymap; Escape, Enter, Tab, arrows and Ctrl/Alt combos still do.
    fn on_key(&mut self, key: Key, mods: Mods) -> Task<Message> {
        if self.screen == Screen::Connecting {
            return Task::none();
        }
        let typing = self.screen == Screen::Login
            || matches!(
                self.ui.focus,
                Focus::Editor | Focus::Input(_) | Focus::Form | Focus::Palette
            );
        if typing
            && !mods.ctrl
            && !mods.alt
            && matches!(key, Key::Char(_) | Key::Backspace | Key::Delete)
        {
            return Task::none();
        }
        match locus_core::keymap(&self.ui, key, mods) {
            Some(msg) => self.dispatch(msg),
            None => Task::none(),
        }
    }

    fn on_editor(&mut self, action: text_editor::Action) -> Task<Message> {
        let mut tasks = Vec::new();
        let takes_focus = matches!(
            action,
            text_editor::Action::Click(_) | text_editor::Action::Drag(_) | text_editor::Action::Edit(_)
        );
        if takes_focus && self.ui.focus != Focus::Editor {
            // A click into the editor takes focus from the list.
            tasks.push(self.dispatch(Msg::EditorFocus));
        }
        let edit = action.is_edit();
        self.content.perform(action);
        if edit {
            let text = self.content.text();
            self.synced_body = text.clone();
            tasks.push(self.dispatch(Msg::EditorChanged(text)));
        }
        Task::batch(tasks)
    }

    fn on_probe(&mut self, ok: bool) -> Task<Message> {
        if ok {
            self.screen = Screen::Main;
            // The window's section (or Notes); a pending `--open` target
            // survives this switch and opens once the section has loaded.
            let section = self.ui.section;
            return self.dispatch(Msg::Switch(section));
        }
        self.screen = Screen::Login;
        let code = std::mem::take(&mut self.startup_code);
        if code.is_empty() {
            return self.dispatch(Msg::BeginLogin);
        }
        self.auto_login = true;
        self.dispatch(Msg::Batch(vec![
            Msg::BeginLogin,
            Msg::Text(TextEdit::Set(code)),
            Msg::Submit,
        ]))
    }

    fn on_scrolled(&mut self, id: &'static str, viewport: Viewport) -> Task<Message> {
        let bounds = viewport.bounds();
        self.viewports.insert(
            id,
            (viewport.absolute_offset().y, bounds.height, bounds.width),
        );
        let layout = if id == view::PHOTOS_SCROLL {
            self.report_layout()
        } else {
            Task::none()
        };
        let thumbs = self.request_thumbs();
        Task::batch([layout, thumbs])
    }
}

/// Run one effect on the runtime and feed its results back as one message.
fn run_effect(client: Client, effect: Effect) -> Task<Message> {
    cosmic::task::future(async move {
        let desk = platform::Desk {
            client: client.clone(),
            cache: platform::cache_dir(),
        };
        Message::Fx(locus_effects::run(&client, &desk, effect).await)
    })
}

/// Start `locus <section> [--open <target>]` detached from this window.
fn spawn_section(section: Section, target: Option<Target>, token: Option<String>) {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().unwrap_or_else(|_| "locus".into());
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(section.slug());
    if let Some(target) = target {
        cmd.arg("--open").arg(target.to_string());
    }
    cmd.env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID")
        .stdin(std::process::Stdio::null())
        .process_group(0);
    if let Some(token) = token {
        cmd.env("XDG_ACTIVATION_TOKEN", token);
    }
    match cmd.spawn() {
        // Reap it: a launch that hands off to a running window exits at once.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("locus: could not start locus {}: {e}", section.slug()),
    }
}

/// Take keyboard focus away from every widget.
fn unfocus() -> Task<Message> {
    cosmic::iced::runtime::task::effect(cosmic::iced::runtime::Action::widget(
        cosmic::iced::core::widget::operation::focusable::unfocus(),
    ))
}

pub(crate) fn thumb_key(sha: &str, size: u32) -> String {
    format!("{sha}@{size}")
}

/// The libcosmic hooks, called by [`Window`].
impl App {
    fn init(mut core: Core, flags: Flags) -> (Self, Task<Message>) {
        // Tab and Escape belong to the core's keymap, not to libcosmic's
        // focus cycling.
        core.set_keyboard_nav(false);
        let settings = config::load();
        let server = if settings.server.is_empty() {
            DEFAULT_SERVER.to_string()
        } else {
            settings.server.clone()
        };
        let mut app = App {
            core,
            nav: section_nav(),
            ui: Ui::new(),
            screen: Screen::Login,
            server,
            startup_code: String::new(),
            client: None,
            save_on_login: None,
            auto_login: false,
            login_pending: false,
            tz: TZ_OFFSET.get().copied().unwrap_or(0),
            window: Size::new(1200.0, 780.0),
            thumbs: HashMap::new(),
            thumbs_pending: HashSet::new(),
            content: text_editor::Content::new(),
            synced_note: None,
            synced_body: String::new(),
            focused: None,
            viewports: HashMap::new(),
            last_rect: None,
        };
        app.ui.focus_section(flags.section);
        if let Some(target) = flags.target {
            // `locus --open note:3` opens Notes; a locked window keeps its own.
            if flags.section.is_none()
                && let Some(section) = target.section()
            {
                app.ui.section = section;
            }
            app.ui.pending_target = Some(target);
        }
        app.core.nav_bar_set_toggled(true);
        let mut tasks = Vec::new();

        let client = Url::parse(&app.server).ok().and_then(|u| Client::new(u).ok());
        match client {
            Some(client) if !settings.code.is_empty() => {
                app.screen = Screen::Connecting;
                app.startup_code = settings.code.clone();
                app.client = Some(client.clone());
                tasks.push(cosmic::task::future(async move {
                    Message::Probed(locus_effects::session_ok(&client).await)
                }));
            }
            _ => {
                app.ui.info("Enter the server address and your ship's web code");
                tasks.push(app.dispatch(Msg::BeginLogin));
            }
        }
        tasks.push(app.report_layout());
        (app, Task::batch(tasks))
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        (self.screen == Screen::Main && view_model::show_sections(&self.ui)).then_some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        match self.nav.data::<Section>(id).copied() {
            Some(section) => self.dispatch(Msg::Switch(section)),
            None => Task::none(),
        }
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        if self.screen != Screen::Main {
            return Vec::new();
        }
        view::header_end()
    }

    fn footer(&self) -> Option<Element<'_, Message>> {
        (self.screen == Screen::Main).then(|| view::footer(self))
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        if self.screen != Screen::Main {
            return None;
        }
        view::overlay(self)
    }

    fn on_window_resize(&mut self, width: f32, height: f32) {
        self.window = Size::new(width, height);
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch(vec![
            // Captured events are included on purpose: a focused text widget
            // captures keys (Enter, Escape, Tab) the core also needs.
            event::listen_with(|event, _status, _id| match event {
                Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                    Some(Message::Key(keys::key(&key)?, keys::mods(modifiers)))
                }
                Event::Window(window::Event::Resized(size)) => Some(Message::Resized(size)),
                _ => None,
            }),
            cosmic::iced::time::every(TICK).map(|_| Message::Tick),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Core(msg) => self.dispatch(msg),
            Message::Key(key, mods) => self.on_key(key, mods),
            Message::Fx(msgs) => {
                let tasks: Vec<_> = msgs.into_iter().map(|m| self.dispatch(m)).collect();
                Task::batch(tasks)
            }
            Message::Editor(action) => self.on_editor(action),
            Message::Thumb(key, handle) => {
                self.thumbs_pending.remove(&key);
                self.thumbs.insert(key, handle);
                Task::none()
            }
            Message::Probed(ok) => self.on_probe(ok),
            Message::ServerEdit(server) => {
                self.server = server;
                Task::none()
            }
            Message::Scrolled(id, viewport) => self.on_scrolled(id, viewport),
            Message::Resized(size) => {
                self.window = size;
                // The grid's width changes with the window; the next viewport
                // report from the grid replaces this guess.
                self.viewports.remove(view::PHOTOS_SCROLL);
                self.report_layout()
            }
            // Refresh the core's clock (today, relative dates).
            Message::Tick => self.dispatch(Msg::Batch(Vec::new())),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        view::content(self)
    }
}

/// The `cosmic::Application` for app id [`app_id`]`(N)`. Each section's
/// window is its own type because libcosmic reads the app id from a const;
/// everything else is [`App`].
pub(crate) struct Window<const N: u8>(App);

impl<const N: u8> cosmic::Application for Window<N> {
    type Executor = executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = app_id(N);

    fn core(&self) -> &Core {
        &self.0.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.0.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        let (app, task) = App::init(core, flags);
        let mut window = Window(app);
        let title = view_model::window_title(&window.0.ui);
        let title_task = match window.0.core.main_window_id() {
            Some(id) => window.set_window_title(title, id),
            None => Task::none(),
        };
        (window, Task::batch([title_task, task]))
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        self.0.nav_model()
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.0.on_nav_select(id)
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        self.0.header_end()
    }

    fn footer(&self) -> Option<Element<'_, Message>> {
        self.0.footer()
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        self.0.dialog()
    }

    fn on_window_resize(&mut self, _id: window::Id, width: f32, height: f32) {
        self.0.on_window_resize(width, height);
    }

    fn subscription(&self) -> Subscription<Message> {
        self.0.subscription()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        self.0.update(message)
    }

    fn view(&self) -> Element<'_, Message> {
        self.0.view()
    }

    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Message> {
        self.0.on_activation(msg)
    }
}

/// The nav bar: one entry per section, in the core's order.
fn section_nav() -> nav_bar::Model {
    let mut nav = nav_bar::Model::default();
    for section in Section::ALL {
        nav.insert()
            .text(section.label())
            .icon(widget::icon::from_name(view::section_icon(section)))
            .data(section);
    }
    nav
}
