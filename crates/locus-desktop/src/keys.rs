//! Translate iced keyboard events into the framework-neutral types of locus-core.

use cosmic::iced::keyboard::{self, key::Named};
use locus_core::{Key, Mods};

pub fn key(key: &keyboard::Key) -> Option<Key> {
    Some(match key.as_ref() {
        keyboard::Key::Character(s) => {
            let mut chars = s.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Char(c)
        }
        keyboard::Key::Named(named) => match named {
            Named::Enter => Key::Enter,
            Named::Escape => Key::Escape,
            Named::Tab => Key::Tab,
            Named::Backspace => Key::Backspace,
            Named::Delete => Key::Delete,
            Named::ArrowUp => Key::Up,
            Named::ArrowDown => Key::Down,
            Named::ArrowLeft => Key::Left,
            Named::ArrowRight => Key::Right,
            Named::Home => Key::Home,
            Named::End => Key::End,
            Named::PageUp => Key::PageUp,
            Named::PageDown => Key::PageDown,
            Named::F1 => Key::F(1),
            _ => return None,
        },
        _ => return None,
    })
}

pub fn mods(m: keyboard::Modifiers) -> Mods {
    Mods {
        ctrl: m.control(),
        alt: m.alt(),
        shift: m.shift(),
        logo: m.logo(),
    }
}
