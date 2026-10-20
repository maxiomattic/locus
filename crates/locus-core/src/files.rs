//! Files section: a directory browser over the catalog. Folders come from the
//! server's directory list plus every ancestor of every file's directory.

use std::collections::BTreeSet;

use crate::msg::{Effect, FileEntry, FilePatch, Msg, Nav};
use crate::state::{
    App, FileSort, FileSortKey, Focus, Input, InputKind, Section, clamp_sel, matches_filter,
};
use crate::update::step;

/// One row of the browser: folders first, then files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Folder { name: String, path: String },
    File(FileEntry),
}

impl Row {
    pub fn name(&self) -> &str {
        match self {
            Row::Folder { name, .. } => name,
            Row::File(f) => &f.name,
        }
    }
}

/// Parent of an absolute path. The parent of `/` is `/`.
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => path[..i].to_string(),
    }
}

/// `dir` joined with a single name, e.g. `/` + `a` gives `/a`.
pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

/// Normalise a user-typed directory: absolute, no `..`, no empty segments.
pub fn normalize_dir(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if !t.starts_with('/') {
        return Err("Directory must start with /".into());
    }
    let mut parts = Vec::new();
    for seg in t.split('/').filter(|s| !s.is_empty()) {
        if seg == ".." || seg == "." {
            return Err("Directory cannot contain . or ..".into());
        }
        parts.push(seg);
    }
    Ok(if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    })
}

fn valid_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("Name is required".into());
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err("Name cannot contain / or be . or ..".into());
    }
    Ok(name.to_string())
}

/// Every folder path the browser knows about, absolute and sorted.
pub fn folders(app: &App) -> Vec<String> {
    let mut all: BTreeSet<String> = BTreeSet::new();
    let add_with_ancestors = |path: &str, all: &mut BTreeSet<String>| {
        let mut p = path.to_string();
        while p != "/" && !p.is_empty() {
            all.insert(p.clone());
            p = parent(&p);
        }
    };
    for d in &app.files.dirs {
        if let Ok(d) = normalize_dir(d) {
            add_with_ancestors(&d, &mut all);
        }
    }
    for f in &app.files.files {
        add_with_ancestors(&f.dir, &mut all);
    }
    add_with_ancestors(&app.files.cwd, &mut all);
    all.into_iter().collect()
}

/// Rows of the current directory after the filter: subfolders, then files.
pub fn rows(app: &App) -> Vec<Row> {
    let cwd = app.files.cwd.as_str();
    let mut out: Vec<Row> = folders(app)
        .into_iter()
        .filter(|p| parent(p) == cwd && p != cwd)
        .map(|path| Row::Folder {
            name: path.rsplit('/').next().unwrap_or_default().to_string(),
            path,
        })
        .filter(|r| matches_filter(&app.filter, &[r.name()]))
        .collect();
    let mut files: Vec<FileEntry> = app
        .files
        .files
        .iter()
        .filter(|f| f.dir == cwd)
        .filter(|f| matches_filter(&app.filter, &[&f.name]))
        .cloned()
        .collect();
    let FileSort { key, descending } = app.files.sort;
    // Folders have no size or date: by name, reversed only when names are.
    if key == FileSortKey::Name && descending {
        out.reverse();
    }
    let by_name = |a: &FileEntry, b: &FileEntry| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    };
    files.sort_by(|a, b| {
        let order = match key {
            FileSortKey::Name => by_name(a, b),
            FileSortKey::Size => a.size.cmp(&b.size).then_with(|| by_name(a, b)),
            FileSortKey::Modified => a.modified.cmp(&b.modified).then_with(|| by_name(a, b)),
        };
        if descending { order.reverse() } else { order }
    });
    out.extend(files.into_iter().map(Row::File));
    out
}

/// Order by `key`; the column already in use flips direction. The selected
/// row stays selected.
pub fn sort_by(app: &mut App, key: FileSortKey) {
    let sort = app.files.sort;
    let descending = if sort.key == key {
        !sort.descending
    } else {
        false
    };
    resort(app, FileSort { key, descending });
}

/// Ascending or descending, keeping the column.
pub fn set_descending(app: &mut App, descending: bool) {
    let key = app.files.sort.key;
    resort(app, FileSort { key, descending });
}

