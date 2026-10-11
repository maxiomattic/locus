//! The complete UI state. Plain data plus small helpers; transitions live in
//! the per-section modules and [`crate::update`].

use std::collections::BTreeSet;

use locus_client::{
    calendar::Event,
    contacts::Contact,
    notes::{Note, NotebookSummary},
    photos::Photo,
};
use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::msg::{FileEntry, Msg, Now};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Section {
    Notes,
    Todo,
    Calendar,
    Photos,
    Files,
    Contacts,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Section::Notes,
        Section::Todo,
        Section::Calendar,
        Section::Photos,
        Section::Files,
        Section::Contacts,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Section::Notes => "Notes",
            Section::Todo => "Todo",
            Section::Calendar => "Calendar",
            Section::Photos => "Photos",
            Section::Files => "Files",
            Section::Contacts => "Contacts",
        }
    }

    /// `1` maps to Notes, `6` to Contacts.
    pub fn from_digit(n: u8) -> Option<Section> {
        Section::ALL.get(usize::from(n).checked_sub(1)?).copied()
    }
}

/// Which single-line prompt has the keyboard. Filter edits `App::filter`;
/// every other kind edits `App::input`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Filter,
    TaskText,
    NewDir,
    RenameNote,
    RenameFile,
    MoveFile,
    Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
    Editor,
    Input(InputKind),
    Form,
    Palette,
    Help,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Confirm {
    pub prompt: String,
    pub on_yes: Msg,
}

/// The open note in the editor. `body` is what the user sees; `last_saved`
/// is the body the server has acknowledged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    pub nb: NotebookSummary,
    pub note_id: u64,
    pub title: String,
    pub body: String,
    pub last_saved: String,
    pub base_revision: u64,
    /// Bumped on every edit; a debounce timer only saves if its token is current.
    pub token: u64,
    pub in_flight: bool,
    /// A save was requested while one was in flight; run it when that one ends.
    pub save_queued: bool,
    /// Set after a conflict: the user's text is kept, the server copy was reloaded.
    pub conflict: bool,
}

impl Editor {
    pub fn dirty(&self) -> bool {
        self.body != self.last_saved
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    NewEvent,
    EditEvent { id: u64, revision: u64 },
    NewContact,
    EditContact { id: u64, revision: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    pub label: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub kind: FormKind,
    pub fields: Vec<FormField>,
    pub active: usize,
    pub error: Option<String>,
}

impl Form {
    pub fn value(&self, label: &str) -> &str {
        self.fields
            .iter()
            .find(|f| f.label == label)
            .map_or("", |f| f.value.as_str())
    }
}

/// Text of the single-line prompt. `target` is the note or file id an action
/// applies to (rename, move), if any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    pub text: String,
    pub target: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaletteState {
    pub query: String,
    pub sel: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotesState {
    pub items: Vec<Note>,
    /// Ids whose `body_md` is known to be the full body.
    pub bodies: BTreeSet<u64>,
    pub sel: usize,
    /// Set by `n` so the created note opens in the editor.
    pub open_on_create: bool,
    /// Source of debounce tokens. Global so a timer for one editor can never
    /// match another editor's current token.
    pub token_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CalMode {
    #[default]
    Agenda,
    Month,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarState {
    pub mode: CalMode,
    pub day: Date,
    pub events: Vec<Event>,
    /// Index into the events on `day`.
    pub sel: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TodoState {
    pub sel: usize,
    pub show_done: bool,
    /// Task text waiting for the Inbox body to load before it can be appended.
    pub pending_add: Option<PendingTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingTask {
    pub text: String,
    pub due: Option<Date>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PhotosState {
    pub items: Vec<Photo>,
    pub sel: usize,
    pub columns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilesState {
    pub files: Vec<FileEntry>,
    /// Explicit folders from the server. Ancestors of file directories are
    /// added on read, see [`crate::files::folders`].
    pub dirs: Vec<String>,
    pub cwd: String,
    pub sel: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContactsState {
    pub items: Vec<Contact>,
    pub sel: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct App {
    pub section: Section,
    pub focus: Focus,
    pub status: Status,
    /// Per section, indexed by [`Section::index`].
    pub loading: [bool; 6],
    /// Per section: has the first load been requested?
    pub loaded: [bool; 6],
    pub filter: String,
    pub input: Input,
    pub form: Option<Form>,
    pub confirm: Option<Confirm>,
    pub palette: PaletteState,
    pub pending_g: bool,
    /// The notebook the Notes and Todo sections work in (the first one on the ship).
    pub notebook: Option<NotebookSummary>,
    pub editor: Option<Editor>,
    pub notes: NotesState,
    pub todo: TodoState,
    pub calendar: CalendarState,
    pub photos: PhotosState,
    pub files: FilesState,
    pub contacts: ContactsState,
    /// The clock from the most recent `update` call; used by view models.
    pub now: Now,
    /// False until the first `update`, which sets the calendar's day to today.
    pub(crate) clock_seen: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> App {
        let today = Date::new(1970, 1, 1).unwrap_or(Date {
            y: 1970,
            m: 1,
            d: 1,
        });
        App {
            section: Section::Notes,
            focus: Focus::List,
            status: Status::default(),
            loading: [false; 6],
            loaded: [false; 6],
            filter: String::new(),
            input: Input::default(),
            form: None,
            confirm: None,
            palette: PaletteState::default(),
            pending_g: false,
            notebook: None,
            editor: None,
            notes: NotesState::default(),
            todo: TodoState::default(),
            calendar: CalendarState {
                mode: CalMode::Agenda,
                day: today,
                events: Vec::new(),
                sel: 0,
            },
            photos: PhotosState {
                columns: 4,
                ..PhotosState::default()
            },
            files: FilesState {
                cwd: "/".into(),
                ..FilesState::default()
            },
            contacts: ContactsState::default(),
            now: Now {
                unix_secs: 0,
                today,
                tz_offset_secs: 0,
            },
            clock_seen: false,
        }
    }

    pub fn is_loading(&self, section: Section) -> bool {
        self.loading[section.index()]
    }

    pub fn set_status(&mut self, level: Level, text: impl Into<String>) {
        self.status = Status {
            level,
            text: text.into(),
        };
    }

    /// Open the confirm prompt. `on_yes` runs if the user answers `y`.
    pub fn ask(&mut self, prompt: impl Into<String>, on_yes: Msg) {
        self.confirm = Some(Confirm {
            prompt: prompt.into(),
            on_yes,
        });
        self.focus = Focus::Confirm;
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.set_status(Level::Info, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.set_status(Level::Warn, text);
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.set_status(Level::Error, text);
    }
}

/// Clamp a selection index into `0..len` (0 when empty).
pub fn clamp_sel(sel: usize, len: usize) -> usize {
    sel.min(len.saturating_sub(1))
}

/// Case-insensitive substring match of `filter` against any of `fields`.
/// An empty filter matches everything.
pub fn matches_filter(filter: &str, fields: &[&str]) -> bool {
    let f = filter.trim();
    if f.is_empty() {
        return true;
    }
    let f = f.to_lowercase();
    fields.iter().any(|t| t.to_lowercase().contains(&f))
}
