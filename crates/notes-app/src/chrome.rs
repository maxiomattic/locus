//! COSMIC Text–style chrome: menus, header icons, keybinds.

use cosmic::cosmic_theme::palette::Srgb;
use cosmic::cosmic_theme::{ThemeBuilder, DARK_PALETTE, LIGHT_PALETTE};
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::keyboard::Key;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::menu::key_bind::Modifier;
use cosmic::widget::menu::{self, ItemHeight, ItemWidth, KeyBind};
use cosmic::widget::{self, nav_bar, RcElementWrapper};
use cosmic::{theme, Element};
use notes_sync::Note;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::markdown;
use crate::{Message, Pane};

pub static SEARCH_ID: LazyLock<widget::Id> =
    LazyLock::new(|| widget::Id::new("notes-search"));

pub static FIND_ID: LazyLock<widget::Id> = LazyLock::new(|| widget::Id::new("notes-find"));

const GRAPH_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <g fill="currentColor">
    <circle cx="8" cy="3.5" r="2.15"/>
    <circle cx="3.2" cy="12.2" r="2.15"/>
    <circle cx="12.8" cy="12.2" r="2.15"/>
  </g>
  <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
    <path d="M6.4 4.9 L4.5 10.4"/>
    <path d="M9.6 4.9 L11.5 10.4"/>
    <path d="M5.3 12.2 H10.7"/>
  </g>
</svg>"#;

fn graph_handle() -> widget::icon::Handle {
    widget::icon::from_svg_bytes(GRAPH_SVG).symbolic(true)
}

const PIN_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="currentColor" d="M4.4 1.6h7.2l.5 2.3-1.4.5v2l1.9 1.2v1.2H9.4v3.3l-1 .8-1-.8V8.8H3.4V7.6l1.9-1.2v-2l-1.4-.5Z"/>
</svg>"#;

const FOLDER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="currentColor" d="M2 3.2h4l1.2 1.6H14v7.6a1.4 1.4 0 0 1-1.4 1.4H3.4A1.4 1.4 0 0 1 2 12.4Z"/>
</svg>"#;

const REFRESH_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" d="M12.8 8a4.8 4.8 0 1 1-1.5-3.5 M11.6 2.4l.2 2.4-2.4.2"/>
</svg>"#;

const FINDUP_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" d="M3.5 9.5 8 5l4.5 4.5"/>
</svg>"#;

const FINDDOWN_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" d="M3.5 6.5 8 11l4.5-4.5"/>
</svg>"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Accent {
    System,
    Blue,
    Indigo,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    WarmGrey,
}

impl Accent {
    const ALL: [Accent; 10] = [
        Self::System,
        Self::Blue,
        Self::Indigo,
        Self::Purple,
        Self::Pink,
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::WarmGrey,
    ];

    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Blue => "blue",
            Self::Indigo => "indigo",
            Self::Purple => "purple",
            Self::Pink => "pink",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::WarmGrey => "warm-grey",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Blue => "Blue",
            Self::Indigo => "Indigo",
            Self::Purple => "Purple",
            Self::Pink => "Pink",
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::WarmGrey => "Warm grey",
        }
    }

    fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|a| a.as_str() == name)
            .unwrap_or(Self::System)
    }

    fn srgb(self) -> Option<Srgb> {
        let choice = match self {
            Self::System => return None,
            other => other,
        };
        let inner = if theme::is_dark() {
            DARK_PALETTE.as_ref()
        } else {
            LIGHT_PALETTE.as_ref()
        };
        let c = match choice {
            Self::System => unreachable!(),
            Self::Blue => inner.accent_blue,
            Self::Indigo => inner.accent_indigo,
            Self::Purple => inner.accent_purple,
            Self::Pink => inner.accent_pink,
            Self::Red => inner.accent_red,
            Self::Orange => inner.accent_orange,
            Self::Yellow => inner.accent_yellow,
            Self::Green => inner.accent_green,
            Self::WarmGrey => inner.accent_warm_grey,
        };
        Some(Srgb::new(c.red, c.green, c.blue))
    }
}

/// Read `org.freedesktop.appearance accent-color` from the XDG Settings portal.
/// Returns sRGB in [0, 1]; out-of-range values mean "no preference".
fn portal_accent_srgb() -> Option<Srgb> {
    let out = std::process::Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.ReadOne",
            "org.freedesktop.appearance",
            "accent-color",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8(out.stdout).ok()?;
    // e.g. (<(0.227, 0.580, 0.290)>,)
    let start = raw.find("((").or_else(|| raw.find("<("))?;
    let nums = raw.get(start + 2..)?;
    let end = nums.find(')')?;
    let mut parts = nums[..end].split(',').map(|s| s.trim().parse::<f64>().ok());
    let r = parts.next()??;
    let g = parts.next()??;
    let b = parts.next()??;
    if !(0.0..=1.0).contains(&r) || !(0.0..=1.0).contains(&g) || !(0.0..=1.0).contains(&b) {
        return None;
    }
    Some(Srgb::new(r as f32, g as f32, b as f32))
}

fn accent_path() -> std::path::PathBuf {
    notes_sync::data_dir().join("accent")
}

pub fn load_accent() -> Accent {
    std::fs::read_to_string(accent_path())
        .ok()
        .map(|s| Accent::from_name(s.trim()))
        .unwrap_or(Accent::System)
}

pub fn save_accent(accent: Accent) {
    let path = accent_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, accent.as_str());
}

pub const NAV_WIDTH_DEFAULT: f32 = 260.0;
pub const NAV_WIDTH_MIN: f32 = 180.0;
pub const NAV_WIDTH_MAX: f32 = 520.0;

fn nav_width_path() -> std::path::PathBuf {
    notes_sync::data_dir().join("nav-width")
}

