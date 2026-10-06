//! Notes window: login to the household ship, then list / open / edit.
//!
//! Markdown stays visible in Editor (`**bold**` keeps the stars) and is styled
//! in place as GitHub Flavored Markdown plus `[[wikilinks]]`. View → Write
//! hides the markers and offers formatting controls. Preview, a graph, and
//! delete (right-click a note) are here. There is no Save / unsaved state.
//!
//! Default ship URL is the bring-up VM: 127.0.0.1:8080.

mod chrome;
mod graph;
mod highlight;
mod markdown;
mod search;
mod smooth_scroll;
mod source;
mod vim;

use cosmic::app::{Core, Settings, Task};
use cosmic::iced::clipboard;
use cosmic::iced::keyboard::{self, Key};
use cosmic::iced::window;
use cosmic::iced::{event, keyboard::key::Named, Alignment, Event, Length, Size, Subscription};
use cosmic::prelude::*;
use cosmic::widget::menu::KeyBind;
use cosmic::widget::nav_bar;
use cosmic::widget::text_editor;
use cosmic::{executor, widget, Element};
use notes_sync::{
    fetch_vm_web_code, keep_both, load_saved, push_dirty, save_session, store_path, vault_root,
    vault_sync, Note, Session, Snapshot, Store,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const IDLE_MS: u64 = 1100;
const VAULT_TICK_SECS: u64 = 60;
const FOCUS_REFRESH: Duration = Duration::from_secs(15);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default()
        .antialiasing(false)
        .client_decorations(true)
        .default_icon_theme("Cosmic")
        .resizable(Some(8.0))
        .size(Size::new(1200.0, 760.0))
        .transparent(true);
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}

enum Screen {
    Login,
    Main,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Pane {
    Page,
    Write,
    Graph,
    Preview,
}

struct App {
    core: Core,
    nav_model: nav_bar::Model,
    key_binds: HashMap<KeyBind, chrome::MenuAct>,
    screen: Screen,
    pane: Pane,
    eyre_url: String,
    web_code: String,
    status: String,
    busy: bool,
    syncing: bool,
    session: Option<Session>,
    store: Store,
    notes: Vec<Note>,
    cache: HashMap<u64, Note>,
    history: Vec<Snapshot>,
    show_history: bool,
    undo: Vec<EditPoint>,
    redo: Vec<EditPoint>,
    undo_coalesce: UndoCoalesce,
    search: String,
    selected: Option<u64>,
    title: String,
    body: text_editor::Content,
    revision: u64,
    idle_gen: u64,
    pending: Option<Pending>,
    pending_delete: Option<u64>,
    conflict: Option<Note>,
    sort_title: bool,
    accent: chrome::Accent,
    vim: vim::State,
    vim_keys: String,
    select_anchor: Option<(usize, usize)>,
    history_pick: usize,
    nav_width: f32,
    nav_dragging: bool,
    back: Vec<u64>,
    forward: Vec<u64>,
    pinned: BTreeSet<u64>,
    dirty_ids: HashSet<u64>,
    find: Option<String>,
    win_title: String,
    last_refresh: Option<Instant>,
}

enum Pending {
    Open(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UndoCoalesce {
    None,
    Insert,
    Delete,
    Title,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EditPoint {
    title: String,
    body: String,
    line: usize,
    column: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Url(String),
    Code(String),
    FetchVmCode,
    VmCode(Result<String, String>),
    Connect,
    Connected(Result<Session, String>),
    Search(String),
    Refresh,
    Listed(Result<Vec<Note>, String>),
    NewNote,
    Created(Result<Note, String>),
    Open(u64),
    Opened(Result<Note, String>),
    Title(String),
    Body(text_editor::Action),
    Idle(u64),
    Commit,
    Pushed(Result<PushResult, String>),
    Restore(u64),
    ShowHistory,
    CloseHistory,
    Undo,
    Redo,
    DeleteAskNote(u64),
    Delete,
    CancelDelete,
    ConflictBoth,
    ConflictMine,
    ConflictShip,
    OpenWiki(String),
    CreateLinked(String),
    InsertWiki,
    InsertBold,
    InsertItalic,
    InsertStrike,
    InsertCode,
    FormatHeading,
    FormatBullet,
    FormatNumbered,
    FormatTask,
    FormatQuote,
    Duplicate,
    ToggleTask(usize),
    OpenUrl(String),
    GoBack,
    GoForward,
    DailyNow,
    SyncNow,
    WindowFocused,
    TogglePin(u64),
    OpenVaultFile,
    CopyNoteMd,
    FindToggle,
    FindClose,
    FindInput(String),
    FindNext(bool),
    EditCut,
    EditCopy,
    EditRequestPaste,
    EditPasteIn(String),
    SortTitle,
    SetSortTitle(bool),
    SetPane(Pane),
    SetCaret(usize, usize),
    DragSelect(usize, usize, usize, usize),
    VimKeys(String),
    VimRun(String),
    HistoryPick(usize),
    FocusSearch,
    PrevNote,
    NextNote,
    DeleteCurrent,
    SetAccent(chrome::Accent),
    ToggleVim,
    VimMode(vim::Mode),
    Redraw,
    VaultTick,
    NavResizeStart,
    NavResize(f32),
}

#[derive(Debug, Clone)]
enum PushResult {
    Saved(Note),
    Conflict(Note),
}

enum ConflictChoice {
    Both,
    Mine,
    Ship,
}

fn files_dir() -> PathBuf {
    let dir = notes_sync::data_dir().join("files");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
    map: impl FnOnce(Result<T, String>) -> Message + Send + 'static,
) -> Task<Message> {
    cosmic::task::future(async move {
        let result =
            tokio::task::spawn_blocking(work).await.unwrap_or_else(|e| Err(e.to_string()));
        map(result)
    })
}

fn is_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | ')' | ']')
}

fn byte_offset(text: &str, line: usize, column: usize) -> usize {
    let mut off = 0;
    for (i, raw) in text.split('\n').enumerate() {
        if i == line {
            return off + source::clamp_column(raw, column);
        }
        off += raw.len() + 1;
    }
    text.len()
}

fn position_at(text: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(text.len());
    let mut start = 0;
    for (i, raw) in text.split('\n').enumerate() {
        let end = start + raw.len();
        if byte <= end {
            return (i, source::clamp_column(raw, byte - start));
        }
        start = end + 1;
    }
    (
        text.split('\n').count().saturating_sub(1),
        0,
    )
}

fn note_title(title: &str) -> String {
    if title.trim().is_empty() {
        "Untitled".into()
    } else {
        title.to_string()
    }
}

/// Local date of "now" (UTC) as `YYYY-MM-DD`; used for the daily note title.
fn today_title() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's civil-from-days: day count since 1970-01-01 to (y, m, d).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

impl cosmic::Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "org.digital-workspace.notes";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let saved = load_saved();
        let store = Store::open(store_path()).unwrap_or_else(|_| {
            Store::open_memory().expect("in-memory notes store")
        });
        if let Some(root) = vault_root() {
            let _ = vault_sync(&store, &root);
        }
        let cached = store.list_heads().unwrap_or_default();
        let mut cache = HashMap::new();
        for note in &cached {
            cache.insert(note.id, note.clone());
        }
        let mut app = Self {
            core,
            nav_model: nav_bar::Model::default(),
            key_binds: chrome::key_binds(),
            screen: Screen::Login,
            pane: Pane::Page,
            eyre_url: saved.eyre_url.clone(),
            web_code: String::new(),
            status: String::new(),
            busy: false,
            syncing: false,
            session: None,
            store,
            notes: cached,
            cache,
            history: Vec::new(),
            show_history: false,
            undo: Vec::new(),
            redo: Vec::new(),
            undo_coalesce: UndoCoalesce::None,
            search: String::new(),
            selected: None,
            title: String::new(),
            body: text_editor::Content::new(),
            revision: 0,
            idle_gen: 0,
            pending: None,
            pending_delete: None,
            conflict: None,
            sort_title: false,
            accent: chrome::load_accent(),
            vim: vim::State::load(),
            vim_keys: String::new(),
            select_anchor: None,
            history_pick: 0,
            nav_width: chrome::load_nav_width(),
            nav_dragging: false,
            back: Vec::new(),
            forward: Vec::new(),
            pinned: chrome::pinned_ids(),
            dirty_ids: HashSet::new(),
            find: None,
            win_title: "Notes — Locus".into(),
            last_refresh: None,
        };
        app.set_header_title("Notes".into());
        app.core.window.show_headerbar = true;
        app.core.window.show_close = true;
        app.core.window.show_maximize = true;
        app.core.window.show_minimize = true;
        app.core.window.sharp_corners = false;
        let title_task = match app.core.main_window_id() {
            Some(id) => app.set_window_title("Notes — Locus".into(), id),
            None => Task::none(),
        };
        let title_task =
            title_task.chain(cosmic::command::set_theme(chrome::theme_for(app.accent)));

