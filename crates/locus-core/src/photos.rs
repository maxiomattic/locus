//! Photos section: a grid of catalog entries and a single-photo viewer.

use locus_client::photos::Photo;

use crate::msg::{Effect, Msg, Nav};
use crate::state::{App, Focus, Input, InputKind, Section, clamp_sel, matches_filter};

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

fn selected(app: &App) -> Option<Photo> {
    let sel = clamp_sel(app.photos.sel, visible(app).len());
    visible(app).get(sel).map(|p| (*p).clone())
}

pub fn begin_rename(app: &mut App) {
    match selected(app) {
        Some(p) => {
            app.input = Input {
                text: p.title,
                target: Some(p.id),
            };
            app.focus = Focus::Input(InputKind::RenamePhoto);
        }
        None => app.info("No photo selected"),
    }
}

/// The album prompt shows the album as a path, `/` for none.
pub fn begin_move(app: &mut App) {
    match selected(app) {
        Some(p) => {
            app.input = Input {
                text: format!("/{}", p.album.trim_matches('/')),
                target: Some(p.id),
            };
            app.focus = Focus::Input(InputKind::MovePhoto);
        }
        None => app.info("No photo selected"),
    }
}

pub fn submit_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let title = app.input.text.trim().to_string();
    if title.is_empty() {
        app.error("Title is required");
        return;
    }
    submit(app, fx, Some(title), None);
}

pub fn submit_move(app: &mut App, fx: &mut Vec<Effect>) {
    let album = app.input.text.trim().trim_matches('/').to_string();
    if album.split('/').any(|s| s == "..") {
        app.error("Album cannot contain ..");
        return;
    }
    submit(app, fx, None, Some(album));
}

fn submit(app: &mut App, fx: &mut Vec<Effect>, title: Option<String>, album: Option<String>) {
    let Some(id) = app.input.target else {
        app.focus = Focus::List;
        return;
    };
    let Some(photo) = app.photos.items.iter().find(|p| p.id == id) else {
        app.error("Photo no longer exists");
        return;
    };
    fx.push(Effect::PatchPhoto {
        id,
        expected_revision: photo.revision,
        title,
        album,
    });
    app.input = Input::default();
    app.focus = Focus::List;
}

pub fn patched(app: &mut App, photo: Photo) {
    match app.photos.items.iter_mut().find(|p| p.id == photo.id) {
        Some(slot) => *slot = photo,
        None => app.photos.items.push(photo),
    }
    app.photos.sel = clamp_sel(app.photos.sel, visible(app).len());
    app.info("Updated photo");
}
