//! Shared helpers for the behaviour tests: a fixed clock, fixture builders,
//! and a `press` that runs the real keymap and then `update`.
#![allow(dead_code)]

use locus_core::date::{Date, local_to_unix};
use locus_core::locus_client::calendar::Event;
use locus_core::locus_client::notes::{Note, Notebook, NotebookSummary};
use locus_core::locus_client::photos::Photo;
pub use locus_core::{App, Effect, Key, Mods, Msg, Now, Section, keymap, update};

/// 2026-10-11 (a Sunday), 12:00 UTC, offset 0.
pub fn now() -> Now {
    now_at(Date::new(2026, 10, 11).unwrap(), 0)
}

pub fn now_at(today: Date, tz_offset_secs: i32) -> Now {
    Now {
        unix_secs: local_to_unix(today, 12, 0, tz_offset_secs),
        today,
        tz_offset_secs,
    }
}

pub fn date(y: i32, m: u8, d: u8) -> Date {
    Date::new(y, m, d).expect("valid test date")
}

pub fn nb() -> NotebookSummary {
    NotebookSummary {
        host: "~zod".into(),
        flag_name: "notes".into(),
        notebook: Notebook {
            id: 1,
            title: "Locus".into(),
            root_folder_id: 10,
        },
        visibility: "private".into(),
    }
}

pub fn note(id: u64, title: &str, body: &str, revision: u64, updated_at: u64) -> Note {
    Note {
        id,
        notebook_id: 1,
        folder_id: 10,
        title: title.into(),
        slug: None,
        body_md: body.into(),
        revision,
        updated_at,
    }
}

pub fn event(id: u64, title: &str, start: u64, end: u64, revision: u64) -> Event {
    Event {
        id,
        calendar: "default".into(),
        title: title.into(),
        start,
        end,
        body: String::new(),
        revision,
    }
}

pub fn photo(id: u64, title: &str, path: &str) -> Photo {
    Photo {
        id,
        album: "Holidays".into(),
        title: title.into(),
        path: path.into(),
        tags: vec!["sun".into()],
        revision: 1,
    }
}

pub fn file(id: u64, dir: &str, name: &str, revision: u64) -> locus_core::FileEntry {
    locus_core::FileEntry {
        id,
        dir: dir.into(),
        name: name.into(),
        sha256: format!("{:064x}", id),
        size: 2048,
        mime: "text/plain".into(),
        modified: 0,
        revision,
    }
}

pub fn contact(
    id: u64,
    name: &str,
    email: &str,
    ship: &str,
) -> locus_core::locus_client::contacts::Contact {
    locus_core::locus_client::contacts::Contact {
        id,
        name: name.into(),
        email: email.into(),
        ship: ship.into(),
        revision: 1,
    }
}

/// Run one message against `app` with the default clock.
pub fn send(app: &mut App, msg: Msg) -> Vec<Effect> {
    update(app, msg, now())
}

pub fn send_at(app: &mut App, msg: Msg, clock: Now) -> Vec<Effect> {
    update(app, msg, clock)
}

/// Translate a key through the keymap, then update. Keys the keymap ignores
/// produce no effects and leave the state alone.
pub fn press_with(app: &mut App, key: Key, mods: Mods, clock: Now) -> Vec<Effect> {
    match keymap(app, key, mods) {
        Some(msg) => update(app, msg, clock),
        None => Vec::new(),
    }
}

pub fn press(app: &mut App, key: Key) -> Vec<Effect> {
    press_with(app, key, Mods::NONE, now())
}

pub fn ctrl(app: &mut App, c: char) -> Vec<Effect> {
    press_with(app, Key::Char(c), Mods::CTRL, now())
}

/// Type each character of `s` into whatever has focus.
pub fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, Key::Char(c));
    }
}

/// An app that has its notebook and notes loaded, with the Notes section active.
pub fn loaded_notes(notes: Vec<Note>) -> App {
    let mut app = App::new();
    send(&mut app, Msg::Reload);
    send(
        &mut app,
        Msg::NotebooksLoaded {
            notebooks: vec![nb()],
        },
    );
    send(&mut app, Msg::NotesLoaded { nb: nb(), notes });
    app
}

pub fn has_effect(fx: &[Effect], want: impl Fn(&Effect) -> bool) -> bool {
    fx.iter().any(want)
}

pub fn save_notes(fx: &[Effect]) -> Vec<&Effect> {
    fx.iter()
        .filter(|e| matches!(e, Effect::SaveNote { .. }))
        .collect()
}

pub fn section_of(app: &App) -> Section {
    app.section
}

/// The full cross-link to note `id` of the fixture notebook.
pub fn note_target(id: u64) -> locus_core::Target {
    locus_core::Target::NoteAt(locus_core::NoteRef {
        nb: locus_core::NbKey::of(&nb()),
        id,
    })
}

/// A second notebook on the same ship.
pub fn nb_journal() -> NotebookSummary {
    NotebookSummary {
        host: "~zod".into(),
        flag_name: "journal".into(),
        notebook: Notebook {
            id: 2,
            title: "Journal".into(),
            root_folder_id: 20,
        },
        visibility: "private".into(),
    }
}

pub fn folder(id: u64, name: &str, parent: Option<u64>) -> locus_core::locus_client::notes::Folder {
    locus_core::locus_client::notes::Folder {
        id,
        notebook_id: 0,
        name: name.into(),
        parent_folder_id: parent,
    }
}

/// A note in `folder_id` (ids are per notebook, so callers may reuse them).
pub fn note_in(id: u64, folder_id: u64, title: &str, body: &str, updated_at: u64) -> Note {
    Note {
        folder_id,
        ..note(id, title, body, 1, updated_at)
    }
}
