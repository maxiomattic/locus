//! Photos section: a grid of catalog entries and a single-photo viewer.

use locus_client::photos::Photo;

use crate::msg::{Effect, Msg, Nav};
use crate::state::{App, Focus, Section, clamp_sel, matches_filter};

/// The content hash when `path` is `sha256:<hex>`; `None` for other paths.
pub fn hash_of(path: &str) -> Option<&str> {
    let hex = path.strip_prefix("sha256:")?;
    (!hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// Photos in the filter, in catalog order. Grid indexes refer to this list.
pub fn visible(app: &App) -> Vec<&Photo> {
    app.photos
        .items
        .iter()
        .filter(|p| matches_filter(&app.filter, &[&p.title, &p.path]))
        .collect()
}

pub fn nav(app: &mut App, nav: Nav) {
    let len = visible(app).len();
    let cols = app.photos.columns.max(1);
    let sel = clamp_sel(app.photos.sel, len);
    if app.focus == Focus::Detail {
        // The viewer steps one photo at a time and does not wrap.
        app.photos.sel = match nav {
            Nav::Left => sel.saturating_sub(1),
            Nav::Right => (sel + 1).min(len.saturating_sub(1)),
            _ => sel,
        };
        return;
    }
    let last = len.saturating_sub(1);
    app.photos.sel = match nav {
        Nav::Left => sel.saturating_sub(1),
        Nav::Right => (sel + 1).min(last),
        Nav::Up => sel.saturating_sub(cols),
        Nav::Down => (sel + cols).min(last),
        Nav::HalfUp => sel.saturating_sub(cols * 2),
        Nav::HalfDown => (sel + cols * 2).min(last),
        Nav::Top => 0,
        Nav::Bottom => last,
    };
}

pub fn open_viewer(app: &mut App) {
    if visible(app).is_empty() {
        app.info("No photos");
        return;
    }
    app.focus = Focus::Detail;
}

pub fn loaded(app: &mut App, photos: Vec<Photo>) {
    app.photos.items = photos;
    app.loading[Section::Photos.index()] = false;
    app.loaded[Section::Photos.index()] = true;
    app.photos.sel = clamp_sel(app.photos.sel, visible(app).len());
}

pub fn ask_delete(app: &mut App) {
    let sel = clamp_sel(app.photos.sel, visible(app).len());
    let Some((id, title)) = visible(app).get(sel).map(|p| (p.id, p.title.clone())) else {
        app.info("No photo selected");
        return;
    };
    app.ask(
        format!("Delete photo \"{title}\"?"),
        Msg::ExecuteDelete {
            section: Section::Photos,
            id,
        },
    );
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    fx.push(Effect::DeletePhoto { id });
    app.info("Deleting photo");
}

pub fn deleted(app: &mut App, id: u64) {
    app.photos.items.retain(|p| p.id != id);
    let len = visible(app).len();
    app.photos.sel = clamp_sel(app.photos.sel, len);
    if len == 0 && app.focus == Focus::Detail {
        app.focus = Focus::List;
    }
    app.info("Photo deleted");
}