pub fn clamp_nav_width(width: f32) -> f32 {
    width.clamp(NAV_WIDTH_MIN, NAV_WIDTH_MAX)
}

pub fn load_nav_width() -> f32 {
    std::fs::read_to_string(nav_width_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .map(clamp_nav_width)
        .unwrap_or(NAV_WIDTH_DEFAULT)
}

pub fn save_nav_width(width: f32) {
    let path = nav_width_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, clamp_nav_width(width).to_string());
}

fn pinned_path() -> std::path::PathBuf {
    notes_sync::data_dir().join("pinned")
}

pub fn pinned_ids() -> BTreeSet<u64> {
    std::fs::read_to_string(pinned_path())
        .ok()
        .map(|s| {
            s.lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_pinned(pinned: &BTreeSet<u64>) {
    let path = pinned_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text: String = pinned
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let _ = std::fs::write(path, text);
}

pub fn theme_for(accent: Accent) -> theme::Theme {
    let color = match accent {
        Accent::System => portal_accent_srgb(),
        other => other.srgb(),
    };
    if matches!(accent, Accent::System) && color.is_none() {
        return theme::system_preference();
    }
    let mut builder = match (theme::is_dark(), theme::is_high_contrast()) {
        (true, true) => ThemeBuilder::dark_high_contrast(),
        (true, false) => ThemeBuilder::dark(),
        (false, true) => ThemeBuilder::light_high_contrast(),
        (false, false) => ThemeBuilder::light(),
    };
    if let Some(c) = color {
        builder = builder.accent(c);
    }
    theme::Theme::custom(Arc::new(builder.build()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuAct {
    NewNote,
    Duplicate,
    DeleteNote,
    SaveSync,
    CopyMd,
    VersionHistory,
    Undo,
    Redo,
    FindNotes,
    FindInNote,
    Bold,
    Italic,
    Strike,
    Code,
    Heading,
    Bullet,
    Numbered,
    Task,
    Quote,
    Wiki,
    Editor,
    Write,
    Preview,
    Graph,
    GoBack,
    GoForward,
    PrevNote,
    NextNote,
    DailyNow,
    SyncNow,
    OpenVaultFile,
    JumpTo(usize),
    SortNewest,
    SortTitle,
    Vim,
    Accent(Accent),
}

impl menu::Action for MenuAct {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            Self::NewNote => Message::NewNote,
            Self::Duplicate => Message::Duplicate,
            Self::DeleteNote => Message::DeleteCurrent,
            Self::SaveSync => Message::Commit,
            Self::CopyMd => Message::CopyNoteMd,
            Self::VersionHistory => Message::ShowHistory,
            Self::Undo => Message::Undo,
            Self::Redo => Message::Redo,
            Self::FindNotes => Message::FocusSearch,
            Self::FindInNote => Message::FindToggle,
            Self::Bold => Message::InsertBold,
            Self::Italic => Message::InsertItalic,
            Self::Strike => Message::InsertStrike,
            Self::Code => Message::InsertCode,
            Self::Heading => Message::FormatHeading,
            Self::Bullet => Message::FormatBullet,
            Self::Numbered => Message::FormatNumbered,
            Self::Task => Message::FormatTask,
            Self::Quote => Message::FormatQuote,
            Self::Wiki => Message::InsertWiki,
            Self::Editor => Message::SetPane(Pane::Page),
            Self::Write => Message::SetPane(Pane::Write),
            Self::Preview => Message::SetPane(Pane::Preview),
            Self::Graph => Message::SetPane(Pane::Graph),
            Self::GoBack => Message::GoBack,
            Self::GoForward => Message::GoForward,
            Self::PrevNote => Message::PrevNote,
            Self::NextNote => Message::NextNote,
            Self::DailyNow => Message::DailyNow,
            Self::SyncNow => Message::SyncNow,
            Self::OpenVaultFile => Message::OpenVaultFile,
            Self::JumpTo(line) => Message::SetCaret(*line, 0),
            Self::SortNewest => Message::SetSortTitle(false),
            Self::SortTitle => Message::SetSortTitle(true),
            Self::Vim => Message::ToggleVim,
            Self::Accent(accent) => Message::SetAccent(*accent),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NavMenuAction {
    Delete(u64),
    Pin(u64),
    Unpin(u64),
}

impl menu::Action for NavMenuAction {
    type Message = cosmic::Action<Message>;

    fn message(&self) -> Self::Message {
        match self {
            Self::Delete(id) => cosmic::Action::App(Message::DeleteAskNote(*id)),
            Self::Pin(id) => cosmic::Action::App(Message::TogglePin(*id)),
            Self::Unpin(id) => cosmic::Action::App(Message::TogglePin(*id)),
        }
    }
}

pub fn key_binds() -> HashMap<KeyBind, MenuAct> {
    let mut binds = HashMap::new();
    macro_rules! bind {
        ([$($modifier:ident),+ $(,)?], $key:expr, $action:ident) => {
            binds.insert(
                KeyBind {
                    modifiers: vec![$(Modifier::$modifier),+],
                    key: $key,
                },
                MenuAct::$action,
            );
        };
    }
    bind!([Ctrl], Key::Character("n".into()), NewNote);
    bind!([Ctrl, Shift], Key::Named(Named::Delete), DeleteNote);
    bind!([Ctrl], Key::Character("s".into()), SaveSync);
    bind!([Ctrl, Shift], Key::Character("d".into()), Duplicate);
    bind!([Ctrl, Shift], Key::Character("c".into()), CopyMd);
    bind!([Ctrl], Key::Character("z".into()), Undo);
    bind!([Ctrl, Shift], Key::Character("z".into()), Redo);
    bind!([Ctrl], Key::Character("y".into()), Redo);
    bind!([Ctrl], Key::Character("f".into()), FindNotes);
    bind!([Ctrl, Shift], Key::Character("f".into()), FindInNote);
    bind!([Ctrl], Key::Character("b".into()), Bold);
    bind!([Ctrl], Key::Character("i".into()), Italic);
    bind!([Ctrl], Key::Character("k".into()), Wiki);
    bind!([Ctrl], Key::Character("j".into()), DailyNow);
    bind!([Alt], Key::Named(Named::ArrowLeft), GoBack);
    bind!([Alt], Key::Named(Named::ArrowRight), GoForward);
    bind!([Ctrl], Key::Character("[".into()), PrevNote);
    bind!([Ctrl], Key::Character("]".into()), NextNote);
    bind!([Ctrl], Key::Character("1".into()), Editor);
    bind!([Ctrl], Key::Character("2".into()), Write);
    bind!([Ctrl], Key::Character("p".into()), Preview);
    bind!([Ctrl], Key::Character("g".into()), Graph);
    binds
}

fn item(label: &str, icon: Option<&str>, action: MenuAct) -> menu::Item<MenuAct, String> {
    menu::Item::Button(
        label.to_string(),
        icon.map(|name| widget::icon::from_name(name).into()),
        action,
    )
}

pub fn menu_bar<'a>(
    key_binds: &HashMap<KeyBind, MenuAct>,
    pane: &Pane,
    sort_title: bool,
    accent: Accent,
    vim: bool,
    headings: &[markdown::Heading],
    can_open_vault: bool,
) -> Element<'a, Message> {
    let mut note_items = vec![
        item("New note", Some("document-new-symbolic"), MenuAct::NewNote),
        item("Duplicate", None, MenuAct::Duplicate),
        item("Delete note", Some("user-trash-symbolic"), MenuAct::DeleteNote),
        menu::Item::Divider,
        item("Save & sync now", None, MenuAct::SaveSync),
        item("Copy note as Markdown", None, MenuAct::CopyMd),
        menu::Item::Divider,
        item("Version history", None, MenuAct::VersionHistory),
    ];
    if can_open_vault {
        note_items.push(item("Open vault file", None, MenuAct::OpenVaultFile));
    }

    let mut go_items = vec![
        item("Back", Some("go-previous-symbolic"), MenuAct::GoBack),
        item("Forward", Some("go-next-symbolic"), MenuAct::GoForward),
        menu::Item::Divider,
        item("Previous note", None, MenuAct::PrevNote),
        item("Next note", None, MenuAct::NextNote),
        menu::Item::Divider,
        item("Today's note", None, MenuAct::DailyNow),
        menu::Item::Divider,
        item("Sync now", Some("view-refresh-symbolic"), MenuAct::SyncNow),
    ];
    if !headings.is_empty() {
        go_items.push(menu::Item::Folder(
            "Jump to heading".into(),
            headings
                .iter()
                .map(|h| {
                    let depth = "  ".repeat(h.level.saturating_sub(1) as usize);
                    menu::Item::Button(
                        format!("{depth}{}", truncate_text(&h.text, 48)),
                        None,
                        MenuAct::JumpTo(h.line),
                    )
                })
                .collect(),
        ));
    }

    let view_items: Vec<menu::Item<MenuAct, String>> = vec![
        menu::Item::CheckBox(
            "Markdown".into(),
            None,
            matches!(pane, Pane::Page),
            MenuAct::Editor,
        ),
        menu::Item::CheckBox(
            "Rich text".into(),
            None,
            matches!(pane, Pane::Write),
            MenuAct::Write,
        ),
        menu::Item::CheckBox(
            "Graph".into(),
            None,
            matches!(pane, Pane::Graph),
            MenuAct::Graph,
        ),
        menu::Item::CheckBox(
            "Preview".into(),
            None,
            matches!(pane, Pane::Preview),
            MenuAct::Preview,
        ),
        menu::Item::Divider,
        item(
            "Find in note",
            Some("edit-find-symbolic"),
            MenuAct::FindInNote,
        ),
        menu::Item::Divider,
        menu::Item::CheckBox("Vim".into(), None, vim, MenuAct::Vim),
        menu::Item::Divider,
        menu::Item::CheckBox(
            "Sort by newest".into(),
            None,
            !sort_title,
            MenuAct::SortNewest,
        ),
        menu::Item::CheckBox("Sort by title".into(), None, sort_title, MenuAct::SortTitle),
        menu::Item::Divider,
        menu::Item::Folder(
            "Accent".into(),
            Accent::ALL
                .into_iter()
                .map(|choice| {
                    menu::Item::CheckBox(
                        choice.label().into(),
                        None,
                        accent == choice,
                        MenuAct::Accent(choice),
                    )
                })
                .collect(),
        ),
    ];

    let tree = |label: &'static str,
                items: Vec<menu::Item<MenuAct, String>>| {
        menu::Tree::with_children(
            RcElementWrapper::new(Element::from(menu::root(label))),
            menu::items(key_binds, items),
        )
    };

    menu::bar(vec![
        tree("Note", note_items),
        tree("Edit", edit_items()),
        tree("Go", go_items),
        tree("View", view_items),
    ])
    .item_height(ItemHeight::Dynamic(36))
    .item_width(ItemWidth::Uniform(280))
    .spacing(4.0)
    .into()
}

fn edit_items() -> Vec<menu::Item<MenuAct, String>> {
    vec![
        item("Undo", None, MenuAct::Undo),
        item("Redo", None, MenuAct::Redo),
        menu::Item::Divider,
        item("Bold", None, MenuAct::Bold),
        item("Italic", None, MenuAct::Italic),
        item("Strikethrough", None, MenuAct::Strike),
        item("Code", None, MenuAct::Code),
        item("Heading", None, MenuAct::Heading),
        item("Bulleted list", None, MenuAct::Bullet),
        item("Numbered list", None, MenuAct::Numbered),
        item("Task", None, MenuAct::Task),
        item("Quote", None, MenuAct::Quote),
        item("Wikilink", None, MenuAct::Wiki),
        menu::Item::Divider,
        item("Search notes", None, MenuAct::FindNotes),
    ]
}

fn truncate_text(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

pub fn header_end<'a>(pane: &Pane) -> Vec<Element<'a, Message>> {
    vec![widget::row::with_capacity(3)
        .spacing(0)
        .align_y(Alignment::Center)
        .push(header_icon(
            widget::icon::from_name("accessories-text-editor-symbolic").into(),
            "Markdown",
            matches!(pane, Pane::Page),
            Some(Message::SetPane(Pane::Page)),
        ))
        .push(header_icon(
            widget::icon::from_name("format-text-bold-symbolic").into(),
            "Rich text",
            matches!(pane, Pane::Write),
            Some(Message::SetPane(Pane::Write)),
        ))
        .push(header_icon(
            graph_handle(),
            "Graph",
            matches!(pane, Pane::Graph),
            Some(Message::SetPane(Pane::Graph)),
        ))
        .into()]
}

const BOLD_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="currentColor" d="m 4 3 v 2 h 5 c 0.429688 0 1 0.613281 1 1 v 1 h -5 c -0.917969 0 -1.734375 0.378906 -2.25 0.964844 c -0.515625 0.585937 -0.742188 1.324218 -0.738281 2.046875 c 0.007812 0.71875 0.246093 1.445312 0.757812 2.027343 c 0.515625 0.578126 1.320313 0.960938 2.230469 0.960938 h 9 v -7 c 0 -1.632812 -1.320312 -3 -3 -3 z m 3 6 h 3 v 2 h -3 c -0.398438 0 -0.578125 -0.117188 -0.730469 -0.289062 c -0.152343 -0.167969 -0.253906 -0.441407 -0.257812 -0.722657 c 0 -0.277343 0.09375 -0.539062 0.238281 -0.703125 c 0.148438 -0.164062 0.328125 -0.285156 0.75 -0.285156 z"/></svg>"#;

const ITALIC_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="currentColor" d="m 5 3 v 2 h 6 c 0.429688 0 0.609375 0.28125 0.480469 0.730469 l -0.347657 1.269531 h -5.242187 c -1.839844 0 -3.132813 1.34375 -3.503906 2.738281 c -0.1875 0.699219 -0.175781 1.480469 0.238281 2.15625 c 0.410156 0.675781 1.25 1.105469 2.15625 1.105469 h 6.761719 l 1.863281 -6.730469 c 0.4375 -1.570312 -0.726562 -3.269531 -2.40625 -3.269531 z m 0.890625 6 h 4.683594 l -0.554688 2 h -5.238281 c -0.394531 0 -0.40625 -0.070312 -0.449219 -0.144531 c -0.046875 -0.074219 -0.097656 -0.292969 -0.015625 -0.59375 c 0.160156 -0.605469 0.730469 -1.261719 1.574219 -1.261719 z"/></svg>"#;

const STRIKE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="currentColor" d="m 4 3 v 2 h 6 c 0.429688 0 1 0.613281 1 1 h 2 c 0 -1.632812 -1.320312 -3 -3 -3 z m -3 4 v 2 h 14 v -2 z m 2 3 c 0.007812 0.71875 0.269531 1.453125 0.78125 2.03125 s 1.308594 0.96875 2.21875 0.96875 h 7 v -3 h -2 v 1 h -5 c -0.398438 0 -0.566406 -0.109375 -0.71875 -0.28125 s -0.277344 -0.4375 -0.28125 -0.71875 z"/></svg>"#;

const LINK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" d="M6.8 9.2 L5.2 10.8 a2.2 2.2 0 1 1-3.1-3.1 L3.7 6.1 M9.2 6.8 L10.8 5.2 a2.2 2.2 0 1 1 3.1 3.1 L12.3 9.9 M6.2 9.8 L9.8 6.2"/></svg>"#;

const TASK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" d="M3.2 3.2 H12.8 V12.8 H3.2 Z M5 8.1 L7.1 10.2 L11.2 5.6"/></svg>"#;

const HEADING_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" d="M4 3.2 V12.8 M12 3.2 V12.8 M4 8 H12"/>
</svg>"#;

const CODE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" d="M6.2 4.2 L2.8 8 L6.2 11.8 M9.8 4.2 L13.2 8 L9.8 11.8"/>
</svg>"#;

const BULLET_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <g fill="currentColor">
    <circle cx="3.4" cy="4" r="1.15"/>
    <circle cx="3.4" cy="8" r="1.15"/>
    <circle cx="3.4" cy="12" r="1.15"/>
  </g>
  <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
    <path d="M6.4 4 H13"/>
    <path d="M6.4 8 H13"/>
    <path d="M6.4 12 H13"/>
  </g>
</svg>"#;

const NUMBERED_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" d="M3 3.2 V6.2 M2.2 6.2 H3.8 M6.4 4 H13 M6.4 8 H13 M6.4 12 H13"/>
  <path fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" d="M2.2 9.2 H3.6 C4.2 9.2 4.6 9.6 4.6 10.1 C4.6 10.6 4.2 11 3.6 11 H2.8 L4.6 13"/>
</svg>"#;

const QUOTE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">
  <path fill="currentColor" d="M3.2 12.6 L6.4 7.2 V3.4 H2.4 V7.2 H4.7 L3.2 12.6 Z M9.4 12.6 L12.6 7.2 V3.4 H8.6 V7.2 H10.9 L9.4 12.6 Z"/>
</svg>"#;

fn svg_handle(bytes: &'static [u8]) -> widget::icon::Handle {
    widget::icon::from_svg_bytes(bytes).symbolic(true)
}

pub fn format_bar<'a>() -> Element<'a, Message> {
    let space = theme::spacing();
    let btn = |svg: &'static [u8], tip: &'static str, msg: Message| {
        widget::button::icon(svg_handle(svg))
            .extra_small()
            .icon_size(20)
            .tooltip(tip)
            .on_press(msg)
    };
    widget::row::with_capacity(10)
        .spacing(space.space_xxxs)
        .align_y(Alignment::Center)
        .push(btn(BOLD_SVG, "Bold", Message::InsertBold))
        .push(btn(ITALIC_SVG, "Italic", Message::InsertItalic))
        .push(btn(STRIKE_SVG, "Strikethrough", Message::InsertStrike))
        .push(btn(CODE_SVG, "Code", Message::InsertCode))
        .push(btn(HEADING_SVG, "Heading", Message::FormatHeading))
        .push(btn(BULLET_SVG, "Bulleted list", Message::FormatBullet))
        .push(btn(NUMBERED_SVG, "Numbered list", Message::FormatNumbered))
        .push(btn(TASK_SVG, "Task", Message::FormatTask))
        .push(btn(QUOTE_SVG, "Quote", Message::FormatQuote))
        .push(btn(LINK_SVG, "Wikilink", Message::InsertWiki))
        .into()
}

fn header_icon<'a>(
    handle: widget::icon::Handle,
    tooltip: &'static str,
    selected: bool,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    widget::tooltip(
        widget::button::icon(handle)
            .padding(8)
            .icon_size(16)
            .class(theme::Button::HeaderBar)
            .selected(selected)
            .on_press_maybe(on_press),
        widget::text(tooltip),
        widget::tooltip::Position::Bottom,
    )
    .into()
}

pub fn nav_context_menu(
    model: &nav_bar::Model,
    pinned: &BTreeSet<u64>,
) -> Vec<menu::Tree<cosmic::Action<Message>>> {
    menu::nav_context(
        &HashMap::<KeyBind, NavMenuAction>::new(),
        model
            .iter()
            .map(|entity| {
                let id = model.data::<u64>(entity).copied().unwrap_or(0);
                let pin = if pinned.contains(&id) {
                    menu::Item::Button(
                        "Unpin note",
                        Some(svg_handle(PIN_SVG)),
                        NavMenuAction::Unpin(id),
                    )
                } else {
                    menu::Item::Button(
                        "Pin note",
                        Some(svg_handle(PIN_SVG)),
                        NavMenuAction::Pin(id),
                    )
                };
                vec![
                    pin,
                    menu::Item::Button(
                        "Delete",
                        Some(widget::icon::from_name("user-trash-symbolic").into()),
                        NavMenuAction::Delete(id),
                    ),
                ]
            })
            .collect(),
    )
}

pub fn nav_tools<'a>(
    search: &'a str,
    sort_title: bool,
    busy: bool,
) -> Element<'a, cosmic::Action<Message>> {
    let space = theme::spacing();
    let sync = widget::tooltip(
        widget::button::icon(svg_handle(REFRESH_SVG)).on_press_maybe(
            (!busy).then_some(cosmic::Action::App(Message::SyncNow)),
        ),
        widget::text("Sync now"),
        widget::tooltip::Position::Bottom,
    );
    let new_note = widget::tooltip(
        widget::button::icon(widget::icon::from_name("document-new-symbolic"))
            .on_press_maybe((!busy).then_some(cosmic::Action::App(Message::NewNote))),
        widget::text("New note"),
        widget::tooltip::Position::Bottom,
    );
    let search = widget::search_input("Search notes", search)
        .id(SEARCH_ID.clone())
        .on_input(|s| cosmic::Action::App(Message::Search(s)))
        .on_clear(cosmic::Action::App(Message::Search(String::new())));
    let sort = widget::tooltip(
        widget::button::icon(widget::icon::from_name("view-sort-descending-symbolic"))
            .selected(sort_title)
            .on_press(cosmic::Action::App(Message::SortTitle)),
        widget::text(if sort_title {
            "Sort by title"
        } else {
            "Sort by newest"
        }),
        widget::tooltip::Position::Bottom,
    );
    widget::row::with_capacity(5)
        .spacing(space.space_xxs)
        .align_y(Alignment::Center)
        .push(sync)
        .push(sort)
        .push(search)
        .push(new_note)
        .into()
}

pub fn note_nav<'a>(
    model: &'a nav_bar::Model,
    notes: &'a [Note],
    search: &'a str,
    pinned: &'a BTreeSet<u64>,
    dirty: &'a HashSet<u64>,
) -> Element<'a, cosmic::Action<Message>> {
    use cosmic::iced::core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
    let space = theme::spacing();
    let radii = theme::active().cosmic().corner_radii.radius_s;
    let empty_binds = HashMap::<KeyBind, NavMenuAction>::new();
    let count = model.iter().count();
    if count == 0 {
        let note = if search.trim().is_empty() {
            "No notes yet — press Ctrl+N"
        } else {
            "No matches"
        };
        return widget::container(
            widget::text::caption(note)
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .padding([space.space_l, space.space_xxs])
        .into();
    }
    let mut col = widget::column::with_capacity(count)
        .spacing(space.space_xxs)
        .width(Length::Fill);
    for (row_index, entity) in model.iter().enumerate() {
        let title = model.text(entity).unwrap_or("Untitled");
        let note_id = model.data::<u64>(entity).copied().unwrap_or(0);
        let is_pinned = pinned.contains(&note_id);
        let is_dirty = dirty.contains(&note_id);
        let snippet = notes
            .get(row_index)
            .map(|n| readable_line(&n.body_md))
            .unwrap_or_default();

        let title_row = {
            let mut row = widget::row::with_capacity(4)
                .spacing(space.space_xxs)
                .align_y(Alignment::Center);
            if is_vault_note(notes, row_index) {
                row = row.push(widget::icon(svg_handle(FOLDER_SVG)));
            }
            if is_pinned {
                row = row.push(widget::icon(svg_handle(PIN_SVG)));
            }
            row = row.push(
                widget::text(title)
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                    .width(Length::Fill),
            );
            if is_dirty {
                row = row.push(widget::text("●").size(10.0).class(theme::Text::Accent));
            }
            row
        };
        let label: Element<'_, cosmic::Action<Message>> = if snippet.is_empty() {
            widget::column::with_capacity(1).push(title_row).into()
        } else {
            let sub = if is_vault_note(notes, row_index) {
                format!("vault · {snippet}")
            } else {
                snippet
            };
            widget::column::with_capacity(2)
                .push(title_row)
                .push(
                    widget::text::caption(sub)
                        .wrapping(Wrapping::None)
                        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                        .width(Length::Fill),
                )
                .into()
        };

        let button = widget::button::custom(label)
            .padding([space.space_xxs, space.space_s])
            .width(Length::Fill)
            .selected(model.is_active(entity))
            .class(theme::Button::ListItem(radii))
            .on_press(cosmic::Action::Cosmic(cosmic::app::Action::NavBar(entity)));
        let row = widget::tooltip(
            button,
            widget::text(title),
            widget::tooltip::Position::Right,
        );
        let mut items = vec![menu::Item::Button(
            "Delete",
            Some(widget::icon::from_name("user-trash-symbolic").into()),
            NavMenuAction::Delete(note_id),
        )];
        items.push(if is_pinned {
            menu::Item::Button(
                "Unpin note",
                Some(svg_handle(PIN_SVG)),
                NavMenuAction::Unpin(note_id),
            )
        } else {
            menu::Item::Button(
                "Pin note",
                Some(svg_handle(PIN_SVG)),
                NavMenuAction::Pin(note_id),
            )
        });
        let menu = menu::items(&empty_binds, items);
        col = col.push(widget::context_menu(row, Some(menu)));
    }
    widget::scrollable(widget::container(col).padding(space.space_xxs).width(Length::Fill))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn is_vault_note(notes: &[notes_sync::Note], index: usize) -> bool {
    notes
        .get(index)
        .map(|n| n.id >= notes_sync::VAULT_ID_BASE)
        .unwrap_or(false)
}

pub fn nav_resize_handle<'a>() -> Element<'a, cosmic::Action<Message>> {
    widget::mouse_area(
        widget::container(widget::divider::vertical::default())
            .width(Length::Fixed(8.0))
            .height(Length::Fill)
            .align_x(Alignment::Center),
    )
    .on_press(cosmic::Action::App(Message::NavResizeStart))
    .interaction(cosmic::iced::mouse::Interaction::ResizingHorizontally)
    .into()
}

pub fn footer<'a>(
    status: &'a str,
    counts: Option<(usize, usize, usize)>,
    caret: Option<(usize, usize)>,
    vim: Option<(&'static str, String)>,
) -> Element<'a, Message> {
    let space = theme::spacing();
    let mut right = widget::row::with_capacity(3)
        .spacing(space.space_s)
        .align_y(Alignment::Center);
    if let Some((line, column)) = caret {
        right = right.push(widget::text::caption(format!(
            "Ln {}, Col {}",
            line + 1,
            column
        )));
    }
    if let Some((words, chars, mins)) = counts {
        if mins > 0 {
            right = right.push(widget::text::caption(format!("~{mins} min read")));
        }
        right = right.push(widget::text::caption(format!("{words} words · {chars} chars")));
    }
    let mut row = widget::row::with_capacity(4)
        .spacing(space.space_s)
        .padding([space.space_xxs, space.space_s])
        .align_y(Alignment::Center)
        .push(widget::text::caption(status).width(Length::Fill))
        .push(right);
    if let Some((mode, keys)) = vim {
        let mut badge = widget::row::with_capacity(3)
            .spacing(space.space_xs)
            .align_y(Alignment::Center)
            .push(widget::text::body("Vim"))
            .push(widget::text::caption(mode));
        if !keys.is_empty() {
            badge = badge.push(widget::text::body(keys));
        }
        row = row.push(
            widget::container(badge)
                .padding([4, 10])
                .class(theme::Container::Card),
        );
    }
    row.into()
}

/// Sidebar search bar for the note being edited: query, "n of m", stepper.
pub fn find_bar<'a>(query: &'a str, found: Option<(usize, usize)>) -> Element<'a, Message> {
    let space = theme::spacing();
    let input = widget::text_input("Find in note", query)
        .id(FIND_ID.clone())
        .on_input(Message::FindInput)
        .on_submit(|_| Message::FindNext(true));
    let counter = widget::text::caption(match found {
        Some((i, n)) => format!("{}/{}", i + 1, n),
        None => String::new(),
    });
    let prev = widget::button::icon(svg_handle(FINDUP_SVG))
        .extra_small()
        .on_press_maybe(found.filter(|(_, n)| *n > 0).map(|_| Message::FindNext(false)));
    let next = widget::button::icon(svg_handle(FINDDOWN_SVG))
        .extra_small()
        .on_press_maybe(found.filter(|(_, n)| *n > 0).map(|_| Message::FindNext(true)));
    let close = widget::button::icon(widget::icon::from_name("window-close-symbolic"))
        .extra_small()
        .on_press(Message::FindClose);
    widget::row::with_capacity(5)
        .spacing(space.space_xs)
        .align_y(Alignment::Center)
        .push(input)
        .push(counter.width(Length::Fixed(64.0)).align_x(Alignment::Center))
        .push(prev)
        .push(next)
        .push(close)
        .into()
}

