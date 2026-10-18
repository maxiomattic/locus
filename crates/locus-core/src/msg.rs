//! The two halves of the functional core's contract: [`Msg`] comes in (user
//! intents and IO results), [`Effect`] goes out (IO the shell must perform).

use locus_client::{
    calendar::{Event, EventPatch},
    contacts::{Contact, ContactPatch},
    messages::{Conversation, Message},
    notes::{Folder, Note, NoteRevision, NotebookSummary},
    photos::Photo,
};
use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::state::{NbKey, NoteRef, Section, Target};

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
    /// Enter on the focused list row: open a note, enter a folder, open viewer.
    Activate,
    /// `i` in Notes, `e` in Calendar/Contacts.
    Edit,
    New,
    /// A folder inside the notebook folder being browsed (Notes).
    NewFolder,
    /// A new notebook (Notes).
    NewNotebook,
    Delete,
    Rename,
    /// `y`: copy the selected file's path.
    Yank,
    Upload,
    /// Move the selected file (Files) or note (Notes) to another folder.
    Move,
    /// Show or hide the saved revisions of the selected or open note.
    History,
    /// Mouse selection of a revision in the note history.
    SelectRevision(usize),
    /// Restore the selected revision (asks first).
    RestoreRevision,
    /// Internal target of a restore confirmation.
    ExecuteRestore {
        note: NoteRef,
        rev: u64,
    },
    /// Internal target of the confirmation to move a note into another
    /// notebook (`folder` of notebook `to`).
    ExecuteMoveNote {
        note: NoteRef,
        to: NbKey,
        folder: u64,
    },
    /// Space or `x` in Todo.
    ToggleTask,
    /// Space in Movies, Music and the Photos viewer: play or pause.
    TogglePlay,
    /// Next and Previous in the player, for the list it was started from.
    NextTrack,
    PrevTrack,
    /// The player's position slider, in milliseconds.
    SeekTo(u64),
    /// The volume slider, 0 to 100.
    SetVolume(u8),
    /// Stop: close the player.
    StopPlayback,
    /// Messages: the draft field was clicked, so typing goes there.
    BeginDraft,
    /// Messages: send the draft to the open conversation (Enter or Send).
    SendDraft,
    /// Messages: re-read the conversations and the open thread, quietly. The
    /// shell sends this on a timer while Messages is open.
    Poll,
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
    /// Internal target of a folder delete confirmation (Notes).
    ExecuteDeleteFolder {
        nb: NbKey,
        id: u64,
    },
    /// Internal target of a notebook delete confirmation.
    ExecuteDeleteNotebook(NbKey),
    /// Internal target of an empty folder delete confirmation (Files).
    ExecuteDeleteDir {
        path: String,
    },
    /// Internal target of a message delete confirmation (Messages).
    ExecuteDeleteMessage {
        id: u64,
    },
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
    /// A folder was created; `folders` is the notebook's new folder list.
    NoteFolderCreated {
        nb: NotebookSummary,
        folders: Vec<Folder>,
    },
    /// A folder was renamed or moved; `folders` is the new folder list.
    NoteFolderUpdated {
        nb: NotebookSummary,
        folders: Vec<Folder>,
        moved: bool,
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
    NoteMoved {
        nb: NotebookSummary,
        id: u64,
        folder: u64,
    },
    /// Note `id` of `from` was copied to `to` as `note`. `deleted` says
    /// whether the original then went; on `Err` the note is in both. The
    /// note is boxed to keep `Msg` small.
    NoteMovedAcross {
        from: NotebookSummary,
        id: u64,
        to: NotebookSummary,
        note: Box<Note>,
        deleted: Result<(), String>,
    },
    /// Reading the note or creating the copy failed: nothing changed.
    NoteMoveFailed {
        error: String,
    },
    /// A folder and everything in it is gone.
    NoteFolderDeleted {
        nb: NotebookSummary,
        id: u64,
    },
    NotebookDeleted {
        nb: NotebookSummary,
    },
    NotebookRenamed {
        nb: NotebookSummary,
        title: String,
    },
    /// Revision `rev` is the note's text again; `note` is the re-read note.
    NoteRestored {
        nb: NotebookSummary,
        note: Note,
        rev: u64,
    },
    /// A note's saved revisions, newest first, or why they could not load.
    HistoryLoaded {
        nb: NotebookSummary,
        id: u64,
        revs: Result<Vec<NoteRevision>, String>,
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
    PhotoDeleted {
        id: u64,
    },
    PhotoPatched {
        photo: Photo,
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
    DirDeleted {
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
    /// The player's position and length, as the shell reads them. A zero
    /// `duration_ms` means the length is not known yet.
    PlayerTick {
        position_ms: u64,
        duration_ms: u64,
        playing: bool,
    },
    /// The file played to its end.
    PlayerEnded,
    /// The file could not be played.
    PlayerFailed(String),
    ConversationsLoaded {
        conversations: Vec<Conversation>,
    },
    ThreadLoaded {
        peer: String,
        messages: Vec<Message>,
    },
    /// The peer's agent has the message (or the send failed); the thread and
    /// the conversation list are read again.
    MessageSent {
        peer: String,
    },
    MessageDeleted {
        id: u64,
    },
    /// The ship did not take the message: the text goes back to the draft.
    SendFailed {
        peer: String,
        body: String,
        error: String,
    },
    /// A folder moved from `from` to `to` (a rename is a move too).
    DirMoved {
        from: String,
        to: String,
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

/// What the shell's player should do. Load starts a file; the other commands
/// act on the file loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerCmd {
    Load {
        sha256: String,
        name: String,
        mime: String,
        /// Show the picture (a movie or a video photo) as well as playing sound.
        video: bool,
        autoplay: bool,
    },
    Pause,
    Resume,
    /// Jump to this position, in milliseconds.
    Seek(u64),
    /// 0 to 100.
    Volume(u8),
    Stop,
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
    MoveNote {
        nb: NotebookSummary,
        id: u64,
        folder: u64,
    },
    /// Move note `id` of `from` into `folder` of another notebook: read it,
    /// create it there with the same title and body, then delete the
    /// original. Answered by [`Msg::NoteMovedAcross`] or [`Msg::NoteMoveFailed`].
    MoveNoteAcross {
        from: NotebookSummary,
        id: u64,
        to: NotebookSummary,
        folder: u64,
    },
    /// Delete a folder with everything in it.
    DeleteNoteFolder {
        nb: NotebookSummary,
        id: u64,
    },
    DeleteNotebook {
        nb: NotebookSummary,
    },
    LoadHistory {
        nb: NotebookSummary,
        id: u64,
    },
    RenameNotebook {
        nb: NotebookSummary,
        title: String,
    },
    RestoreNote {
        nb: NotebookSummary,
        id: u64,
        rev: u64,
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
    DeletePhoto {
        id: u64,
    },
    /// Change a photo's title and/or album; `None` keeps the field.
    PatchPhoto {
        id: u64,
        expected_revision: u64,
        title: Option<String>,
        album: Option<String>,
    },
    CreateNoteFolder {
        nb: NotebookSummary,
        parent: u64,
        name: String,
    },
    /// Rename a folder (`name`) or move it under `parent`.
    UpdateNoteFolder {
        nb: NotebookSummary,
        id: u64,
        name: Option<String>,
        parent: Option<u64>,
    },
    DeleteContact {
        id: u64,
    },

    LoadFiles,
    LoadDirs,
    /// Rename or move a folder, with everything in it. `from` and `to` are full paths.
    MoveDir {
        from: String,
        to: String,
    },
    CreateDir {
        path: String,
    },
    DeleteDir {
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

    /// Drive the player (shell-handled, like `OpenBlob`): no result message.
    /// The shell reports position back with [`Msg::PlayerTick`].
    Playback(PlayerCmd),

    LoadConversations,
    LoadThread {
        peer: String,
    },
    /// Send `body` to the ship `to`. Answered by [`Msg::MessageSent`] or [`Msg::SendFailed`].
    SendMessage {
        to: String,
        body: String,
    },
    MarkRead {
        peer: String,
    },
    DeleteMessage {
        id: u64,
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
