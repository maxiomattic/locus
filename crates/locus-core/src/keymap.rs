//! Framework-agnostic key handling. Shells translate their native key events
//! into [`Key`] and [`Mods`] and call [`keymap`].
//!
//! The apps are ordinary point-and-click GUIs; these are the usual desktop
//! shortcuts on top (arrows, Enter, Delete, F2, Ctrl+N, Ctrl+F, ...). Plain
//! letters never run commands: typing in a list starts filtering it. Browsers
//! keep Ctrl+N for themselves, so Alt+N is a synonym there.

use crate::msg::{Msg, Nav, TextEdit};
use crate::state::{App, Focus, Section};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
    };
    pub const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        logo: false,
    };
}

/// Translate one key press into the message it means in the current state.
/// `None` means the shell should handle the key itself (for example, typing
/// into the editor widget).
pub fn keymap(app: &App, key: Key, mods: Mods) -> Option<Msg> {
    if mods.logo {
        return None;
    }
    // A confirm dialog is modal: only its own answers apply.
    if app.focus != Focus::Confirm
        && let Some(msg) = command(app, key, mods)
    {
        return Some(msg);
    }
    if mods.ctrl || mods.alt {
        // Ctrl+C on a file row copies its path; in text it stays a copy.
        let copy_path = mods.ctrl && key == Key::Char('c') && app.section == Section::Files;
        return (copy_path && app.focus == Focus::List).then_some(Msg::Yank);
    }
    match app.focus {
        Focus::Editor => match key {
            Key::Escape => Some(Msg::Escape),
            _ => None,
        },
        Focus::Input(_) | Focus::Form | Focus::Palette => text_focus_key(app, key, mods),
        Focus::Confirm => match key {
            Key::Enter | Key::Char('y') => Some(Msg::ConfirmAnswer(true)),
            Key::Escape | Key::Char('n') => Some(Msg::ConfirmAnswer(false)),
            _ => None,
        },
        Focus::Help => match key {
            Key::Escape => Some(Msg::ToggleHelp),
            _ => None,
        },
        Focus::List | Focus::Detail => list_key(app, key),
    }
}

/// Shortcuts that work in every focus, including text fields.
fn command(app: &App, key: Key, mods: Mods) -> Option<Msg> {
    let section = app.section;
    let upload = matches!(section, Section::Photos | Section::Files);
    Some(match (key, mods.ctrl, mods.alt, mods.shift) {
        (Key::F(1), false, false, _) => Msg::ToggleHelp,
        (Key::F(5), false, false, _) => Msg::Reload,
        (Key::Char('n' | 'N'), true, false, _) | (Key::Char('n' | 'N'), false, true, _) => Msg::New,
        (Key::Char('f' | 'F'), true, false, _) => Msg::FocusFilter,
        (Key::Char('k' | 'K'), true, false, _) | (Key::Char('p' | 'P'), true, false, true) => Msg::OpenPalette,
        (Key::Char('r' | 'R'), true, false, _) => Msg::Reload,
        (Key::Char('s' | 'S'), true, false, _) => Msg::SaveNow,
        (Key::Char('o' | 'O'), true, false, _) if upload => Msg::Upload,
        (Key::Enter, true, false, _) => Msg::SaveForm,
        (Key::Left, false, true, _) if matches!(section, Section::Notes | Section::Files) => {
            Msg::Nav(Nav::Left)
        }
        _ => return None,
    })
}

/// Keys while a text field, form or palette has focus: plain characters are
/// text, and only Escape, Enter, Tab and arrows (where meaningful) are commands.
fn text_focus_key(app: &App, key: Key, mods: Mods) -> Option<Msg> {
    match key {
        Key::Escape => Some(Msg::Escape),
        Key::Enter => Some(Msg::Submit),
        Key::Tab if app.focus == Focus::Form => Some(if mods.shift {
            Msg::PrevField
        } else {
            Msg::NextField
        }),
        Key::Backspace => Some(Msg::Text(TextEdit::Backspace)),
        Key::Up if app.focus == Focus::Palette => Some(Msg::Nav(Nav::Up)),
        Key::Down if app.focus == Focus::Palette => Some(Msg::Nav(Nav::Down)),
        Key::Char(c) => Some(Msg::Text(TextEdit::Insert(c.to_string()))),
        _ => None,
    }
}