/// "Linked from" chips: other notes whose wikilinks point here.
pub fn backlinks_row(links: Vec<(u64, String)>) -> Element<'static, Message> {
    let space = theme::spacing();
    let chips: Vec<Element<'static, Message>> = links
        .iter()
        .map(|(id, title)| {
            widget::button::link(title.to_string())
                .on_press(Message::Open(*id))
                .into()
        })
        .collect();
    widget::row::with_capacity(2)
        .spacing(space.space_xs)
        .align_y(Alignment::Center)
        .push(widget::text::caption("Linked from"))
        .push(widget::flex_row(chips).width(Length::Fill))
        .into()
}

/// Empty workspace: no note open and nothing drafted.
pub fn welcome_panel<'a>() -> Element<'a, Message> {
    let space = theme::spacing();
    let body = widget::column::with_capacity(4)
        .spacing(space.space_m)
        .align_x(Alignment::Center)
        .push(widget::text::title1("Notes"))
        .push(widget::text("Pick a note on the left, or start writing."))
        .push(widget::text::caption(
            "Markdown keeps its markers here; Daily note, graph, and history are in the Go menu.",
        ))
        .push(
            widget::button::suggested("New note").on_press(Message::NewNote),
        );
    widget::container(
        widget::container(body)
            .class(theme::Container::Card)
            .padding(space.space_l)
            .max_width(420.0),
    )
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EditMenuAction {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Bold,
    Italic,
    Code,
    Wiki,
    Heading,
    Task,
}

impl menu::Action for EditMenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            Self::Undo => Message::Undo,
            Self::Redo => Message::Redo,
            Self::Cut => Message::EditCut,
            Self::Copy => Message::EditCopy,
            Self::Paste => Message::EditRequestPaste,
            Self::SelectAll => Message::Body(cosmic::iced::widget::text_editor::Action::SelectAll),
            Self::Bold => Message::InsertBold,
            Self::Italic => Message::InsertItalic,
            Self::Code => Message::InsertCode,
            Self::Wiki => Message::InsertWiki,
            Self::Heading => Message::FormatHeading,
            Self::Task => Message::FormatTask,
        }
    }
}

