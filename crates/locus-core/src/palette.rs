//! Command palette: fuzzy search over section switches, "new" commands, and
//! note, file and contact names. Enter runs the top match.

use locus_client::contacts::Contact;

use crate::msg::{Msg, Nav};
use crate::state::{App, Focus, PaletteState, Section, Target};
use crate::update::load_section;
use crate::msg::Effect;
use crate::update::step;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    /// Short category shown beside the label, e.g. `section` or `note`.
    pub hint: &'static str,
    pub action: Msg,
}

/// All palette entries in their default order. In a window locked to one
/// section, entries for other sections open that section's own window (the
/// core turns them into [`Effect::OpenSection`]).
pub fn items(app: &App) -> Vec<Item> {
    let elsewhere = |s: Section| app.locked.is_some_and(|l| l != s);
    let open = |section: Section, target: Target| Msg::Open {
        section,
        target: Some(target),
    };
    let mut out: Vec<Item> = Section::ALL
        .into_iter()
        .map(|s| Item {
            label: format!("Go to {}", s.label()),
            hint: if elsewhere(s) { "app" } else { "section" },
            action: Msg::Switch(s),
        })
        .collect();
    out.extend([
        Item {
            label: "New note".into(),
            hint: "command",
            action: open(Section::Notes, Target::New),
        },
        Item {
            label: "New event".into(),
            hint: "command",
            action: open(Section::Calendar, Target::New),
        },
        Item {
            label: "New folder".into(),
            hint: "command",
            action: open(Section::Files, Target::New),
        },
        Item {
            label: "New contact".into(),
            hint: "command",
            action: open(Section::Contacts, Target::New),
        },
        Item {
            label: "Upload photos".into(),
            hint: "command",
            action: open(Section::Photos, Target::Upload),
        },
        Item {
            label: "Reload section".into(),
            hint: "command",
            action: Msg::Reload,
        },
        Item {
            label: "Log in".into(),
            hint: "command",
            action: Msg::BeginLogin,
        },
    ]);
    out.extend(app.notes.items.iter().map(|n| Item {
        label: n.title.clone(),
        hint: "note",
        action: Msg::Goto {
            section: Section::Notes,
            id: n.id,
        },
    }));
    out.extend(app.files.files.iter().map(|f| Item {
        label: f.name.clone(),
        hint: "file",
        action: Msg::Goto {
            section: Section::Files,
            id: f.id,
        },
    }));
    out.extend(app.contacts.items.iter().map(|c: &Contact| Item {
        label: c.name.clone(),
        hint: "contact",
        action: Msg::Goto {
            section: Section::Contacts,
            id: c.id,
        },
    }));
    out
}

/// Entries matching the palette query, best first. Ties keep default order.
pub fn ranked(app: &App) -> Vec<Item> {
    let query = app.palette.query.trim();
    let mut scored: Vec<(u32, usize, Item)> = items(app)
        .into_iter()
        .enumerate()
        .filter_map(|(i, item)| fuzzy_score(query, &item.label).map(|s| (s, i, item)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, item)| item).collect()
}

/// Score `text` against `query` (case-insensitive). Higher is better; `None`
/// when the query is not a subsequence of the text.
///
/// Prefix matches score highest, then substrings (earlier is better), then
/// scattered subsequences (tighter is better).
pub fn fuzzy_score(query: &str, text: &str) -> Option<u32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    let t = text.to_lowercase();
    if t.starts_with(&q) {
        return Some(1000);
    }
    if let Some(pos) = t.find(&q) {
        let at = t[..pos].chars().count() as u32;
        return Some(800u32.saturating_sub(at.min(500)));
    }
    let mut want = q.chars().peekable();
    let mut first = None;
    let mut last = 0usize;
    for (i, c) in t.chars().enumerate() {
        if want.peek() == Some(&c) {
            first.get_or_insert(i);
            last = i;
            want.next();
        }
    }
    if want.peek().is_some() {
        return None;
    }
    let span = (last - first.unwrap_or(0)) as u32;
    Some(400u32.saturating_sub(span.min(300)))
}

/// Open the palette. A locked window has only loaded its own section, so it
/// also fetches the lists the palette searches (notes, files, contacts).
pub fn open(app: &mut App, fx: &mut Vec<Effect>) {
    app.palette = PaletteState::default();
    app.focus = Focus::Palette;
    if app.locked.is_some() {
        for s in [Section::Notes, Section::Files, Section::Contacts] {
            if !app.loaded[s.index()] {
                load_section(app, s, fx);
            }
        }
    }
}

pub fn move_sel(app: &mut App, nav: Nav) {
    let len = ranked(app).len();
    app.palette.sel = step(app.palette.sel, len, nav);
}

/// The action of the highlighted entry, if any matches.
pub fn selected_action(app: &App) -> Option<Msg> {
    let list = ranked(app);
    let idx = app.palette.sel.min(list.len().saturating_sub(1));
    list.get(idx).map(|i| i.action.clone())
}
