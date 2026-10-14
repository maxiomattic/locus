//! The two halves of the functional core's contract: [`Msg`] comes in (user
//! intents and IO results), [`Effect`] goes out (IO the shell must perform).

use locus_client::{
    calendar::{Event, EventPatch},
    contacts::{Contact, ContactPatch},
    notes::{Folder, Note, NotebookSummary},
    photos::Photo,
};
use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::state::{NoteRef, Section, Target};

/// The clock, passed into every [`crate::update`] call so the core never reads
/// time itself. `tz_offset_secs` follows the convention `local = utc + offset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub unix_secs: i64,
    pub today: Date,
    pub tz_offset_secs: i32,
}

/// A file in the Files section. `locus-client` has no files module yet, so the
/// shell maps its own wire type into this one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: u64,
    /// Absolute directory, e.g. `/` or `/Photos/2026`.
    pub dir: String,
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    /// Unix seconds.
    pub modified: u64,
    pub revision: u64,
}

/// Fields to change on a file. `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilePatch {
    pub name: Option<String>,
    pub dir: Option<String>,
}

/// Cursor movement understood by every list, grid and viewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    HalfDown,
    HalfUp,
}

/// Edits to the text buffer of the focused input, form field or palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEdit {
    Insert(String),
    Backspace,
    /// Replace the whole value. Shells with a real text widget send this, so
    /// the widget owns cursor movement and selection.
    Set(String),
}

/// Everything that can happen to the core. Intents come from the keymap or the
/// shell's widgets; results come back from [`Effect`]s the shell executed.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    // ----- intents -----
    Switch(Section),
    Reload,
    Nav(Nav),
    /// First `g` of `gg` or `g n`, `g t`, .... Sets the pending flag; the next
    /// key completes the sequence.
    GPrefix,
    /// A key after `g` that completes no sequence. Only clears the flag.
    ClearPrefix,
    /// Enter on the focused list row: open a note, enter a folder, open viewer.
    Activate,
    /// `i` in Notes, `e` in Calendar/Contacts.
    Edit,
    New,
    Delete,
    Rename,
    /// `y`: copy the selected file's path.
    Yank,
    Upload,
    /// `m` in Files: move the selected file to another directory.
    MoveFile,
    /// Space or `x` in Todo.
    ToggleTask,
    /// `z` in Todo: show or hide the Done group.
    ToggleDoneTasks,
    /// `m` in Calendar: switch Agenda and Month.
    CalendarMode,
    /// `[` / `]` in Calendar.
    ShiftMonth(i32),
    /// `t` in Calendar.
    Today,
    FocusFilter,
    ToggleHelp,
    OpenPalette,
    Escape,
    /// Enter in an input, form or palette.
    Submit,
    /// Ctrl+Enter: save the open form from any field.
    SaveForm,
    NextField,
    PrevField,
    Text(TextEdit),
    /// `y` / `n` in a confirm prompt.
    ConfirmAnswer(bool),
    /// Ctrl+S.
    SaveNow,
    /// The shell's editor changed; carries the full body text.
    EditorChanged(String),
    /// Debounce timer fired (see [`Effect::ScheduleSave`]).
    SaveTimer(u64),
    /// Shell reports its photo grid width.
    Layout {
        photo_columns: usize,
    },
    /// Mouse selection: set the current section's list cursor to this index
    /// (clamped). Calendar uses the index into the events of the selected day.
    Select(usize),
    /// The shell's editor was clicked while the core had focus elsewhere.
    /// Moves focus to the open note's editor, if one is open.
    EditorFocus,
    /// Mouse selection of a day in the calendar month grid.
    SelectDay(crate::date::Date),
    /// Open a section and then `target` in it. In a window locked to
    /// another section this asks the shell to open that section's window
    /// ([`Effect::OpenSection`]). Shells send it for incoming cross-links.
    Open {
        section: Section,
        target: Option<Target>,
    },
    /// Switch to a section and select the item with this id.
    Goto {
        section: Section,
        id: u64,
    },
    /// Run several messages in order (used by palette entries).
    Batch(Vec<Msg>),
    /// Internal target of a delete confirmation.
    ExecuteDelete {
        section: Section,
        id: u64,
    },
    /// Internal target of a note delete confirmation.
    ExecuteDeleteNote(NoteRef),
    BeginLogin,

    // ----- IO results -----
    NotebooksLoaded {
        notebooks: Vec<NotebookSummary>,
    },
    NotebookCreated {
        notebook: NotebookSummary,
    },
    NotesLoaded {
        nb: NotebookSummary,
        notes: Vec<Note>,
    },
    FoldersLoaded {
        nb: NotebookSummary,
        folders: Vec<Folder>,
    },
    NoteLoaded {
        nb: NotebookSummary,
        note: Note,
    },
    NoteCreated {
        nb: NotebookSummary,
        note: Note,
    },
    /// The shell re-reads the note after a successful save, so `note` carries
    /// the server's new revision and body.
    NoteSaved {
        nb: NotebookSummary,
        note: Note,
    },
    NoteRenamed {
        nb: NotebookSummary,
        id: u64,
        title: String,
    },
    NoteDeleted {
        nb: NotebookSummary,
        id: u64,
    },
    /// Stale `expected_revision`. `body` is what we tried to write; `current`
    /// is the server revision when the shell could re-read it.
    SaveConflict {
        nb: NotebookSummary,
        id: u64,
        body: String,
        current: Option<u64>,
    },
    SaveFailed {
        nb: NotebookSummary,
        id: u64,
        error: String,
    },
    EventsLoaded {
        events: Vec<Event>,
    },
    EventSaved {
        event: Event,
    },
    EventDeleted {
        id: u64,
    },
    PhotosLoaded {
        photos: Vec<Photo>,
    },
    ContactsLoaded {
        contacts: Vec<Contact>,
    },
    ContactSaved {
        contact: Contact,
    },
    ContactDeleted {
        id: u64,
    },
    FilesLoaded {
        files: Vec<FileEntry>,
    },
    DirsLoaded {
        dirs: Vec<String>,
    },
    DirCreated {
        path: String,
    },
    FilePatched {
        file: FileEntry,
    },
    FileDeleted {
        id: u64,
    },
    /// The picker closed. `count` is 0 when the user cancelled.
    UploadDone {
        dir: String,
        count: usize,
    },
    LoggedIn,
    LoginFailed {
        error: String,
    },
    LoadFailed {
        section: Section,
        error: String,
    },
    WriteFailed {
        section: Section,
        error: String,
    },
}