fn resort(app: &mut App, sort: FileSort) {
    let keep = selected(app).map(|r| row_path(&r));
    app.files.sort = sort;
    if let Some(path) = keep {
        if let Some(i) = rows(app).iter().position(|r| row_path(r) == path) {
            app.files.sel = i;
        }
    }
}

pub fn selected(app: &App) -> Option<Row> {
    rows(app).into_iter().nth(app.files.sel)
}

/// The path of a row: the folder path, or the file's directory joined with its name.
pub fn row_path(row: &Row) -> String {
    match row {
        Row::Folder { path, .. } => path.clone(),
        Row::File(f) => join(&f.dir, &f.name),
    }
}

pub fn nav(app: &mut App, nav: Nav) {
    match nav {
        Nav::Left => go_up(app),
        Nav::Right => {
            if let Some(Row::Folder { path, .. }) = selected(app) {
                enter(app, path);
            }
        }
        other => {
            let len = rows(app).len();
            app.files.sel = step(app.files.sel, len, other);
        }
    }
}

/// Open a folder by path. Going up selects the folder we came from, as Back
/// does; anything else starts at the top.
pub fn open_dir(app: &mut App, path: &str) {
    let Ok(path) = normalize_dir(path) else {
        return;
    };
    if path == app.files.cwd {
        return;
    }
    let from = app.files.cwd.clone();
    app.files.cwd = path.clone();
    app.files.sel = 0;
    // `/a` from `/a/b/c`: select `b`.
    let prefix = if path == "/" {
        "/".to_string()
    } else {
        format!("{path}/")
    };
    if let Some(child) = from
        .strip_prefix(&prefix)
        .and_then(|rest| rest.split('/').next())
    {
        if let Some(i) = rows(app)
            .iter()
            .position(|r| matches!(r, Row::Folder { name, .. } if name == child))
        {
            app.files.sel = i;
        }
    }
}

fn enter(app: &mut App, path: String) {
    app.files.cwd = path;
    app.files.sel = 0;
}

fn go_up(app: &mut App) {
    if app.files.cwd == "/" {
        return;
    }
    let old = app.files.cwd.clone();
    let up = parent(&old);
    app.files.cwd = up;
    let name = old.rsplit('/').next().unwrap_or_default().to_string();
    app.files.sel = rows(app)
        .iter()
        .position(|r| matches!(r, Row::Folder { name: n, .. } if *n == name))
        .unwrap_or(0);
}

pub fn activate(app: &mut App, fx: &mut Vec<Effect>) {
    match selected(app) {
        Some(Row::Folder { path, .. }) => enter(app, path),
        Some(Row::File(f)) => fx.push(Effect::OpenBlob {
            sha256: f.sha256,
            name: f.name,
            mime: f.mime,
        }),
        None => app.info("Nothing selected"),
    }
}

pub fn yank(app: &mut App, fx: &mut Vec<Effect>) {
    match selected(app) {
        Some(row) => {
            let path = row_path(&row);
            app.info(format!("Copied {path}"));
            fx.push(Effect::CopyToClipboard(path));
        }
        None => app.info("Nothing selected"),
    }
}

pub fn begin_new_dir(app: &mut App) {
    app.input = Input::default();
    app.focus = Focus::Input(InputKind::NewDir);
}

pub fn submit_new_dir(app: &mut App, fx: &mut Vec<Effect>) {
    match valid_name(&app.input.text) {
        Ok(name) => {
            let path = join(&app.files.cwd, &name);
            app.input = Input::default();
            app.focus = Focus::List;
            fx.push(Effect::CreateDir { path });
        }
        Err(e) => app.error(e),
    }
}

pub fn dir_created(app: &mut App, path: String) {
    if !app.files.dirs.contains(&path) {
        app.files.dirs.push(path);
    }
    app.info("Created folder");
}

pub fn begin_rename(app: &mut App) {
    match selected(app) {
        Some(Row::File(f)) => {
            app.input = Input {
                text: f.name,
                target: Some(f.id),
            };
            app.focus = Focus::Input(InputKind::RenameFile);
        }
        Some(Row::Folder { name, path }) => {
            app.files.dir_edit = Some(path);
            app.input = Input {
                text: name,
                target: None,
            };
            app.focus = Focus::Input(InputKind::RenameDir);
        }
        None => app.info("Select a file or folder to rename"),
    }
}