/// Right-click menu over the editor surface.
pub fn editor_context_menu<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let empty = HashMap::<KeyBind, EditMenuAction>::new();
    let items = vec![
        menu::Item::Button("Undo".to_string(), None, EditMenuAction::Undo),
        menu::Item::Button("Redo".to_string(), None, EditMenuAction::Redo),
        menu::Item::Divider,
        menu::Item::Button("Cut".to_string(), None, EditMenuAction::Cut),
        menu::Item::Button("Copy".to_string(), None, EditMenuAction::Copy),
        menu::Item::Button("Paste".to_string(), None, EditMenuAction::Paste),
        menu::Item::Divider,
        menu::Item::Button("Select all".to_string(), None, EditMenuAction::SelectAll),
        menu::Item::Divider,
        menu::Item::Button("Bold".to_string(), None, EditMenuAction::Bold),
        menu::Item::Button("Italic".to_string(), None, EditMenuAction::Italic),
        menu::Item::Button("Heading".to_string(), None, EditMenuAction::Heading),
        menu::Item::Button("Code".to_string(), None, EditMenuAction::Code),
        menu::Item::Button("Wikilink".to_string(), None, EditMenuAction::Wiki),
        menu::Item::Button("Task".to_string(), None, EditMenuAction::Task),
    ];
    widget::context_menu(content, Some(menu::items(&empty, items))).into()
}