/// IO the shell must perform. Each request's outcome is fed back as the
/// matching [`Msg`] variant.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LoadNotebooks,
    CreateNotebook {
        title: String,
    },
    LoadNotes {
        nb: NotebookSummary,
    },
    LoadFolders {
        nb: NotebookSummary,
    },
    LoadNote {
        nb: NotebookSummary,
        id: u64,
    },
    SaveNote {
        nb: NotebookSummary,
        id: u64,
        body: String,
        expected_revision: u64,
    },
    CreateNote {
        nb: NotebookSummary,
        folder: u64,
        title: String,
        body: String,
    },
    RenameNote {
        nb: NotebookSummary,
        id: u64,
        title: String,
    },
    DeleteNote {
        nb: NotebookSummary,
        id: u64,
    },

    LoadEvents,
    CreateEvent {
        title: String,
        start: u64,
        end: u64,
        body: String,
    },
    PatchEvent {
        id: u64,
        expected_revision: u64,
        patch: EventPatch,
    },
    DeleteEvent {
        id: u64,
    },

    LoadPhotos,
    /// The shell opens its own file picker and uploads into `dir`.
    UploadFiles {
        dir: String,
    },

    LoadContacts,
    CreateContact {
        name: String,
        email: String,
        ship: String,
    },
    PatchContact {
        id: u64,
        expected_revision: u64,
        patch: ContactPatch,
    },
    DeleteContact {
        id: u64,
    },

    LoadFiles,
    LoadDirs,
    CreateDir {
        path: String,
    },
    PatchFile {
        id: u64,
        expected_revision: u64,
        patch: FilePatch,
    },
    DeleteFile {
        id: u64,
    },
    /// Show or download the blob with this content hash.
    OpenBlob {
        sha256: String,
        name: String,
        mime: String,
    },

    /// Start (or restart) a one-shot timer. Send `Msg::SaveTimer(token)` when it fires.
    ScheduleSave {
        after_ms: u32,
        token: u64,
    },
    CopyToClipboard(String),
    /// Open or focus the window (desktop) or tab (web) that is `section`'s
    /// own app, and show `target` there. Shell-handled; no result message.
    OpenSection(Section, Option<Target>),
    Login {
        code: String,
    },
}
