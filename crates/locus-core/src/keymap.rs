//! Framework-agnostic key handling. Shells translate their native key events
//! into [`Key`] and [`Mods`] and call [`keymap`]. Plain characters only become
//! commands when no text field has focus; Escape, Ctrl-combos and Alt+digit
//! always are.
//!
//! Section switching and the palette have platform-neutral bindings (`g n`,
//! `:`, Alt+1..6) because browsers reserve Ctrl+1..6 for tab switching.

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
    if mods.alt && !mods.ctrl && !mods.logo {
        // Alt+1..6 switch sections everywhere, like Ctrl+1..6.
        return match key {
            Key::Char(c @ '1'..='6') => Some(Msg::Switch(Section::from_digit(c as u8 - b'0')?)),
            _ => None,
        };
    }
    if mods.alt || mods.logo {
        return None;
    }
    if mods.ctrl {
        if let Some(msg) = ctrl_command(key) {
            return Some(msg);
        }
        // Ctrl+D / Ctrl+U only scroll lists; everything else with Ctrl is inert.
        return match (key, app.focus) {
            (Key::Char('d'), Focus::List | Focus::Detail) => Some(Msg::Nav(Nav::HalfDown)),
            (Key::Char('u'), Focus::List | Focus::Detail) => Some(Msg::Nav(Nav::HalfUp)),
            _ => None,
        };
    }
    match app.focus {
        Focus::Editor => match key {
            Key::Escape => Some(Msg::Escape),
            _ => None,
        },
        Focus::Input(_) | Focus::Form | Focus::Palette => text_focus_key(app, key, mods),
        Focus::Confirm => match key {
            Key::Char('y') => Some(Msg::ConfirmAnswer(true)),
            Key::Char('n') | Key::Escape => Some(Msg::ConfirmAnswer(false)),
            _ => None,
        },
        Focus::Help => match key {
            Key::Escape | Key::Char('?') => Some(Msg::ToggleHelp),
            _ => None,
        },
        Focus::List | Focus::Detail => match list_key(app, key) {
            // An unbound key after `g` cancels the prefix, so a later `n`
            // does not still switch sections.
            None if app.pending_g => Some(Msg::ClearPrefix),
            other => other,
        },
    }
}