fn relative_age(created_at: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(created_at);
    let secs = (now - created_at).max(0);
    if secs < 45 {
        "just now".into()
    } else if secs < 90 {
        "1 min ago".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 90 * 60 {
        "1 hour ago".into()
    } else if secs < 86400 {
        format!("{} hours ago", secs / 3600)
    } else if secs < 36 * 3600 {
        "yesterday".into()
    } else {
        format!("{} days ago", secs / 86400)
    }
}

fn clip_text(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    let mut out: String = t.chars().take(n).collect();
    if t.chars().count() > n {
        out.push('…');
    }
    out
}

fn readable_line(s: &str) -> String {
    let mut t = s.trim();
    t = t.trim_start_matches('#').trim_start();
    for prefix in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ ", "> "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest;
            break;
        }
    }
    clip_text(t, 52)
}

fn body_delta(old: &str, new: &str) -> String {
    let old_l: Vec<&str> = old.lines().collect();
    let new_l: Vec<&str> = new.lines().collect();
    let mut i = 0;
    while i < old_l.len() && i < new_l.len() && old_l[i] == new_l[i] {
        i += 1;
    }
    let added = readable_line(new_l.get(i).copied().unwrap_or(""));
    let removed = readable_line(old_l.get(i).copied().unwrap_or(""));
    if !added.is_empty() && !removed.is_empty() && added != removed {
        format!("Changed to: {added}")
    } else if !added.is_empty() {
        format!("Added: {added}")
    } else if !removed.is_empty() {
        format!("Removed: {removed}")
    } else if old_l.len() < new_l.len() {
        "Added a blank line".into()
    } else if old_l.len() > new_l.len() {
        "Removed a blank line".into()
    } else {
        "Made a small edit".into()
    }
}

