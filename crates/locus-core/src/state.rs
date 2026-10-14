//! The complete UI state. Plain data plus small helpers; transitions live in
//! the per-section modules and [`crate::update`].

use std::collections::BTreeSet;

use locus_client::{
    calendar::Event,
    contacts::Contact,
    notes::{Folder, Note, NotebookSummary},
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

    /// The lowercase name used on the command line (`locus notes`) and in
    /// web routes (`/app/notes`).
    pub fn slug(self) -> &'static str {
        match self {
            Section::Notes => "notes",
            Section::Todo => "todo",
            Section::Calendar => "calendar",
            Section::Photos => "photos",
            Section::Files => "files",
            Section::Contacts => "contacts",
        }
    }

    /// Inverse of [`Section::slug`], case-insensitive.
    pub fn from_slug(s: &str) -> Option<Section> {
        Section::ALL
            .into_iter()
            .find(|sec| sec.slug().eq_ignore_ascii_case(s.trim()))
    }
}

/// What to show once a section is open, carried across windows by a
/// cross-link (`--open note:12` on the desktop, `#note:12` on the web).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    /// A note in the default notebook (kept for old links).
    Note(u64),
    /// A note in a named notebook: `note:<host>/<flag_name>/<id>`.
    NoteAt(NoteRef),
    File(u64),
    Contact(u64),
    /// Start a new item in the section (`n`).
    New,
    /// Open the upload picker (Photos, Files).
    Upload,
}

impl Target {
    /// Parse `note:12`, `file:3`, `contact:9`, `new` or `upload`.
    pub fn parse(s: &str) -> Option<Target> {
        let s = s.trim();
        match s {
            "new" => return Some(Target::New),
            "upload" => return Some(Target::Upload),
            _ => {}
        }
        let (kind, id) = s.split_once(':')?;
        if kind == "note"
            && let Some((book, id)) = id.rsplit_once('/')
        {
            let (host, flag_name) = book.split_once('/')?;
            if host.is_empty() || flag_name.is_empty() || flag_name.contains('/') {
                return None;
            }
            return Some(Target::NoteAt(NoteRef {
                nb: NbKey {
                    host: host.to_string(),
                    flag_name: flag_name.to_string(),
                },
                id: id.parse().ok()?,
            }));
        }
        let id: u64 = id.parse().ok()?;
        match kind {
            "note" => Some(Target::Note(id)),
            "file" => Some(Target::File(id)),
            "contact" => Some(Target::Contact(id)),
            _ => None,
        }
    }

    /// The section an item target lives in; `None` for `New` and `Upload`,
    /// which apply to whatever section is open.
    pub fn section(&self) -> Option<Section> {
        match self {
            Target::Note(_) | Target::NoteAt(_) => Some(Section::Notes),
            Target::File(_) => Some(Section::Files),
            Target::Contact(_) => Some(Section::Contacts),
            Target::New | Target::Upload => None,
        }
    }

    /// The target for the item `id` in `section`, where items are addressable.
    pub fn item(section: Section, id: u64) -> Option<Target> {
        match section {
            Section::Notes => Some(Target::Note(id)),
            Section::Files => Some(Target::File(id)),
            Section::Contacts => Some(Target::Contact(id)),
            _ => None,
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Note(id) => write!(f, "note:{id}"),
            Target::NoteAt(r) => write!(f, "note:{}/{}/{}", r.nb.host, r.nb.flag_name, r.id),
            Target::File(id) => write!(f, "file:{id}"),
            Target::Contact(id) => write!(f, "contact:{id}"),
            Target::New => f.write_str("new"),
            Target::Upload => f.write_str("upload"),
        }
    }
}

/// Identifies a notebook across the ship: note ids are only unique inside one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NbKey {
    pub host: String,
    pub flag_name: String,
}

impl NbKey {
    pub fn of(nb: &NotebookSummary) -> NbKey {
        NbKey {
            host: nb.host.clone(),
            flag_name: nb.flag_name.clone(),
        }
    }

    pub fn is(&self, nb: &NotebookSummary) -> bool {
        self.host == nb.host && self.flag_name == nb.flag_name
    }
}

/// A note anywhere on the ship: its notebook and its id there.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NoteRef {
    pub nb: NbKey,
    pub id: u64,
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
    /// Does this editor hold note `id` of notebook `nb`?
    pub fn is(&self, nb: &NbKey, id: u64) -> bool {
        self.note_id == id && nb.is(&self.nb)
    }

    /// `host/flag_name/id`: names this note across notebooks.
    pub fn uid(&self) -> String {
        format!("{}/{}/{}", self.nb.host, self.nb.flag_name, self.note_id)
    }

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

/// One notebook with everything loaded from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub nb: NotebookSummary,
    pub folders: Vec<Folder>,
    pub notes: Vec<Note>,
    /// Ids whose `body_md` is known to be the full body.
    pub bodies: BTreeSet<u64>,
    /// Ids a body fetch was already sent for (Todo), so it is sent once.
    pub requested: BTreeSet<u64>,
    pub notes_loaded: bool,
}

impl Book {
    pub fn new(nb: NotebookSummary) -> Book {
        Book {
            nb,
            folders: Vec::new(),
            notes: Vec::new(),
            bodies: BTreeSet::new(),
            requested: BTreeSet::new(),
            notes_loaded: false,
        }
    }

    pub fn key(&self) -> NbKey {
        NbKey::of(&self.nb)
    }

    pub fn note(&self, id: u64) -> Option<&Note> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn folder(&self, id: u64) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }
}

/// Where the Notes list is: the notebook list, or inside a folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum NotesLoc {
    #[default]
    Books,
    Folder {
        nb: NbKey,
        folder: u64,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotesState {
    /// Every notebook on the ship, in the order the ship lists them.
    pub books: Vec<Book>,
    pub loc: NotesLoc,
    /// The start location has been chosen (once, on the first notebook load).
    pub loc_init: bool,
    /// The notebook opened most recently; where `n` creates from the list.
    pub last_nb: Option<NbKey>,
    pub sel: usize,
    /// Set by `n` so the created note opens in the editor.
    pub open_on_create: bool,
    /// The note a rename prompt is for.
    pub rename: Option<NoteRef>,
    /// Source of debounce tokens. Global so a timer for one editor can never
    /// match another editor's current token.
    pub token_seq: u64,
}

impl NotesState {
    pub fn book(&self, key: &NbKey) -> Option<&Book> {
        self.books.iter().find(|b| b.key() == *key)
    }

    pub fn book_mut(&mut self, key: &NbKey) -> Option<&mut Book> {
        self.books.iter_mut().find(|b| b.key() == *key)
    }

    /// Notes in all notebooks.
    pub fn note_count(&self) -> usize {
        self.books.iter().map(|b| b.notes.len()).sum()
    }
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
    pub nb: NbKey,
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
    /// Set when this window is one section's own app (`locus notes`,
    /// `/app/notes`). Switching to another section then asks the shell to
    /// open that section's window instead of switching in place.
    pub locked: Option<Section>,
    /// A cross-link waiting for its section's data to load.
    pub pending_target: Option<Target>,
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
            locked: None,
            pending_target: None,
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

    /// Lock this window to one section (`Some`), or show every section
    /// (`None`). A locked window starts on its section.
    pub fn focus_section(&mut self, section: Option<Section>) {
        self.locked = section;
        if let Some(s) = section {
            self.section = s;
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
