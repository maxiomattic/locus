//! Modal editing for the notes source and rich-text panes.

use cosmic::iced::keyboard::{key::Named, Key, Modifiers};
use cosmic::iced::widget::text_editor::{Action, Edit, Motion};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
    VisualBlock,
    Command,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
            Self::VisualLine => "V-LINE",
            Self::VisualBlock => "V-BLOCK",
            Self::Command => "COMMAND",
        }
    }

    pub fn block_caret(self) -> bool {
        !matches!(self, Self::Insert)
    }

    pub fn paints_selection(self) -> bool {
        matches!(self, Self::Visual | Self::VisualLine | Self::VisualBlock)
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub enabled: bool,
    pub mode: Mode,
    pub pending: String,
}

impl State {
    pub fn load() -> Self {
        let enabled = std::fs::read_to_string(path())
            .ok()
            .is_some_and(|s| s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"));
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub fn persist(&self) {
        let path = path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, if self.enabled { "1" } else { "0" });
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.mode = Mode::Normal;
        self.pending.clear();
        self.persist();
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if !matches!(mode, Mode::Command) {
            self.pending.clear();
        }
    }

    pub fn keys(&self) -> String {
        if matches!(self.mode, Mode::Command) {
            format!(":{}", self.pending)
        } else {
            self.pending.clone()
        }
    }
}

fn path() -> std::path::PathBuf {
    notes_sync::data_dir().join("vim")
}

#[derive(Debug, Clone)]
pub enum Effect {
    Mode(Mode),
    Action(Action),
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Run(String),
    Keys,
}

pub fn handle(
    state: &mut State,
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
) -> Vec<Effect> {
    if !state.enabled {
        return Vec::new();
    }
    if matches!(state.mode, Mode::Normal)
        && modifiers.control()
        && !modifiers.shift()
        && !modifiers.alt()
    {
        if let Key::Character(c) = key {
            if c.as_str().eq_ignore_ascii_case("v") {
                return mode(state, Mode::VisualBlock);
            }
        }
    }
    if modifiers.command() || modifiers.alt() {
        return Vec::new();
    }
    match state.mode {
        Mode::Insert => insert(state, key),
        Mode::Normal => normal(state, key, modifiers, text),
        Mode::Visual | Mode::VisualLine | Mode::VisualBlock => visual(state, key, text),
        Mode::Command => command(state, key, text),
    }
}

fn insert(state: &mut State, key: &Key) -> Vec<Effect> {
    if matches!(key, Key::Named(Named::Escape)) {
        return mode(state, Mode::Normal);
    }
    Vec::new()
}

fn command(state: &mut State, key: &Key, text: Option<&str>) -> Vec<Effect> {
    match key {
        Key::Named(Named::Escape) => mode(state, Mode::Normal),
        Key::Named(Named::Enter) => {
            let cmd = std::mem::take(&mut state.pending);
            state.mode = Mode::Normal;
            vec![Effect::Run(cmd), Effect::Mode(Mode::Normal), Effect::Keys]
        }
        Key::Named(Named::Backspace) => {
            state.pending.pop();
            vec![Effect::Keys]
        }
        _ => {
            if let Some(t) = text {
                for ch in t.chars() {
                    if !ch.is_control() {
                        state.pending.push(ch);
                    }
                }
                vec![Effect::Keys]
            } else {
                Vec::new()
            }
        }
    }
}

fn normal(
    state: &mut State,
    key: &Key,
    modifiers: Modifiers,
    text: Option<&str>,
) -> Vec<Effect> {
    if matches!(key, Key::Named(Named::Escape)) {
        state.pending.clear();
        return vec![Effect::Mode(Mode::Normal), Effect::Keys];
    }
    let ch = match key {
        Key::Character(c) => c.chars().next(),
        Key::Named(Named::Enter) => Some('\n'),
        _ => text.and_then(|t| t.chars().next()),
    };
    let Some(ch) = ch else {
        return Vec::new();
    };
    if ch.is_ascii_digit() && (ch != '0' || !state.pending.is_empty()) {
        if state.pending.chars().all(|c| c.is_ascii_digit()) {
            if state.pending.len() < 3 {
                state.pending.push(ch);
            }
            return vec![Effect::Keys];
        }
    }
    if ch == ':' && state.pending.is_empty() {
        state.mode = Mode::Command;
        state.pending.clear();
        return vec![Effect::Mode(Mode::Command), Effect::Keys];
    }
    if !state.pending.is_empty() {
        let (count, op) = split_count(&state.pending);
        if op.is_empty() {
            return finish_motion(state, count, ch, modifiers.shift());
        }
        state.pending.push(ch);
        return chord(state);
    }
    finish_motion(state, 1, ch, modifiers.shift())
}

fn finish_motion(state: &mut State, count: usize, ch: char, shift: bool) -> Vec<Effect> {
    state.pending.clear();
    let count = count.clamp(1, 99);
    match ch {
        'h' => times(count, Action::Move(Motion::Left)),
        'j' => times(count, Action::Move(Motion::Down)),
        'k' => times(count, Action::Move(Motion::Up)),
        'l' => times(count, Action::Move(Motion::Right)),
        'w' => times(count, Action::Move(Motion::WordRight)),
        'b' => times(count, Action::Move(Motion::WordLeft)),
        '0' => act(Action::Move(Motion::Home)),
        '$' => act(Action::Move(Motion::End)),
        'g' => {
            state.pending = if count == 1 {
                "g".into()
            } else {
                format!("{count}g")
            };
            vec![Effect::Keys]
        }
        'G' => act(Action::Move(Motion::DocumentEnd)),
        'i' => mode(state, Mode::Insert),
        'I' => {
            state.set_mode(Mode::Insert);
            vec![
                Effect::Action(Action::Move(Motion::Home)),
                Effect::Mode(Mode::Insert),
            ]
        }
        'a' => {
            state.set_mode(Mode::Insert);
            vec![
                Effect::Action(Action::Move(Motion::Right)),
                Effect::Mode(Mode::Insert),
            ]
        }
        'A' => {
            state.set_mode(Mode::Insert);
            vec![
                Effect::Action(Action::Move(Motion::End)),
                Effect::Mode(Mode::Insert),
            ]
        }
        'o' => {
            state.set_mode(Mode::Insert);
            vec![
                Effect::Action(Action::Move(Motion::End)),
                Effect::Action(Action::Edit(Edit::Enter)),
                Effect::Mode(Mode::Insert),
            ]
        }
        'O' => {
            state.set_mode(Mode::Insert);
            vec![
                Effect::Action(Action::Move(Motion::Home)),
                Effect::Action(Action::Edit(Edit::Enter)),
                Effect::Action(Action::Move(Motion::Up)),
                Effect::Mode(Mode::Insert),
            ]
        }
        'x' => times(count, Action::Edit(Edit::Delete)),
        'd' | 'y' => {
            state.pending = if count == 1 {
                ch.to_string()
            } else {
                format!("{count}{ch}")
            };
            vec![Effect::Keys]
        }
        'p' | 'P' => vec![Effect::Paste],
        'u' => vec![Effect::Undo],
        'r' if shift => vec![Effect::Redo],
        'v' if shift => mode(state, Mode::VisualLine),
        'v' => mode(state, Mode::Visual),
        _ => {
            state.pending.clear();
            vec![Effect::Keys]
        }
    }
}

fn visual(state: &mut State, key: &Key, text: Option<&str>) -> Vec<Effect> {
    if matches!(key, Key::Named(Named::Escape)) {
        return mode(state, Mode::Normal);
    }
    let ch = match key {
        Key::Character(c) => c.chars().next(),
        _ => text.and_then(|t| t.chars().next()),
    };
    let Some(ch) = ch else {
        return Vec::new();
    };
    if ch.is_ascii_digit() && (ch != '0' || !state.pending.is_empty()) {
        if state.pending.chars().all(|c| c.is_ascii_digit()) && state.pending.len() < 3 {
            state.pending.push(ch);
            return vec![Effect::Keys];
        }
    }
    let (count, _) = split_count(&state.pending);
    state.pending.clear();
    let count = count.clamp(1, 99);
    match ch {
        'h' => times(count, Action::Select(Motion::Left)),
        'j' => times(count, Action::Select(Motion::Down)),
        'k' => times(count, Action::Select(Motion::Up)),
        'l' => times(count, Action::Select(Motion::Right)),
        'w' => times(count, Action::Select(Motion::WordRight)),
        'b' => times(count, Action::Select(Motion::WordLeft)),
        '0' => act(Action::Select(Motion::Home)),
        '$' => act(Action::Select(Motion::End)),
        'x' | 'd' => {
            state.set_mode(Mode::Normal);
            vec![Effect::Cut, Effect::Mode(Mode::Normal)]
        }
        'y' => {
            state.set_mode(Mode::Normal);
            vec![Effect::Copy, Effect::Mode(Mode::Normal)]
        }
        _ => vec![Effect::Keys],
    }
}

fn chord(state: &mut State) -> Vec<Effect> {
    let pending = std::mem::take(&mut state.pending);
    let (count, rest) = split_count(&pending);
    let count = count.clamp(1, 99);
    match rest {
        "gg" => act(Action::Move(Motion::DocumentStart)),
        "dd" => {
            let mut out = Vec::new();
            for _ in 0..count {
                out.push(Effect::Action(Action::SelectLine));
                out.push(Effect::Action(Action::Edit(Edit::Delete)));
            }
            out.push(Effect::Keys);
            out
        }
        "dw" => {
            let mut out = Vec::new();
            for _ in 0..count {
                out.push(Effect::Action(Action::Select(Motion::WordRight)));
                out.push(Effect::Action(Action::Edit(Edit::Delete)));
            }
            out.push(Effect::Keys);
            out
        }
        "yy" => {
            let mut out = vec![Effect::Action(Action::SelectLine), Effect::Copy];
            out.push(Effect::Keys);
            out
        }
        "yw" => vec![
            Effect::Action(Action::Select(Motion::WordRight)),
            Effect::Copy,
            Effect::Keys,
        ],
        rest if rest == "g" || rest == "d" || rest == "y" => {
            state.pending = pending;
            vec![Effect::Keys]
        }
        _ => vec![Effect::Keys],
    }
}

fn split_count(s: &str) -> (usize, &str) {
    let i = s.bytes().position(|b| !b.is_ascii_digit()).unwrap_or(s.len());
    if i == 0 {
        (1, s)
    } else {
        (s[..i].parse().unwrap_or(1).max(1), &s[i..])
    }
}

fn times(n: usize, action: Action) -> Vec<Effect> {
    let mut out: Vec<Effect> = (0..n).map(|_| Effect::Action(action.clone())).collect();
    out.push(Effect::Keys);
    out
}

fn act(action: Action) -> Vec<Effect> {
    vec![Effect::Action(action), Effect::Keys]
}

fn mode(state: &mut State, mode: Mode) -> Vec<Effect> {
    state.set_mode(mode);
    vec![Effect::Mode(mode), Effect::Keys]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_enters_insert() {
        let mut s = State {
            enabled: true,
            ..State::default()
        };
        let out = handle(&mut s, &Key::Character("i".into()), Modifiers::empty(), Some("i"));
        assert!(matches!(s.mode, Mode::Insert));
        assert!(out.iter().any(|e| matches!(e, Effect::Mode(Mode::Insert))));
    }

    #[test]
    fn escape_leaves_insert() {
        let mut s = State {
            enabled: true,
            mode: Mode::Insert,
            ..State::default()
        };
        handle(&mut s, &Key::Named(Named::Escape), Modifiers::empty(), None);
        assert!(matches!(s.mode, Mode::Normal));
    }

    #[test]
    fn count_is_shown_then_motion() {
        let mut s = State {
            enabled: true,
            ..State::default()
        };
        handle(&mut s, &Key::Character("1".into()), Modifiers::empty(), Some("1"));
        handle(&mut s, &Key::Character("2".into()), Modifiers::empty(), Some("2"));
        assert_eq!(s.keys(), "12");
        let out = handle(&mut s, &Key::Character("j".into()), Modifiers::empty(), Some("j"));
        assert_eq!(
            out.iter()
                .filter(|e| matches!(e, Effect::Action(Action::Move(Motion::Down))))
                .count(),
            12
        );
        assert!(s.keys().is_empty());
    }

    #[test]
    fn colon_enters_command() {
        let mut s = State {
            enabled: true,
            ..State::default()
        };
        handle(&mut s, &Key::Character(":".into()), Modifiers::empty(), Some(":"));
        assert!(matches!(s.mode, Mode::Command));
        handle(&mut s, &Key::Character("w".into()), Modifiers::empty(), Some("w"));
        assert_eq!(s.keys(), ":w");
        let out = handle(&mut s, &Key::Named(Named::Enter), Modifiers::empty(), None);
        assert!(out.iter().any(|e| matches!(e, Effect::Run(c) if c == "w")));
        assert!(matches!(s.mode, Mode::Normal));
    }

    #[test]
    fn only_visual_modes_paint_a_selection() {
        assert!(!Mode::Normal.paints_selection());
        assert!(!Mode::Insert.paints_selection());
        assert!(Mode::Visual.paints_selection());
        assert!(Mode::VisualLine.paints_selection());
        assert!(Mode::VisualBlock.paints_selection());
    }
}