fn change_caption(older: Option<&notes_sync::Snapshot>, snap: &notes_sync::Snapshot) -> String {
    let Some(older) = older else {
        return "Started this note".into();
    };
    let mut parts = Vec::new();
    let old_title = older.title.trim();
    let new_title = snap.title.trim();
    if old_title != new_title {
        parts.push(if new_title.is_empty() {
            "Cleared the title".into()
        } else {
            format!("Changed the title to {new_title}")
        });
    }
    if older.body != snap.body {
        parts.push(body_delta(&older.body, &snap.body));
    }
    if parts.is_empty() {
        "Made a small edit".into()
    } else {
        parts.join(" · ")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LineChg {
    Same(String),
    Added(String),
    Removed(String),
}

fn line_changes(old: &str, new: &str) -> Vec<LineChg> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if a[i] == b[j] {
                dp[i][j] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            out.push(LineChg::Same(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            out.push(LineChg::Added(b[j - 1].to_string()));
            j -= 1;
        } else {
            out.push(LineChg::Removed(a[i - 1].to_string()));
            i -= 1;
        }
    }
    out.reverse();
    out
}

fn compact_changes(rows: Vec<LineChg>) -> Vec<LineChg> {
    if rows.len() <= 24 {
        return rows;
    }
    let mut keep = vec![false; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        if !matches!(row, LineChg::Same(_)) {
            keep[i] = true;
            if i > 0 {
                keep[i - 1] = true;
            }
            if i + 1 < rows.len() {
                keep[i + 1] = true;
            }
        }
    }
    rows.into_iter()
        .zip(keep)
        .filter_map(|(row, on)| on.then_some(row))
        .collect()
}

pub fn version_history_dialog<'a>(
    history: &'a [notes_sync::Snapshot],
    pick: usize,
) -> Element<'a, Message> {
    let dialog = widget::dialog()
        .title("Version history")
        .secondary_action(widget::button::standard("Close").on_press(Message::CloseHistory));
    if history.is_empty() {
        return dialog.body("No versions").into();
    }
    let pick = pick.min(history.len() - 1);
    let labels: Vec<String> = history
        .iter()
        .enumerate()
        .map(|(i, snap)| {
            if i == 0 {
                format!("Current · version {}", snap.seq)
            } else {
                format!("{} · version {}", relative_age(snap.created_at), snap.seq)
            }
        })
        .collect();
    let snap = &history[pick];
    let older = history.get(pick + 1);
    let space = theme::spacing();
    let mut body = widget::column::with_capacity(6)
        .spacing(space.space_xs)
        .push(widget::dropdown(labels, Some(pick), Message::HistoryPick))
        .push(widget::text::body(change_caption(older, snap)));
    if let Some(prev) = older {
        if prev.title.trim() != snap.title.trim() {
            body = body.push(widget::text::caption(format!(
                "Title was “{}”, now “{}”",
                clip_text(&prev.title, 40),
                clip_text(&snap.title, 40)
            )));
        }
        let changes = compact_changes(line_changes(&prev.body, &snap.body));
        let mut lines = widget::column::with_capacity(changes.len().max(1));
        if changes.iter().all(|c| matches!(c, LineChg::Same(_))) {
            lines = lines.push(widget::text::caption("The words on the page did not change."));
        } else {
            for chg in &changes {
                lines = lines.push(match chg {
                    LineChg::Same(s) => widget::text::caption(if s.is_empty() {
                        " ".into()
                    } else {
                        s.clone()
                    }),
                    LineChg::Added(s) => widget::text::body(format!(
                        "Added: {}",
                        if s.is_empty() { "(blank line)".into() } else { s.clone() }
                    )),
                    LineChg::Removed(s) => widget::text::body(format!(
                        "Removed: {}",
                        if s.is_empty() { "(blank line)".into() } else { s.clone() }
                    )),
                });
            }
        }
        body = body.push(crate::smooth_scroll::vertical(lines, Length::Fixed(240.0)));
    } else {
        body = body.push(widget::text::caption("This is the first saved copy of the note."));
    }
    dialog
        .control(body)
        .primary_action(
            widget::button::suggested("Restore this version").on_press(Message::Restore(snap.seq)),
        )
        .max_height(620.0)
        .into()
}