fn list_key(app: &App, key: Key) -> Option<Msg> {
    let section = app.section;
    let calendar = section == Section::Calendar;
    Some(match key {
        Key::Escape => Msg::Escape,
        Key::Enter => Msg::Activate,
        Key::Up => Msg::Nav(Nav::Up),
        Key::Down => Msg::Nav(Nav::Down),
        Key::Left => Msg::Nav(Nav::Left),
        Key::Right => Msg::Nav(Nav::Right),
        Key::Backspace if matches!(section, Section::Files | Section::Notes) => Msg::Nav(Nav::Left),
        Key::Home if calendar => Msg::Today,
        Key::PageUp if calendar => Msg::ShiftMonth(-1),
        Key::PageDown if calendar => Msg::ShiftMonth(1),
        Key::Home => Msg::Nav(Nav::Top),
        Key::End => Msg::Nav(Nav::Bottom),
        Key::PageUp => Msg::Nav(Nav::HalfUp),
        Key::PageDown => Msg::Nav(Nav::HalfDown),
        Key::Delete => Msg::Delete,
        Key::F(2) if matches!(section, Section::Calendar | Section::Contacts) => Msg::Edit,
        Key::F(2) => Msg::Rename,
        Key::Char(' ') if section == Section::Todo => Msg::ToggleTask,
        // Type-to-filter: the first letter opens the filter with that letter.
        Key::Char(c) if !c.is_whitespace() => Msg::Batch(vec![
            Msg::FocusFilter,
            Msg::Text(TextEdit::Insert(c.to_string())),
        ]),
        _ => return None,
    })
}

/// The bindings shown in the shortcuts dialog for `section`, as `(keys, what)`.
pub fn bindings(section: Section) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![
        ("Up / Down, Home / End", "move in the list"),
        ("Page Up / Page Down", "move a page"),
        ("Enter", "open"),
        ("Ctrl+N or Alt+N", "new"),
        ("Delete", "delete (asks first)"),
        ("Ctrl+F, or start typing", "filter the list"),
        ("Esc", "close, go back or clear the filter"),
        ("Ctrl+R or F5", "reload"),
        ("Ctrl+K", "command palette"),
        ("F1", "this list of shortcuts"),
    ];
    match section {
        Section::Notes => out.extend([
            ("Backspace or Alt+Left", "up to the parent folder"),
            ("F2", "rename the note"),
            ("Ctrl+S", "save now (notes also save as you type)"),
        ]),
        Section::Todo => out.extend([
            ("Space", "tick or untick the task"),
            ("Enter", "open the note the task is in"),
        ]),
        Section::Calendar => out.extend([
            ("Left / Right", "previous / next day"),
            ("Up / Down", "previous / next event of the day"),
            ("Page Up / Page Down", "previous / next month"),
            ("Home", "today"),
            ("F2", "edit the event"),
            ("Tab / Shift+Tab", "next / previous field"),
            ("Ctrl+Enter", "save the event"),
        ]),
        Section::Photos => out.extend([
            ("Arrows", "move in the grid; previous / next in the viewer"),
            ("Ctrl+O", "upload photos"),
        ]),
        Section::Files => out.extend([
            ("Backspace or Alt+Left", "parent folder"),
            ("F2", "rename"),
            ("Ctrl+O", "upload here"),
            ("Ctrl+C", "copy the file's path"),
        ]),
        Section::Contacts => out.extend([
            ("F2", "edit the contact"),
            ("Tab / Shift+Tab", "next / previous field"),
            ("Ctrl+Enter", "save the contact"),
        ]),
    }
    out
}
