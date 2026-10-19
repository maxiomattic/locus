//! Framework-agnostic core of the Locus apps.
//!
//! The shape is functional core, imperative shell:
//!
//! - [`App`] is the whole UI state.
//! - [`update`] is a pure transition: `(App, Msg, Now) -> Vec<Effect>`.
//!   Every user intent and every IO result is a [`Msg`]; every IO request is an
//!   [`Effect`] the shell performs, then feeds back as a `Msg`.
//! - [`keymap`] turns framework-neutral [`Key`] and [`Mods`] into a [`Msg`].
//! - [`view_model`] returns plain data for drawing. Shells hold no logic.
//!
//! The crate does no IO, spawns nothing, and has no UI or async runtime
//! dependency. The caller supplies the clock through [`Now`].

pub mod date;
pub mod keymap;
pub mod media;
pub mod messages;
pub mod msg;
pub mod palette;
pub mod state;
pub mod view_model;

mod calendar;
mod contacts;
mod files;
mod notes;
mod photos;
mod todo;
mod update;

pub use date::Date;
pub use keymap::{Key, Mods, bindings, keymap};
pub use locus_client;
pub use msg::{Effect, FileEntry, FilePatch, Msg, Nav, Now, PlayerCmd, TextEdit};
pub use notes::left_in_both;
pub use state::{
    App, CalMode, Focus, InputKind, Level, NbKey, NoteRef, NotesLoc, Playback, Section, Target,
};
pub use todo::is_inbox_title;
pub use update::update;