/// Rename a folder: the new name stays in the same parent folder.
pub fn submit_rename_dir(app: &mut App, fx: &mut Vec<Effect>) {
    let name = match valid_name(&app.input.text) {
        Ok(n) => n,
        Err(e) => {
            app.error(e);
            return;
        }
    };
    let Some(from) = app.files.dir_edit.clone() else {
        app.focus = Focus::List;
        return;
    };
    let to = join(&parent(&from), &name);
    finish_dir_move(app, fx, from, to);
}

/// The Move prompt for a folder takes the full path it should have.
pub fn begin_move_dir(app: &mut App, path: String) {
    app.files.dir_edit = Some(path.clone());
    app.input = Input {
        text: path,
        target: None,
    };
    app.focus = Focus::Input(InputKind::MoveDir);
}

pub fn submit_move_dir(app: &mut App, fx: &mut Vec<Effect>) {
    let to = match normalize_dir(&app.input.text) {
        Ok(d) => d,
        Err(e) => {
            app.error(e);
            return;
        }
    };
    let Some(from) = app.files.dir_edit.clone() else {
        app.focus = Focus::List;
        return;
    };
    finish_dir_move(app, fx, from, to);
}

fn finish_dir_move(app: &mut App, fx: &mut Vec<Effect>, from: String, to: String) {
    app.input = Input::default();
    app.files.dir_edit = None;
    app.focus = Focus::List;
    if to == from {
        app.info("The folder already has that name");
    } else if to == "/" {
        app.error("A folder cannot be the root");
    } else if to.starts_with(&format!("{from}/")) {
        app.error("A folder cannot move into itself");
    } else {
        fx.push(Effect::MoveDir { from, to });
    }
}

/// A folder moved. A current folder inside it follows it to its new path.
pub fn dir_moved(app: &mut App, from: &str, to: &str) {
    let under = |p: &str| p == from || p.starts_with(&format!("{from}/"));
    if under(&app.files.cwd) {
        app.files.cwd = format!("{to}{}", &app.files.cwd[from.len()..]);
        app.files.sel = 0;
    }
    app.info(format!("Moved folder to {to}"));
}

pub fn submit_rename(app: &mut App, fx: &mut Vec<Effect>) {
    let name = match valid_name(&app.input.text) {
        Ok(n) => n,
        Err(e) => {
            app.error(e);
            return;
        }
    };
    let Some(id) = app.input.target else {
        app.focus = Focus::List;
        return;
    };
    let Some(file) = app.files.files.iter().find(|f| f.id == id).cloned() else {
        app.error("File no longer exists");
        return;
    };
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::PatchFile {
        id,
        expected_revision: file.revision,
        patch: FilePatch {
            name: Some(name),
            dir: None,
        },
    });
}

pub fn begin_move(app: &mut App) {
    match selected(app) {
        Some(Row::File(f)) => {
            app.input = Input {
                text: app.files.cwd.clone(),
                target: Some(f.id),
            };
            app.focus = Focus::Input(InputKind::MoveFile);
        }
        Some(Row::Folder { path, .. }) => begin_move_dir(app, path),
        None => app.info("Select a file or folder to move"),
    }
}

pub fn submit_move(app: &mut App, fx: &mut Vec<Effect>) {
    let dir = match normalize_dir(&app.input.text) {
        Ok(d) => d,
        Err(e) => {
            app.error(e);
            return;
        }
    };
    let Some(id) = app.input.target else {
        app.focus = Focus::List;
        return;
    };
    let Some(file) = app.files.files.iter().find(|f| f.id == id).cloned() else {
        app.error("File no longer exists");
        return;
    };
    app.input = Input::default();
    app.focus = Focus::List;
    fx.push(Effect::PatchFile {
        id,
        expected_revision: file.revision,
        patch: FilePatch {
            name: None,
            dir: Some(dir),
        },
    });
}