        let has_vault = vault_root().is_some() && !app.notes.is_empty();
        if has_vault {
            app.enter_main();
        }
        let auto = if let Some(key) = saved.api_key.filter(|k| !k.is_empty()) {
            app.busy = true;
            if !has_vault {
                app.status = "Reconnecting…".into();
            }
            let url = saved.eyre_url;
            title_task.chain(run_blocking(
                move || Session::connect_with_api_key(&url, &key).map_err(|e| e.to_string()),
                Message::Connected,
            ))
        } else {
            title_task
        };
        (app, auto)
    }

    fn on_app_exit(&mut self) -> Option<Self::Message> {
        self.flush_local();
        None
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        matches!(self.screen, Screen::Main).then_some(&self.nav_model)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        self.nav_model.activate(id);
        let Some(&note_id) = self.nav_model.data::<u64>(id) else {
            return Task::none();
        };
        self.update(Message::Open(note_id))
    }

    fn nav_context_menu(&self) -> Option<Vec<cosmic::widget::menu::Tree<cosmic::Action<Self::Message>>>> {
        Some(chrome::nav_context_menu(&self.nav_model, &self.pinned))
    }

    fn nav_bar(&self) -> Option<Element<'_, cosmic::Action<Self::Message>>> {
        if !matches!(self.screen, Screen::Main) || !self.core().nav_bar_active() {
            return None;
        }
        let nav_model = self.nav_model()?;
        let space = cosmic::theme::spacing();
        let list = widget::column::with_capacity(2)
            .spacing(space.space_xxs)
            .padding([space.space_xxs, space.space_xxs, 0, space.space_xxs])
            .push(chrome::nav_tools(&self.search, self.sort_title, self.busy))
            .push(chrome::note_nav(
                nav_model,
                &self.notes,
                &self.search,
                &self.pinned,
                &self.dirty_ids,
            ))
            .width(Length::Fill)
            .height(Length::Fill);
        let condensed = self.core().is_condensed();
        let content: Element<'_, cosmic::Action<Self::Message>> = if condensed {
            list.into()
        } else {
            widget::row::with_capacity(2)
                .push(list)
                .push(chrome::nav_resize_handle())
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        let width = if condensed {
            Length::Fill
        } else {
            Length::Fixed(self.nav_width)
        };
        Some(
            widget::container(content)
                .width(width)
                .height(Length::Fill)
                .class(cosmic::theme::Container::custom(widget::nav_bar::nav_bar_style))
                .into(),
        )
    }

    fn on_escape(&mut self) -> Task<Self::Message> {
        if self.find.is_some() {
            self.find = None;
            return Task::none();
        }
        if self.conflict.is_some() || self.pending_delete.is_some() || self.show_history {
            self.conflict = None;
            self.pending_delete = None;
            self.show_history = false;
            return Task::none();
        }
        if self.vim.enabled && !matches!(self.vim.mode, vim::Mode::Normal) {
            self.vim.set_mode(vim::Mode::Normal);
            return Task::none();
        }
        if !matches!(self.pane, Pane::Page) {
            self.pane = Pane::Page;
            return Task::none();
        }
        if !self.search.is_empty() {
            self.search.clear();
            self.sync_nav_model();
        }
        Task::none()
    }

    fn on_search(&mut self) -> Task<Self::Message> {
        if !matches!(self.screen, Screen::Main) {
            return Task::none();
        }
        self.core.nav_bar_set_toggled(true);
        widget::text_input::focus(chrome::SEARCH_ID.clone())
    }

    fn footer(&self) -> Option<Element<'_, Self::Message>> {
        matches!(self.screen, Screen::Main).then(|| {
            let counts = self.selected.map(|_| {
                let words = markdown::word_count(&self.body.text());
                (
                    words,
                    markdown::char_count(&self.body.text()),
                    markdown::reading_minutes(words),
                )
            });
            let caret = self.selected.map(|_| {
                let cursor = self.body.cursor();
                (cursor.position.line, cursor.position.column)
            });
            chrome::footer(
                &self.status,
                counts,
                caret,
                self.vim
                    .enabled
                    .then(|| (self.vim.mode.label(), self.vim_keys.clone())),
            )
        })
    }

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        if let Some(host) = &self.conflict {
            return Some(
                widget::dialog()
                    .title("This note changed on the ship")
                    .body(format!("{} · r{}", host.title, host.revision))
                    .primary_action(
                        widget::button::suggested("Keep both").on_press(Message::ConflictBoth),
                    )
                    .secondary_action(
                        widget::button::standard("Keep mine").on_press(Message::ConflictMine),
                    )
                    .tertiary_action(
                        widget::button::text("Use ship version").on_press(Message::ConflictShip),
                    )
                    .into(),
            );
        }
        if self.show_history {
            return Some(chrome::version_history_dialog(&self.history, self.history_pick));
        }
        if self.pending_delete.is_none() {
            return None;
        }
        Some(
            widget::dialog()
                .title("Delete this note?")
                .primary_action(
                    widget::button::destructive("Delete").on_press(Message::Delete),
                )
                .secondary_action(
                    widget::button::standard("Cancel").on_press(Message::CancelDelete),
                )
                .into(),
        )
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let keys = event::listen_with(|event, _status, _id| match event {
            Event::Mouse(cosmic::iced::mouse::Event::ButtonReleased(
                cosmic::iced::mouse::Button::Left,
            )) => Some(Message::Redraw),
            Event::Window(window::Event::Focused) => Some(Message::WindowFocused),
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if modifiers.alt() && !modifiers.command() && !modifiers.control() {
                    return match key.as_ref() {
                        Key::Named(Named::ArrowLeft) => Some(Message::GoBack),
                        Key::Named(Named::ArrowRight) => Some(Message::GoForward),
                        _ => None,
                    };
                }
                if !modifiers.command() {
                    return None;
                }
                match key.as_ref() {
                    Key::Character("n") => Some(Message::NewNote),
                    Key::Character("z") if modifiers.shift() => Some(Message::Redo),
                    Key::Character("z") => Some(Message::Undo),
                    Key::Character("y") => Some(Message::Redo),
                    Key::Character("g") => Some(Message::SetPane(Pane::Graph)),
                    Key::Character("p") => Some(Message::SetPane(Pane::Preview)),
                    Key::Character("b") => Some(Message::InsertBold),
                    Key::Character("i") => Some(Message::InsertItalic),
                    Key::Character("k") => Some(Message::InsertWiki),
                    Key::Character("1") => Some(Message::SetPane(Pane::Page)),
                    Key::Character("2") => Some(Message::SetPane(Pane::Write)),
                    Key::Character("3") => Some(Message::SetPane(Pane::Graph)),
                    Key::Character("s") => Some(Message::Commit),
                    Key::Character("j") => Some(Message::DailyNow),
                    Key::Character("f") if modifiers.shift() => Some(Message::FindToggle),
                    Key::Character("c") if modifiers.shift() => Some(Message::CopyNoteMd),
                    Key::Character("d") if modifiers.shift() => Some(Message::Duplicate),
                    Key::Character("[") => Some(Message::PrevNote),
                    Key::Character("]") => Some(Message::NextNote),
                    Key::Named(Named::Delete) if modifiers.shift() => Some(Message::DeleteCurrent),
                    _ => None,
                }
            }
            _ => None,
        });
        let mut subs = vec![keys];
        if self.nav_dragging {
            subs.push(event::listen_with(|event, _status, _id| {
                if let Event::Mouse(cosmic::iced::mouse::Event::CursorMoved { position }) = event {
                    Some(Message::NavResize(position.x))
                } else {
                    None
                }
            }));
        }
        if matches!(self.screen, Screen::Main) && vault_root().is_some() {
            subs.push(
                cosmic::iced::time::every(Duration::from_secs(VAULT_TICK_SECS))
                    .map(|_| Message::VaultTick),
            );
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Url(s) => self.eyre_url = s,
            Message::Code(s) => self.web_code = s,
            Message::FetchVmCode => {
                self.busy = true;
                self.status = "Asking the test VM for a login code…".into();
                return run_blocking(fetch_vm_web_code, Message::VmCode);
            }
            Message::VmCode(Ok(code)) => {
                self.busy = false;
                self.web_code = code;
                self.status = "Got a login code. Connect when ready.".into();
            }
            Message::VmCode(Err(e)) => {
                self.busy = false;
                self.status = e;
            }
            Message::Connect => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                self.status = "Signing in…".into();
                let url = self.eyre_url.clone();
                let code = self.web_code.clone();
                return run_blocking(
                    move || Session::connect(&url, &code).map_err(|e| e.to_string()),
                    Message::Connected,
                );
            }
            Message::Connected(Ok(session)) => {
                self.busy = false;
                save_session(&session);
                self.status = format!("Signed in · {}", session.notebook_title());
                self.session = Some(session);
                self.enter_main();
                let drain = self.drain_outbox();
                return drain.chain(self.refresh_list());
            }
            Message::Connected(Err(e)) => {
                self.busy = false;
                if load_saved().api_key.filter(|k| !k.is_empty()).is_some() {
                    self.status = format!("offline · {e}");
                    self.enter_main();
                } else {
                    self.status = e;
                }
            }
            Message::Search(s) => {
                self.search = s;
                self.sync_nav_model();
            }
            Message::Refresh => return self.refresh_list(),
            Message::Listed(Ok(notes)) => {
                self.busy = false;
                self.merge_listed(notes);
                self.sync_nav_model();
                if self.selected.is_none() {
                    if let Some(id) = self.notes.first().map(|n| n.id) {
                        return self.open_note(id);
                    }
                }
                self.refresh_status();
            }
            Message::Listed(Err(e)) => {
                self.busy = false;
                self.status = format!("offline · {e}");
                self.enter_main();
            }
            Message::NewNote => {
                if !matches!(self.screen, Screen::Main) {
                    return Task::none();
                }
                self.flush_local();
                if self.selected.is_none() && self.has_draft() {
                    return self.create_from_editor();
                }
                return self.create_empty();
            }
            Message::Created(Ok(note)) => {
                self.busy = false;
                self.cache.insert(note.id, note.clone());
                let _ = self.store.upsert_head(&note);
                let _ = self.store.snapshot(note.id, &note.title, &note.body_md);
                self.apply_note(&note);
                self.refresh_status();
                let title_task = self.update_win_title();
                return title_task.chain(self.take_pending());
            }
            Message::Created(Err(e)) => {
                self.busy = false;
                self.status = e;
            }
            Message::Open(id) => {
                self.pane = Pane::Page;
                if self.selected == Some(id) {
                    return Task::none();
                }
                self.flush_local();
                if self.selected.is_none() && self.has_draft() {
                    self.pending = Some(Pending::Open(id));
                    return self.create_from_editor();
                }
                let next = self.open_note(id);
                return self.chain_sync(next);
            }
            Message::Opened(Ok(note)) => {
                self.busy = false;
                self.cache.insert(note.id, note.clone());
                let _ = self.store.upsert_head(&note);
                if self
                    .store
                    .history(note.id, 1)
                    .ok()
                    .map(|h| h.is_empty())
                    .unwrap_or(true)
                {
                    let _ = self.store.snapshot(note.id, &note.title, &note.body_md);
                }
                self.apply_note(&note);
                self.refresh_status();
                return self.update_win_title();
            }
            Message::Opened(Err(e)) => {
                self.busy = false;
                self.status = e;
            }
            Message::Title(s) => {
                let changed = s != self.title;
                if changed {
                    self.record_undo(UndoCoalesce::Title);
                }
                let boundary = s.chars().last().is_some_and(is_boundary);
                self.title = s;
                if let Some(id) = self.selected {
                    if let Some(note) = self.cache.get_mut(&id) {
                        note.title = self.title.clone();
                    }
                    self.sync_nav_model();
                }
                let title_task = if changed {
                    self.update_win_title()
                } else {
                    Task::none()
                };
                return title_task.chain(self.after_edit(boundary));
            }
            Message::Body(action) => {
                source::clamp_caret(&mut self.body);
                if matches!(self.pane, Pane::Write) {
                    source::prepare_formatted_edit(&mut self.body, &action);
                }
                if matches!(
                    &action,
                    text_editor::Action::Edit(text_editor::Edit::Enter)
                ) && self.body.cursor().selection.is_none()
                {
                    let text = self.body.text();
                    let cursor = self.body.cursor();
                    if let Some(next) =
                        markdown::list_enter(&text, cursor.position.line, cursor.position.column)
                    {
                        self.record_undo(UndoCoalesce::None);
                        self.body = text_editor::Content::with_text(&next.text);
                        self.body.move_to(text_editor::Cursor {
                            position: text_editor::Position {
                                line: next.line,
                                column: next.column,
                            },
                            selection: None,
                        });
                        source::clamp_caret(&mut self.body);
                        if matches!(self.pane, Pane::Write) {
                            source::snap_formatted_caret(&mut self.body, true);
                        }
                        return self.after_edit(true);
                    }
                }
                let prefer_end = match &action {
                    text_editor::Action::Move(m) | text_editor::Action::Select(m) => {
                        source::motion_prefer_end(*m)
                    }
                    _ => true,
                };
                let boundary = match &action {
                    text_editor::Action::Edit(text_editor::Edit::Insert(c)) => is_boundary(*c),
                    text_editor::Action::Edit(text_editor::Edit::Enter) => true,
                    text_editor::Action::Edit(text_editor::Edit::Paste(_)) => true,
                    text_editor::Action::Edit(_) => false,
                    _ => {
                        self.body.perform(action);
                        if matches!(self.pane, Pane::Write) {
                            source::snap_formatted_caret(&mut self.body, prefer_end);
                        }
                        return Task::none();
                    }
                };
                self.record_undo(if matches!(
                    &action,
                    text_editor::Action::Edit(text_editor::Edit::Insert(c)) if !is_boundary(*c)
                ) {
                    UndoCoalesce::Insert
                } else if matches!(
                    &action,
                    text_editor::Action::Edit(text_editor::Edit::Backspace)
                        | text_editor::Action::Edit(text_editor::Edit::Delete)
                ) {
                    UndoCoalesce::Delete
                } else {
                    UndoCoalesce::None
                });
                self.body.perform(action);
                if matches!(self.pane, Pane::Write) {
                    source::snap_formatted_caret(&mut self.body, prefer_end);
                }
                return self.after_edit(boundary);
            }
            Message::Idle(tick) => {
                if tick == self.idle_gen {
                    return self.snapshot_and_sync();
                }
            }
            Message::Commit => return self.snapshot_and_sync(),
            Message::Pushed(Ok(PushResult::Saved(note))) => {
                self.syncing = false;
                if self.selected == Some(note.id) {
                    self.revision = note.revision;
                }
                if let Some(cached) = self.cache.get_mut(&note.id) {
                    cached.revision = note.revision;
                }
                let _ = self.store.set_ship_revision(note.id, note.revision);
                let _ = self.store.clear_dirty(note.id);
                self.refresh_status();
                return self.take_pending();
            }
            Message::Pushed(Ok(PushResult::Conflict(host))) => {
                self.syncing = false;
                self.conflict = Some(host);
            }
            Message::Pushed(Err(e)) => {
                self.syncing = false;
                if let Some(id) = self.selected {
                    let _ = self.store.mark_dirty(id);
                }
                self.status = format!("offline · {e}");
            }
            Message::Restore(seq) => {
                let Some(id) = self.selected else {
                    return Task::none();
                };
                let snap = self
                    .history
                    .iter()
                    .find(|s| s.seq == seq)
                    .cloned()
                    .or_else(|| self.store.snapshot_at(id, seq).ok().flatten());
                let Some(snap) = snap else {
                    return Task::none();
                };
                self.show_history = false;
                self.record_undo(UndoCoalesce::None);
                self.title = snap.title.clone();
                self.body = text_editor::Content::with_text(&snap.body);
                if let Some(note) = self.cache.get_mut(&id) {
                    note.title = snap.title;
                    note.body_md = snap.body;
                }
                self.sync_nav_model();
                let _ = self.store.snapshot(id, &self.title, &self.body.text());
                self.reload_history();
                return self.snapshot_and_sync();
            }
            Message::ShowHistory => {
                if self.selected.is_some() {
                    self.reload_history();
                    self.history_pick = 0;
                    self.show_history = true;
                }
            }
            Message::CloseHistory => {
                self.show_history = false;
            }
            Message::Undo => return self.undo_edit(),
            Message::Redo => return self.redo_edit(),
            Message::DeleteAskNote(id) => {
                if !self.busy {
                    self.pending_delete = Some(id);
                }
            }
            Message::CancelDelete => {
                self.pending_delete = None;
                self.conflict = None;
            }
            Message::Delete => {
                let Some(id) = self.pending_delete.take() else {
                    return Task::none();
                };
                let task = self.delete_note(id);
                return self.update_win_title().chain(task);
            }
            Message::ConflictBoth => return self.resolve_conflict(ConflictChoice::Both),
            Message::ConflictMine => return self.resolve_conflict(ConflictChoice::Mine),
            Message::ConflictShip => return self.resolve_conflict(ConflictChoice::Ship),
            Message::OpenWiki(target) => return self.open_wiki(&target),
            Message::CreateLinked(title) => return self.create_linked(&title),
            Message::InsertWiki => {
                self.record_undo(UndoCoalesce::None);
                self.wrap_or_insert("[[", "]]");
                return self.snapshot_and_sync();
            }
            Message::InsertBold => {
                self.record_undo(UndoCoalesce::None);
                self.wrap_or_insert("**", "**");
                return self.snapshot_and_sync();
            }
            Message::InsertItalic => {
                self.record_undo(UndoCoalesce::None);
                self.wrap_or_insert("*", "*");
                return self.snapshot_and_sync();
            }
            Message::InsertStrike => {
                self.record_undo(UndoCoalesce::None);
                self.wrap_or_insert("~~", "~~");
                return self.snapshot_and_sync();
            }
            Message::InsertCode => {
                self.record_undo(UndoCoalesce::None);
                self.wrap_or_insert("`", "`");
                return self.snapshot_and_sync();
            }
            Message::FormatHeading => {
                self.record_undo(UndoCoalesce::None);
                self.apply_line_text(markdown::cycle_heading);
                return self.snapshot_and_sync();
            }
            Message::FormatBullet => {
                self.record_undo(UndoCoalesce::None);
                self.apply_line_prefix("- ");
                return self.snapshot_and_sync();
            }
            Message::FormatNumbered => {
                self.record_undo(UndoCoalesce::None);
                self.apply_line_prefix("1. ");
                return self.snapshot_and_sync();
            }
            Message::FormatTask => {
                self.record_undo(UndoCoalesce::None);
                self.apply_line_prefix("- [ ] ");
                return self.snapshot_and_sync();
            }
            Message::FormatQuote => {
                self.record_undo(UndoCoalesce::None);
                self.apply_line_prefix("> ");
                return self.snapshot_and_sync();
            }
            Message::Duplicate => return self.duplicate_selected(),
            Message::ToggleTask(line) => {
                self.record_undo(UndoCoalesce::None);
                let next = markdown::toggle_task_line(&self.body.text(), line);
                self.body = text_editor::Content::with_text(&next);
                return self.snapshot_and_sync();
            }
            Message::OpenUrl(url) => {
                let _ = open::that_detached(&url);
            }
            Message::GoBack => return self.go_back(),
            Message::GoForward => return self.go_forward(),
            Message::DailyNow => return self.open_or_create_daily(),
            Message::SyncNow => return self.sync_now(),
            Message::WindowFocused => {
                if matches!(self.screen, Screen::Main)
                    && self.session.is_some()
                    && self
                        .last_refresh
                        .map(|t| t.elapsed() >= FOCUS_REFRESH)
                        .unwrap_or(true)
                {
                    self.last_refresh = Some(Instant::now());
                    return self.drain_outbox().chain(self.refresh_list());
                }
            }
            Message::TogglePin(id) => {
                if self.pinned.contains(&id) {
                    self.pinned.remove(&id);
                } else {
                    self.pinned.insert(id);
                }
                chrome::save_pinned(&self.pinned);
                self.sync_nav_model();
            }
            Message::OpenVaultFile => {
                let Some(id) = self.selected else {
                    return Task::none();
                };
                if !self.store.is_vault(id).unwrap_or(false) {
                    self.status = "Only vault notes live on disk".into();
                    return Task::none();
                }
                if let Ok(Some(file)) = self.store.vault_by_id(id) {
                    let path = PathBuf::from(&file.path);
                    match open::that_detached(&path) {
                        Ok(()) => self.status = format!("Opened {}", file.path),
                        Err(e) => self.status = format!("Could not open {}: {e}", file.path),
                    }
                }
            }
            Message::CopyNoteMd => {
                let title = note_title(&self.title);
                let body = self.body.text();
                let md = if body.trim().is_empty() {
                    title.clone()
                } else {
                    format!("{title}\n\n{body}")
                };
                self.status = "Copied the note as Markdown".into();
                return clipboard::write::<Message>(md).map(cosmic::Action::App);
            }
            Message::FindToggle => {
                if self.find.take().is_some() {
                    self.find = None;
                    return Task::none();
                }
                self.find = Some(self.body.selection().unwrap_or_default());
                return widget::text_input::focus(chrome::FIND_ID.clone());
            }
            Message::FindClose => {
                self.find = None;
            }
            Message::FindInput(query) => {
                if self.find.is_some() {
                    self.find = Some(query.clone());
                    if !query.trim().is_empty() {
                        return self.find_jump(true);
                    }
                }
            }
            Message::FindNext(forward) => return self.find_jump(forward),
            Message::EditCut => {
                if let Some(sel) = self.body.selection() {
                    self.record_undo(UndoCoalesce::None);
                    let (start, end) = self.cursor_offsets();
                    self.splice(start, end, "", start);
                    return clipboard::write::<Message>(sel)
                        .map(cosmic::Action::App)
                        .chain(self.snapshot_and_sync());
                }
            }
            Message::EditCopy => {
                if let Some(sel) = self.body.selection() {
                    return clipboard::write::<Message>(sel).map(cosmic::Action::App);
                }
            }
            Message::EditRequestPaste => {
                return clipboard
                    ::read()
                    .map(|text| Message::EditPasteIn(text.unwrap_or_default()))
                    .map(cosmic::Action::App);
            }
            Message::EditPasteIn(text) => {
                if text.is_empty() {
                    return Task::none();
                }
                self.record_undo(UndoCoalesce::None);
                let (start, end) = self.cursor_offsets();
                self.splice(start, end, &text, start + text.len());
                return self.snapshot_and_sync();
            }
            Message::SortTitle => {
                self.sort_title = !self.sort_title;
                self.sync_nav_model();
            }
            Message::SetSortTitle(by_title) => {
                self.sort_title = by_title;
                self.sync_nav_model();
            }
            Message::SetPane(pane) => {
                if matches!(self.screen, Screen::Main) {
                    self.pane = pane;
                    if matches!(pane, Pane::Write) {
                        source::snap_formatted_caret(&mut self.body, true);
                    }
                }
            }
            Message::FocusSearch => return self.on_search(),
            Message::PrevNote => return self.step_note(-1),
            Message::NextNote => return self.step_note(1),
            Message::DeleteCurrent => {
                if let Some(id) = self.selected {
                    if !self.busy {
                        self.pending_delete = Some(id);
                    }
                }
            }
            Message::SetAccent(accent) => {
                self.accent = accent;
                chrome::save_accent(accent);
                return cosmic::command::set_theme(chrome::theme_for(accent));
            }
            Message::ToggleVim => {
                self.vim.toggle();
                self.vim_keys.clear();
            }
            Message::VimMode(mode) => {
                self.vim.set_mode(mode);
                if !matches!(mode, vim::Mode::Command) && !self.vim.pending.is_empty() {
                    self.vim_keys = self.vim.keys();
                }
            }
            Message::VimKeys(keys) => {
                self.vim_keys = keys;
            }
            Message::VimRun(cmd) => {
                let cmd = cmd.trim();
                if cmd.is_empty() || matches!(cmd, "q" | "q!") {
                    return Task::none();
                }
                if matches!(cmd, "w" | "wq" | "wa") {
                    return self.snapshot_and_sync();
                }
                if let Ok(n) = cmd.parse::<usize>() {
                    let last = self.body.line_count().saturating_sub(1);
                    let line = n.saturating_sub(1).min(last);
                    source::place_caret(&mut self.body, line, 0);
                }
            }
            Message::HistoryPick(i) => {
                if i < self.history.len() {
                    self.history_pick = i;
                }
            }
            Message::Redraw => {
                if self.nav_dragging {
                    self.nav_dragging = false;
                    chrome::save_nav_width(self.nav_width);
                }
            }
            Message::NavResizeStart => {
                self.nav_dragging = true;
            }
            Message::NavResize(x) => {
                if self.nav_dragging {
                    self.nav_width = chrome::clamp_nav_width(x);
                }
            }
            Message::VaultTick => return self.tick_vault(),
            Message::SetCaret(line, column) => {
                let last = self.body.line_count().saturating_sub(1);
                let line = line.min(last);
                let column = self
                    .body
                    .line(line)
                    .map(|l| source::clamp_column(l.text.as_ref(), column))
                    .unwrap_or(0);
                if self.vim.enabled && self.vim.mode.paints_selection() {
                    self.vim.set_mode(vim::Mode::Normal);
                }
                self.select_anchor = Some((line, column));
                source::place_caret(&mut self.body, line, column);
                if matches!(self.pane, Pane::Write) {
                    source::snap_formatted_caret(&mut self.body, true);
                }
            }
            Message::DragSelect(from_line, from_col, line, column) => {
                let last = self.body.line_count().saturating_sub(1);
                let line = line.min(last);
                let column = self
                    .body
                    .line(line)
                    .map(|l| source::clamp_column(l.text.as_ref(), column))
                    .unwrap_or(0);
                let from_line = from_line.min(last);
                let from_col = self
                    .body
                    .line(from_line)
                    .map(|l| source::clamp_column(l.text.as_ref(), from_col))
                    .unwrap_or(0);
                self.select_anchor = Some((from_line, from_col));
                if self.vim.enabled && !self.vim.mode.paints_selection() {
                    self.vim.set_mode(vim::Mode::Visual);
                }
                self.body.move_to(text_editor::Cursor {
                    position: text_editor::Position { line, column },
                    selection: Some(text_editor::Position {
                        line: from_line,
                        column: from_col,
                    }),
                });
                if matches!(self.pane, Pane::Write) {
                    source::snap_formatted_caret(&mut self.body, true);
                }
            }
        }
        Task::none()
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        if !matches!(self.screen, Screen::Main) {
            return Vec::new();
        }
        let can_open_vault = self
            .selected
            .is_some_and(|id| self.store.is_vault(id).unwrap_or(false));
        vec![chrome::menu_bar(
            &self.key_binds,
            &self.pane,
            self.sort_title,
            self.accent,
            self.vim.enabled,
            &markdown::headings(&self.body.text()),
            can_open_vault,
        )]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        if !matches!(self.screen, Screen::Main) {
            return Vec::new();
        }
        chrome::header_end(&self.pane)
    }

    fn system_theme_update(
        &mut self,
        _keys: &[&'static str],
        _new_theme: &cosmic::cosmic_theme::Theme,
    ) -> Task<Self::Message> {
        cosmic::command::set_theme(chrome::theme_for(self.accent))
    }

    fn system_theme_mode_update(
        &mut self,
        _keys: &[&'static str],
        _new_theme: &cosmic::cosmic_theme::ThemeMode,
    ) -> Task<Self::Message> {
        cosmic::command::set_theme(chrome::theme_for(self.accent))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.screen {
            Screen::Login => self.view_login(),
            Screen::Main => self.view_main(),
        }
    }
}

impl App {
    fn apply_note(&mut self, note: &Note) {
        self.selected = Some(note.id);
        self.title = note.title.clone();
        self.body = text_editor::Content::with_text(&note.body_md);
        self.revision = note.revision;
        self.clear_undo();
        self.reload_history();
        self.sync_nav_model();
    }

    fn enter_main(&mut self) {
        self.screen = Screen::Main;
        self.core.nav_bar_set_toggled(true);
        self.sync_nav_model();
    }

    fn sync_nav_model(&mut self) {
        let notes = self.visible_notes();
        let mut model = nav_bar::Model::default();
        for note in &notes {
            let item = model
                .insert()
                .text(note_title(&note.title))
                .data(note.id);
            if Some(note.id) == self.selected {
                item.activate();
            }
        }
        self.nav_model = model;
        self.notes = notes;
    }

    fn step_note(&mut self, delta: isize) -> Task<Message> {
        if !matches!(self.screen, Screen::Main) || self.notes.is_empty() {
            return Task::none();
        }
        let current = self
            .selected
            .and_then(|id| self.notes.iter().position(|n| n.id == id))
            .unwrap_or(0);
        let len = self.notes.len() as isize;
        let next = (current as isize + delta).rem_euclid(len) as usize;
        let id = self.notes[next].id;
        cosmic::Application::update(self, Message::Open(id))
    }

    fn has_draft(&self) -> bool {
        !self.title.trim().is_empty() || !self.body.text().trim().is_empty()
    }

    /// Follow the window title to the note being edited.
    fn update_win_title(&mut self) -> Task<Message> {
        let next = match self.selected {
            Some(_) => format!("{} — Notes — Locus", note_title(&self.title)),
            None => "Notes — Locus".into(),
        };
        if self.win_title == next {
            return Task::none();
        }
        self.win_title = next.clone();
        match self.core.main_window_id() {
            Some(id) => self.set_window_title(next, id),
            None => Task::none(),
        }
    }

    fn find_jump(&mut self, forward: bool) -> Task<Message> {
        let Some(query) = self.find.clone() else {
            return Task::none();
        };
        if query.trim().is_empty() {
            return Task::none();
        }
        let text = self.body.text();
        let matches = search::find_offsets(&text, &query);
        if matches.is_empty() {
            self.status = format!("No matches for “{}”", query.trim());
            return Task::none();
        }
        let caret = self.cursor_offsets().1;
        let index = if forward {
            matches.iter().position(|(s, _)| *s > caret).unwrap_or(0)
        } else {
            matches
                .iter()
                .rposition(|(s, _)| *s < caret)
                .unwrap_or(matches.len() - 1)
        };
        let (start, end) = matches[index];
        let from = position_at(&text, start);
        let to = position_at(&text, end);
        self.body.move_to(text_editor::Cursor {
            position: text_editor::Position {
                line: to.0,
                column: to.1,
            },
            selection: Some(text_editor::Position {
                line: from.0,
                column: from.1,
            }),
        });
        Task::none()
    }

    /// (current match index, total) for the find bar, if it is open.
    fn find_state(&self) -> Option<(usize, usize)> {
        let query = self.find.as_ref()?;
        let text = self.body.text();
        let matches = search::find_offsets(&text, query);
        if matches.is_empty() {
            return Some((0, 0));
        }
        let caret = self.cursor_offsets().1;
        let index = matches
            .iter()
            .position(|(s, _)| *s >= caret)
            .unwrap_or(matches.len() - 1);
        Some((index, matches.len()))
    }

    /// Notes whose `[[wikilinks]]` point at the open note.
    fn backlinks(&self) -> Vec<(u64, String)> {
        let Some(selected) = self.selected else {
            return Vec::new();
        };
        let Some(host) = self.cache.get(&selected) else {
            return Vec::new();
        };
        let key = markdown::normalize_target(&host.title);
        if key.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<(u64, String)> = self
            .cache
            .values()
            .filter(|note| note.id != selected)
            .filter(|note| {
                markdown::wikilinks(&note.body_md).into_iter().any(|link| {
                    let target = markdown::normalize_target(&link.target);
                    target == key || markdown::basename_of(&target) == key
                })
            })
            .map(|note| (note.id, note_title(&note.title)))
            .collect();
        out.sort_by_key(|(id, title)| (title.to_lowercase(), *id));
        out
    }

    fn reload_history(&mut self) {
        self.history = self
            .selected
            .and_then(|id| self.store.history(id, 256).ok())
            .unwrap_or_default();
    }

    fn refresh_status(&mut self) {
        self.dirty_ids = self.store.dirty_ids().unwrap_or_default();
        self.status = match self.selected {
            Some(id) if self.dirty_ids.contains(&id) => "saving…".into(),
            Some(id)
                if self.store.is_vault(id).unwrap_or(false)
                    && !self.dirty_ids.contains(&id) =>
            {
                "vault · saved on disk".into()
            }
            Some(_) => format!("synced · ship r{}", self.revision),
            None => format!("{} notes", self.cache.len()),
        };
    }

    fn current_point(&self) -> EditPoint {
        let cursor = self.body.cursor();
        EditPoint {
            title: self.title.clone(),
            body: self.body.text(),
            line: cursor.position.line,
            column: cursor.position.column,
        }
    }

    fn apply_point(&mut self, point: EditPoint) {
        self.title = point.title;
        self.body = text_editor::Content::with_text(&point.body);
        let last = self.body.line_count().saturating_sub(1);
        let line = point.line.min(last);
        let column = self
            .body
            .line(line)
            .map(|l| source::clamp_column(l.text.as_ref(), point.column))
            .unwrap_or(0);
        self.body.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column },
            selection: None,
        });
        if let Some(id) = self.selected {
            if let Some(note) = self.cache.get_mut(&id) {
                note.title = self.title.clone();
                note.body_md = self.body.text();
            }
        }
        self.sync_nav_model();
    }

    fn record_undo(&mut self, coalesce: UndoCoalesce) {
        let point = self.current_point();
        if self.undo.last() == Some(&point) {
            self.undo_coalesce = coalesce;
            return;
        }
        if coalesce != UndoCoalesce::None && self.undo_coalesce == coalesce {
            self.redo.clear();
            return;
        }
        self.undo.push(point);
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.undo_coalesce = coalesce;
    }

    fn clear_undo(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.undo_coalesce = UndoCoalesce::None;
    }

    fn undo_edit(&mut self) -> Task<Message> {
        let Some(prev) = self.undo.pop() else {
            return Task::none();
        };
        self.redo.push(self.current_point());
        self.undo_coalesce = UndoCoalesce::None;
        self.apply_point(prev);
        self.after_edit(true)
    }

    fn redo_edit(&mut self) -> Task<Message> {
        let Some(next) = self.redo.pop() else {
            return Task::none();
        };
        self.undo.push(self.current_point());
        self.undo_coalesce = UndoCoalesce::None;
        self.apply_point(next);
        self.after_edit(true)
    }

    fn flush_local(&mut self) {
        let Some(id) = self.selected else {
            return;
        };
        let title = note_title(&self.title);
        let body = self.body.text();
        if let Some(note) = self.cache.get_mut(&id) {
            note.title = title.clone();
            note.body_md = body.clone();
        }
        if self.store.snapshot(id, &title, &body).ok().flatten().is_some() {
            self.reload_history();
        }
    }

    fn after_edit(&mut self, boundary: bool) -> Task<Message> {
        if let Some(id) = self.selected {
            if let Some(note) = self.cache.get_mut(&id) {
                note.title = self.title.clone();
                note.body_md = self.body.text();
            }
        }
        if boundary {
            return self.snapshot_and_sync();
        }
        self.refresh_status();
        self.schedule_idle()
    }

    fn schedule_idle(&mut self) -> Task<Message> {
        self.idle_gen = self.idle_gen.wrapping_add(1);
        let tick = self.idle_gen;
        cosmic::task::future(async move {
            tokio::time::sleep(Duration::from_millis(IDLE_MS)).await;
            Message::Idle(tick)
        })
    }

    fn snapshot_and_sync(&mut self) -> Task<Message> {
        if self.selected.is_none() {
            if self.has_draft() {
                return self.create_from_editor();
            }
            return Task::none();
        }
        self.flush_local();
        self.refresh_status();
        if self.selected.is_some_and(|id| self.store.is_vault(id).unwrap_or(false)) {
            self.sync_vault();
            return Task::none();
        }
        self.push_head()
    }

    fn tick_vault(&mut self) -> Task<Message> {
        self.flush_local();
        self.sync_vault();
        Task::none()
    }

    fn sync_vault(&mut self) {
        let Some(root) = vault_root() else {
            return;
        };
        let report = match vault_sync(&self.store, &root) {
            Ok(report) => report,
            Err(e) => {
                self.status = format!("vault sync failed · {e}");
                return;
            }
        };
        if report.pulled + report.pushed + report.conflicts + report.removed == 0 {
            return;
        }
        let refresh_editor = report.pulled + report.conflicts + report.removed > 0;
        let selected = self.selected;
        let editor = self.body.text();
        if let Ok(heads) = self.store.list_heads() {
            self.cache.clear();
            for n in heads {
                self.cache.insert(n.id, n);
            }
        }
        if refresh_editor {
            if let Some(id) = selected {
                if let Some(n) = self.cache.get(&id).cloned() {
                    if n.body_md != editor {
                        self.apply_note(&n);
                    }
                } else {
                    self.selected = None;
                    self.title.clear();
                    self.body = text_editor::Content::new();
                }
            }
        }
        self.sync_nav_model();
        self.refresh_status();
    }

    fn push_head(&mut self) -> Task<Message> {
        if self.syncing {
            return Task::none();
        }
        let Some(id) = self.selected else {
            return Task::none();
        };
        if self.store.is_vault(id).unwrap_or(false) {
            return Task::none();
        }
        let Some(session) = self.session.clone() else {
            let _ = self.store.mark_dirty(id);
            self.status = "offline · queued".into();
            return Task::none();
        };
        let title = note_title(&self.title);
        let body = self.body.text();
        let rev = self.revision;
        self.syncing = true;
        run_blocking(
            move || match session.note_patch(id, &title, &body, rev) {
                Ok(note) => Ok(PushResult::Saved(note)),
                Err(e) if e.is_conflict() => {
                    let host = session.note_open(id).map_err(|e| e.to_string())?;
                    Ok(PushResult::Conflict(host))
                }
                Err(e) => Err(e.to_string()),
            },
            Message::Pushed,
        )
    }

    fn drain_outbox(&mut self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let path = store_path();
        run_blocking(
            move || {
                let store = Store::open(path).map_err(|e| e.to_string())?;
                push_dirty(&session, &store).map_err(|e| e.to_string())?;
                Ok(())
            },
            |result| match result {
                Ok(()) => Message::Refresh,
                Err(e) => Message::Pushed(Err(e)),
            },
        )
    }

    fn resolve_conflict(&mut self, choice: ConflictChoice) -> Task<Message> {
        let Some(host) = self.conflict.take() else {
            return Task::none();
        };
        let Some(id) = self.selected else {
            return Task::none();
        };
        match choice {
            ConflictChoice::Ship => {
                self.apply_note(&host);
                let _ = self.store.upsert_head(&host);
                self.refresh_status();
                return self.update_win_title();
            }
            ConflictChoice::Both => {
                let merged = keep_both(&self.body.text(), &host);
                self.body = text_editor::Content::with_text(&merged);
                self.revision = host.revision;
                if let Some(note) = self.cache.get_mut(&id) {
                    note.body_md = merged.clone();
                    note.revision = host.revision;
                }
                let _ = self.store.snapshot(id, &self.title, &merged);
                self.reload_history();
                self.push_head()
            }
            ConflictChoice::Mine => {
                self.revision = host.revision;
                if let Some(note) = self.cache.get_mut(&id) {
                    note.revision = host.revision;
                }
                self.push_head()
            }
        }
    }

    fn chain_sync(&mut self, next: Task<Message>) -> Task<Message> {
        let push = self.push_head();
        push.chain(next)
    }

    fn create_from_editor(&mut self) -> Task<Message> {
        let title = note_title(&self.title);
        let body = self.body.text();
        let Some(session) = self.session.clone() else {
            match self.store.enqueue_create(&title, &body) {
                Ok(qid) => self.status = format!("offline · queued create pending-{qid}"),
                Err(e) => self.status = e.to_string(),
            }
            return Task::none();
        };
        self.busy = true;
        run_blocking(
            move || session.note_create(&title, &body).map_err(|e| e.to_string()),
            Message::Created,
        )
    }

    fn create_empty(&mut self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            match self.store.enqueue_create("Untitled", "") {
                Ok(qid) => self.status = format!("offline · queued create pending-{qid}"),
                Err(e) => self.status = e.to_string(),
            }
            return Task::none();
        };
        self.busy = true;
        run_blocking(
            move || session.note_create("Untitled", "").map_err(|e| e.to_string()),
            Message::Created,
        )
    }

    /// Open history-aware: pushes the current note onto the back stack.
    fn open_note(&mut self, id: u64) -> Task<Message> {
        if let Some(prev) = self.selected {
            if prev != id && self.back.last() != Some(&prev) {
                self.back.push(prev);
                if self.back.len() > 64 {
                    self.back.remove(0);
                }
                self.forward.clear();
            }
        }
        self.open_note_silent(id)
    }

    fn go_back(&mut self) -> Task<Message> {
        let Some(prev) = self.back.pop() else {
            return Task::none();
        };
        if self.selected.is_some_and(|cur| cur != prev) {
            self.forward.push(self.selected.expect("checked above"));
        }
        if !matches!(self.screen, Screen::Main) {
            self.enter_main();
        }
        self.open_note_silent(prev)
    }

    fn go_forward(&mut self) -> Task<Message> {
        let Some(next) = self.forward.pop() else {
            return Task::none();
        };
        if self.selected.is_some_and(|cur| cur != next) {
            self.back.push(self.selected.expect("checked above"));
        }
        if !matches!(self.screen, Screen::Main) {
            self.enter_main();
        }
        self.open_note_silent(next)
    }

    fn open_or_create_daily(&mut self) -> Task<Message> {
        if !matches!(self.screen, Screen::Main) {
            return Task::none();
        }
        let title = today_title();
        if let Some(note) = self
            .cache
            .values()
            .find(|n| n.title.trim().to_lowercase() == title)
        {
            let id = note.id;
            self.flush_local();
            return self.open_note(id);
        }
        self.flush_local();
        let body = String::new();
        match self.session.clone() {
            Some(session) => {
                self.busy = true;
                self.status = "Creating today's note…".into();
                run_blocking(
                    move || session.note_create(&title, &body).map_err(|e| e.to_string()),
                    Message::Created,
                )
            }
            None => {
                match self.store.enqueue_create(&title, "") {
                    Ok(qid) => self.status = format!("offline · queued today's note pending-{qid}"),
                    Err(e) => self.status = e.to_string(),
                }
                Task::none()
            }
        }
    }

    fn sync_now(&mut self) -> Task<Message> {
        if !matches!(self.screen, Screen::Main) || self.busy || self.syncing {
            return Task::none();
        }
        self.flush_local();
        let _ = self.tick_vault();
        if self.session.is_some() {
            self.status = "Syncing…".into();
            self.syncing = true;
            let drain = self.drain_outbox();
            let refresh = self.refresh_list();
            drain.chain(refresh)
        } else if self.store.dirty_ids().map(|d| !d.is_empty()).unwrap_or(false) {
            self.status = "offline · will retry".into();
            Task::none()
        } else {
            self.refresh_status();
            Task::none()
        }
    }

    /// Open a note without touching the back/forward stacks.
    fn open_note_silent(&mut self, id: u64) -> Task<Message> {
        if self.store.is_vault(id).unwrap_or(false) {
            if let Some(note) = self.cache.get(&id).cloned() {
                self.apply_note(&note);
            } else if let Ok(Some(note)) = self.store.head(id) {
                self.cache.insert(id, note.clone());
                self.apply_note(&note);
            }
            self.refresh_status();
            return Task::none();
        }
        if let Some(note) = self.cache.get(&id).cloned() {
            let _ = self.store.upsert_head(&note);
            self.apply_note(&note);
            self.refresh_status();
        } else if let Ok(Some(note)) = self.store.head(id) {
            self.cache.insert(id, note.clone());
            self.apply_note(&note);
            self.refresh_status();
        }
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        run_blocking(
            move || session.note_open(id).map_err(|e| e.to_string()),
            Message::Opened,
        )
    }

    fn take_pending(&mut self) -> Task<Message> {
        match self.pending.take() {
            Some(Pending::Open(id)) => self.open_note(id),
            None => Task::none(),
        }
    }

    fn merge_listed(&mut self, notes: Vec<Note>) {
        for incoming in notes {
            if incoming.id >= notes_sync::VAULT_ID_BASE
                || self.store.is_vault(incoming.id).unwrap_or(false)
            {
                continue;
            }
            let mut note = incoming;
            if note.body_md.is_empty() {
                if let Ok(Some(local)) = self.store.head(note.id) {
                    if !local.body_md.is_empty() {
                        note.body_md = local.body_md;
                    }
                    if note.title.trim().is_empty() {
                        note.title = local.title;
                    }
                }
            }
            if self.selected == Some(note.id) {
                note.title = self.title.clone();
                note.body_md = self.body.text();
            }
            let _ = self.store.upsert_head(&note);
            self.cache.insert(note.id, note);
        }
    }

    fn visible_notes(&self) -> Vec<Note> {
        let q = self.search.trim();
        let mut notes: Vec<Note> = self.cache.values().cloned().collect();
        let pin = |id: u64| self.pinned.contains(&id);
        if !q.is_empty() {
            notes.retain(|n| search::note_score(&n.title, &n.body_md, q).is_some());
            notes.sort_by(|a, b| {
                let sa = search::note_score(&a.title, &a.body_md, q).unwrap_or(0);
                let sb = search::note_score(&b.title, &b.body_md, q).unwrap_or(0);
                sb.cmp(&sa)
                    .then_with(|| pin(b.id).cmp(&pin(a.id)))
                    .then_with(|| {
                        if self.sort_title {
                            a.title.to_lowercase().cmp(&b.title.to_lowercase())
                        } else {
                            b.id.cmp(&a.id)
                        }
                    })
            });
            return notes;
        }
        if self.sort_title {
            notes.sort_by(|a, b| {
                pin(b.id)
                    .cmp(&pin(a.id))
                    .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                    .then_with(|| b.id.cmp(&a.id))
            });
        } else {
            notes.sort_by_key(|n| (!pin(n.id), std::cmp::Reverse(n.id)));
        }
        notes
    }

    fn refresh_list(&mut self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        if self.cache.is_empty() {
            self.busy = true;
            self.status = "Loading notes…".into();
        }
        run_blocking(
            move || session.note_list().map_err(|e| e.to_string()),
            Message::Listed,
        )
    }

    fn cursor_offsets(&self) -> (usize, usize) {
        let text = self.body.text();
        let cursor = self.body.cursor();
        let a = byte_offset(&text, cursor.position.line, cursor.position.column);
        let b = cursor
            .selection
            .map(|p| byte_offset(&text, p.line, p.column))
            .unwrap_or(a);
        if a <= b {
            (a, b)
        } else {
            (b, a)
        }
    }

    fn splice(&mut self, start: usize, end: usize, insert: &str, caret: usize) {
        let text = self.body.text();
        let start = start.min(text.len());
        let end = end.min(text.len()).max(start);
        let mut next = String::with_capacity(text.len() + insert.len());
        next.push_str(&text[..start]);
        next.push_str(insert);
        next.push_str(&text[end..]);
        let caret = caret.min(next.len());
        self.body = text_editor::Content::with_text(&next);
        let (line, column) = position_at(&next, caret);
        self.body.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column },
            selection: None,
        });
        if let Some(id) = self.selected {
            if let Some(note) = self.cache.get_mut(&id) {
                note.body_md = next;
            }
        }
        if matches!(self.pane, Pane::Write) {
            source::snap_formatted_caret(&mut self.body, true);
        }
    }

    fn wrap_or_insert(&mut self, left: &str, right: &str) {
        let (start, end) = self.cursor_offsets();
        let text = self.body.text();
        let inner = text.get(start..end).unwrap_or("").to_string();
        let caret = start + left.len() + inner.len();
        self.splice(start, end, &format!("{left}{inner}{right}"), caret);
    }

    fn apply_line_prefix(&mut self, marker: &str) {
        let line = self.body.cursor().position.line;
        let next = markdown::toggle_line_prefix(&self.body.text(), line, marker);
        self.replace_body(next, line);
    }

    fn apply_line_text(&mut self, edit: fn(&str, usize) -> String) {
        let line = self.body.cursor().position.line;
        let next = edit(&self.body.text(), line);
        self.replace_body(next, line);
    }

    fn replace_body(&mut self, next: String, line: usize) {
        let column = self.body.cursor().position.column;
        self.body = text_editor::Content::with_text(&next);
        let last = self.body.line_count().saturating_sub(1);
        let line = line.min(last);
        let column = self
            .body
            .line(line)
            .map(|l| source::clamp_column(l.text.as_ref(), column))
            .unwrap_or(0);
        self.body.move_to(text_editor::Cursor {
            position: text_editor::Position { line, column },
            selection: None,
        });
        if let Some(id) = self.selected {
            if let Some(note) = self.cache.get_mut(&id) {
                note.body_md = next;
            }
        }
        if matches!(self.pane, Pane::Write) {
            source::snap_formatted_caret(&mut self.body, true);
        }
    }

    fn delete_note(&mut self, id: u64) -> Task<Message> {
        if self.store.is_vault(id).unwrap_or(false) {
            self.pending_delete = None;
            self.status = "vault notes stay on disk".into();
            return Task::none();
        }
        self.flush_local();
        let was_open = self.selected == Some(id);
        let _ = self.store.delete_note(id);
        self.cache.remove(&id);
        if was_open {
            self.selected = None;
            self.title.clear();
            self.body = text_editor::Content::new();
            self.history.clear();
            self.clear_undo();
            self.show_history = false;
            self.revision = 0;
        }
        self.sync_nav_model();
        let Some(session) = self.session.clone() else {
            let _ = self.store.enqueue_delete(id);
            self.status = "offline · queued delete".into();
            return Task::none();
        };
        run_blocking(
            move || session.note_delete(id).map_err(|e| e.to_string()),
            move |result| match result {
                Ok(()) => Message::Refresh,
                Err(e) => {
                    let _ = Store::open(store_path()).and_then(|s| s.enqueue_delete(id));
                    Message::Pushed(Err(e))
                }
            },
        )
    }

    fn open_wiki(&mut self, target: &str) -> Task<Message> {
        let key = target.trim().to_lowercase();
        if let Some(note) = self
            .cache
            .values()
            .find(|n| n.title.trim().to_lowercase() == key)
        {
            let id = note.id;
            self.flush_local();
            return self.open_note(id);
        }
        self.create_linked(target)
    }

    fn create_linked(&mut self, title: &str) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        let title = note_title(title);
        let from = note_title(&self.title);
        let body = format!("Linked from [[{from}]].\n");
        self.busy = true;
        run_blocking(
            move || session.note_create(&title, &body).map_err(|e| e.to_string()),
            Message::Created,
        )
    }

    fn duplicate_selected(&mut self) -> Task<Message> {
        let Some(session) = self.session.clone() else {
            return Task::none();
        };
        self.flush_local();
        let title = format!("{} copy", note_title(&self.title));
        let body = self.body.text();
        self.busy = true;
        run_blocking(
            move || session.note_create(&title, &body).map_err(|e| e.to_string()),
            Message::Created,
        )
    }

    fn view_login(&self) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        let form = widget::column::with_capacity(8)
            .spacing(space.space_s)
            .push(widget::text::title1("Notes"))
            .push(widget::text::caption("Server"))
            .push(
                widget::text_input("http://127.0.0.1:8080", &self.eyre_url)
                    .on_input(Message::Url)
                    .on_submit(|_| Message::Connect),
            )
            .push(widget::text::caption("Ship login code"))
            .push(
                widget::text_input("word-word-word-word", &self.web_code)
                    .password()
                    .on_input(Message::Code)
                    .on_submit(|_| Message::Connect),
            )
            .push(
                widget::row::with_capacity(2)
                    .spacing(space.space_s)
                    .push(
                        widget::button::standard("Get code from VM")
                            .on_press_maybe((!self.busy).then_some(Message::FetchVmCode)),
                    )
                    .push(
                        widget::button::suggested("Connect")
                            .on_press_maybe((!self.busy).then_some(Message::Connect)),
                    ),
            )
            .push(widget::text::caption(&self.status));

        widget::container(
            widget::container(form)
                .class(cosmic::theme::Container::Card)
                .padding(space.space_l)
                .max_width(480.0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        if self.selected.is_none() && !self.has_draft() {
            return widget::container(chrome::welcome_panel())
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into();
        }
        let center: Element<'_, Message> = match self.pane {
            Pane::Graph => graph::view(graph::build(&self.cache), self.selected),
            Pane::Preview => self.view_preview(),
            Pane::Write => self.view_write(),
            Pane::Page => self.view_page(),
        };
        widget::container(center)
            .padding([space.space_xs, space.space_s])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn editor_column<'a>(&'a self, editor: Element<'a, Message>) -> Element<'a, Message> {
        let space = cosmic::theme::spacing();
        let links = self.backlinks();
        let mut col = widget::column::with_capacity(4)
            .spacing(space.space_xs)
            .push(
                widget::text_input("Untitled", &self.title)
                    .size(22.0)
                    .on_input(Message::Title)
                    .on_submit(|_| Message::Commit),
            )
            .push(chrome::format_bar());
        if self.find.is_some() {
            col = col.push(chrome::find_bar(
                self.find.as_deref().unwrap_or(""),
                self.find_state(),
            ));
        }
        col = col.push(chrome::editor_context_menu(
            widget::container(editor)
                .width(Length::Fill)
                .height(Length::Fill),
        ));
        if !links.is_empty() {
            col = col.push(chrome::backlinks_row(links));
        }
        col.width(Length::Fill).height(Length::Fill).into()
    }

    fn view_page(&self) -> Element<'_, Message> {
        self.editor_column(source::view(&self.body, &self.vim))
    }

    fn view_write(&self) -> Element<'_, Message> {
        self.editor_column(source::view_write(&self.body, &self.vim))
    }

    fn view_preview(&self) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        let body = self.body.text();
        widget::column::with_capacity(2)
            .spacing(space.space_xs)
            .push(
                widget::text_input("Untitled", &self.title)
                    .on_input(Message::Title)
                    .on_submit(|_| Message::Commit),
            )
            .push(markdown::view_visible(&body, &files_dir()))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, search, today_title};

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn known_days_map_to_dates() {
        // 2026-09-14 is 20710 days after the epoch.
        assert_eq!(civil_from_days(20_710), (2026, 9, 14));
        // 2024-02-29 (leap day) is 19782 days after the epoch.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn today_title_shape() {
        let t = today_title();
        assert_eq!(t.len(), 10);
        assert_eq!(t.as_bytes()[4], b'-');
        assert_eq!(t.as_bytes()[7], b'-');
    }

    #[test]
    fn find_offsets_are_sorted_non_overlapping() {
        let got = search::find_offsets("a ab abc", "");
        assert!(got.is_empty());
    }
}