#[cfg(test)]
mod tests {
    use super::{body_delta, change_caption, line_changes, LineChg};
    use notes_sync::Snapshot;

    fn snap(seq: u64, title: &str, body: &str) -> Snapshot {
        Snapshot {
            note_id: 1,
            seq,
            title: title.into(),
            body: body.into(),
            hash: String::new(),
            created_at: 0,
        }
    }

    #[test]
    fn first_version_is_created() {
        assert_eq!(change_caption(None, &snap(1, "A", "hi")), "Started this note");
    }

    #[test]
    fn added_line_shows_plus() {
        assert_eq!(body_delta("hi", "hi\n# Interesting"), "Added: Interesting");
        let chg = line_changes("hi", "hi\nthere");
        assert!(chg.iter().any(|c| matches!(c, LineChg::Added(s) if s == "there")));
        assert!(chg.iter().any(|c| matches!(c, LineChg::Same(s) if s == "hi")));
    }

    #[test]
    fn removed_line_shows_minus() {
        assert_eq!(body_delta("hi\nthere", "hi"), "Removed: there");
    }

    #[test]
    fn nav_width_clamps() {
        assert_eq!(super::clamp_nav_width(100.0), super::NAV_WIDTH_MIN);
        assert_eq!(super::clamp_nav_width(900.0), super::NAV_WIDTH_MAX);
        assert_eq!(super::clamp_nav_width(300.0), 300.0);
    }

    #[test]
    fn rename_without_repeating_unchanged_title() {
        let older = snap(1, "Old", "same");
        let newer = snap(2, "New", "same");
        assert_eq!(
            change_caption(Some(&older), &newer),
            "Changed the title to New"
        );
    }
}