/// Rename a file chosen in another list (Movies, Music).
pub fn begin_rename_file(app: &mut App, f: &FileEntry) {
    app.input = Input {
        text: f.name.clone(),
        target: Some(f.id),
    };
    app.focus = Focus::Input(InputKind::RenameFile);
}

/// Move a file chosen in another list (Movies, Music) to the folder typed in.
pub fn begin_move_file(app: &mut App, f: &FileEntry) {
    app.input = Input {
        text: f.dir.clone(),
        target: Some(f.id),
    };
    app.focus = Focus::Input(InputKind::MoveFile);
}

/// Delete a file chosen in another list (Movies, Music). Asks first.
pub fn ask_delete_file(app: &mut App, f: &FileEntry) {
    app.ask(
        format!("Delete \"{}\"?", f.name),
        Msg::ExecuteDelete {
            section: Section::Files,
            id: f.id,
        },
    );
}

pub fn ask_delete(app: &mut App) {
    match selected(app) {
        Some(Row::File(f)) => app.ask(
            format!("Delete \"{}\"?", f.name),
            Msg::ExecuteDelete {
                section: Section::Files,
                id: f.id,
            },
        ),
        Some(Row::Folder { name, path }) => {
            if dir_is_empty(app, &path) {
                app.ask(
                    format!("Delete empty folder \"{name}\"?"),
                    Msg::ExecuteDeleteDir { path },
                );
            } else {
                app.info(format!("Folder \"{name}\" is not empty"));
            }
        }
        None => app.info("Nothing selected"),
    }
}

/// True when no file and no other folder lies under `path`.
pub fn dir_is_empty(app: &App, path: &str) -> bool {
    let below = |p: &str| p.starts_with(&format!("{}/", path.trim_end_matches('/')));
    !app.files
        .files
        .iter()
        .any(|f| f.dir == path || below(&f.dir))
        && !folders(app).iter().any(|d| below(d))
}

pub fn execute_delete_dir(app: &mut App, fx: &mut Vec<Effect>, path: String) {
    fx.push(Effect::DeleteDir { path });
    app.info("Deleting folder");
}

pub fn dir_deleted(app: &mut App, path: &str) {
    app.files
        .dirs
        .retain(|d| normalize_dir(d).as_deref() != Ok(path));
    app.files.sel = clamp_sel(app.files.sel, rows(app).len());
    app.info("Folder deleted");
}

pub fn execute_delete(app: &mut App, fx: &mut Vec<Effect>, id: u64) {
    fx.push(Effect::DeleteFile { id });
    app.info("Deleting file");
}

pub fn files_loaded(app: &mut App, files: Vec<FileEntry>) {
    app.files.files = files;
    // Movies and Music read the same catalog, so its arrival ends their loads too.
    for section in [Section::Files, Section::Movies, Section::Music] {
        app.loading[section.index()] = false;
        app.loaded[section.index()] = true;
    }
    app.files.sel = clamp_sel(app.files.sel, rows(app).len());
}

pub fn dirs_loaded(app: &mut App, dirs: Vec<String>) {
    app.files.dirs = dirs;
    app.files.sel = clamp_sel(app.files.sel, rows(app).len());
}

pub fn file_patched(app: &mut App, file: FileEntry) {
    let id = file.id;
    match app.files.files.iter_mut().find(|f| f.id == id) {
        Some(slot) => *slot = file,
        None => app.files.files.push(file),
    }
    app.files.sel = clamp_sel(app.files.sel, rows(app).len());
    app.info("Updated file");
}

pub fn file_deleted(app: &mut App, id: u64) {
    app.files.files.retain(|f| f.id != id);
    app.files.sel = clamp_sel(app.files.sel, rows(app).len());
    app.info("File deleted");
}

/// Enter the file's directory with the file selected (used by the palette).
pub fn select_file(app: &mut App, id: u64) {
    let Some(dir) = app
        .files
        .files
        .iter()
        .find(|f| f.id == id)
        .map(|f| f.dir.clone())
    else {
        app.info("File not found");
        return;
    };
    app.files.cwd = dir;
    app.files.sel = rows(app)
        .iter()
        .position(|r| matches!(r, Row::File(f) if f.id == id))
        .unwrap_or(0);
    app.focus = Focus::List;
}