/// Ctrl-combos that work everywhere, including inside text fields.
fn ctrl_command(key: Key) -> Option<Msg> {
    Some(match key {
        Key::Char(c @ '1'..='6') => Msg::Switch(Section::from_digit(c as u8 - b'0')?),
        Key::Char('k') => Msg::OpenPalette,
        Key::Char('r') => Msg::Reload,
        Key::Char('s') => Msg::SaveNow,
        Key::Enter => Msg::SaveForm,
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

/// The letter after `g` that switches to each section, in sidebar order.
pub const GO_KEYS: [(char, Section); 6] = [
    ('n', Section::Notes),
    ('t', Section::Todo),
    ('c', Section::Calendar),
    ('p', Section::Photos),
    ('f', Section::Files),
    ('o', Section::Contacts),
];

/// The letter that follows `g` to switch to `section`.
pub fn go_key(section: Section) -> char {
    GO_KEYS
        .iter()
        .find(|(_, s)| *s == section)
        .map_or('?', |(c, _)| *c)
}

fn list_key(app: &App, key: Key) -> Option<Msg> {
    let section = app.section;
    if app.pending_g
        && let Key::Char(c) = key
        && let Some((_, s)) = GO_KEYS.iter().find(|(k, _)| *k == c)
    {
        return Some(Msg::Switch(*s));
    }
    Some(match key {
        Key::Escape => Msg::Escape,
        Key::Enter => Msg::Activate,
        Key::Up => Msg::Nav(Nav::Up),
        Key::Down => Msg::Nav(Nav::Down),
        Key::Left => Msg::Nav(Nav::Left),
        Key::Right => Msg::Nav(Nav::Right),
        Key::Backspace if section == Section::Files => Msg::Nav(Nav::Left),
        Key::Home => Msg::Nav(Nav::Top),
        Key::End => Msg::Nav(Nav::Bottom),
        Key::PageUp => Msg::Nav(Nav::HalfUp),
        Key::PageDown => Msg::Nav(Nav::HalfDown),
        Key::Char(c) => match c {
            'j' => Msg::Nav(Nav::Down),
            'k' => Msg::Nav(Nav::Up),
            'h' => Msg::Nav(Nav::Left),
            'l' => Msg::Nav(Nav::Right),
            'g' if app.pending_g => Msg::Nav(Nav::Top),
            'g' => Msg::GPrefix,
            'G' => Msg::Nav(Nav::Bottom),
            '/' => Msg::FocusFilter,
            ':' => Msg::OpenPalette,
            '?' => Msg::ToggleHelp,
            'i' | 'e' => Msg::Edit,
            'n' => Msg::New,
            'd' => Msg::Delete,
            'r' => Msg::Rename,
            'y' => Msg::Yank,
            'u' => Msg::Upload,
            'm' if section == Section::Calendar => Msg::CalendarMode,
            'm' if section == Section::Files => Msg::MoveFile,
            ' ' | 'x' if section == Section::Todo => Msg::ToggleTask,
            'z' if section == Section::Todo => Msg::ToggleDoneTasks,
            '[' if section == Section::Calendar => Msg::ShiftMonth(-1),
            ']' if section == Section::Calendar => Msg::ShiftMonth(1),
            't' if section == Section::Calendar => Msg::Today,
            _ => return None,
        },
        _ => return None,
    })
}

/// The bindings shown in the help overlay for `section`, as `(keys, what)`.
pub fn bindings(section: Section) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![
        ("g n/t/c/p/f/o", "go to Notes/Todo/Calendar/Photos/Files/Contacts"),
        ("Alt+1..6, Ctrl+1..6", "switch section"),
        (": or Ctrl+K", "command palette"),
        ("/", "filter this list"),
        ("Esc", "close / leave / clear filter"),
        ("Ctrl+R", "reload section"),
        ("?", "toggle this help"),
        ("j/k, Up/Down", "move"),
        ("gg / G", "top / bottom"),
        ("Ctrl+D / Ctrl+U, PgDn / PgUp", "half page down / up"),
        ("Home / End", "top / bottom"),
        ("n", "new"),
        ("d", "delete (asks first)"),
    ];
    match section {
        Section::Notes => out.extend([
            ("Enter / i", "open in editor"),
            ("r", "rename"),
            ("Ctrl+S", "save now"),
        ]),
        Section::Todo => out.extend([
            ("Space / x", "toggle task"),
            ("n", "add task to Inbox"),
            ("Enter", "open source note"),
            ("z", "show / hide Done"),
        ]),
        Section::Calendar => out.extend([
            ("h / l", "previous / next day"),
            ("j / k", "previous / next event of the day"),
            ("[ / ]", "previous / next month"),
            ("t", "today"),
            ("m", "agenda / month"),
            ("e", "edit event"),
            ("Tab / Shift+Tab", "next / previous field (form)"),
            ("Ctrl+Enter", "save event (form)"),
        ]),
        Section::Photos => out.extend([
            ("h/j/k/l", "move in grid"),
            ("Enter", "open viewer"),
            ("h / l", "previous / next (viewer)"),
            ("u", "upload photos"),
        ]),
        Section::Files => out.extend([
            ("Enter / l", "open folder or file"),
            ("h / Backspace", "parent folder"),
            ("u", "upload here"),
            ("r", "rename"),
            ("m", "move to directory"),
            ("y", "copy path"),
        ]),
        Section::Contacts => out.extend([
            ("Enter", "open detail"),
            ("e", "edit contact"),
            ("Tab / Shift+Tab", "next / previous field (form)"),
            ("Ctrl+Enter", "save contact (form)"),
        ]),
    }
    out
}
