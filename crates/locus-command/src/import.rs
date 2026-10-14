//! Idempotent Obsidian vault importer.
//!
//! Markdown files become notes (lossless: body is the file verbatim), every
//! other file goes to `%files`. Nothing is ever overwritten: an entry that
//! exists with different content is reported as a conflict. The decision logic
//! is [`plan`], a pure function from the vault listing and existing state to
//! a list of [`Action`]s; [`run`] gathers state, plans and (unless dry) applies.

use locus_client::Client;
use locus_client::files::FileEntry;
use locus_client::notes::{Folder, Note};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use url::Url;

/// What the CLI was asked to do.
pub struct Options {
    pub vault: PathBuf,
    pub url: String,
    pub code_file: PathBuf,
    pub notebook: String,
    pub files_dir: Option<String>,
    pub dry_run: bool,
}

/// One file in the vault. `dir` holds the path components below the vault root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultItem {
    pub dir: Vec<String>,
    pub name: String,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A `*.md` file: its text.
    Note(String),
    /// Anything else: lowercase hex SHA-256 of its bytes.
    Binary { sha256: String },
}

/// What is already on the ship, as far as the plan cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Existing {
    /// The notebook's root folder id.
    pub root: u64,
    pub folders: Vec<ExistingFolder>,
    pub notes: Vec<ExistingNote>,
    pub files: Vec<ExistingFile>,
    /// Absolute `%files` folder paths.
    pub dirs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingFolder {
    pub id: u64,
    pub parent: Option<u64>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingNote {
    pub folder: u64,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingFile {
    pub dir: String,
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Create,
    Skip,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A notebook folder; `id` is set when it already exists.
    Folder { path: Vec<String>, id: Option<u64> },
    /// A note in the folder at `dir`, from vault file `file`.
    Note {
        dir: Vec<String>,
        file: String,
        title: String,
        outcome: Outcome,
    },
    /// A `%files` folder (absolute path).
    Dir { path: String, exists: bool },
    /// A `%files` entry in `abs_dir`, from vault file `rel_dir/name`.
    File {
        rel_dir: Vec<String>,
        abs_dir: String,
        name: String,
        mime: String,
        outcome: Outcome,
    },
}

impl Action {
    fn outcome(&self) -> Outcome {
        match self {
            Action::Folder { id, .. } => exists_outcome(id.is_some()),
            Action::Dir { exists, .. } => exists_outcome(*exists),
            Action::Note { outcome, .. } | Action::File { outcome, .. } => *outcome,
        }
    }

    /// One-line description, e.g. `note   create  School/Maths.md`.
    pub fn describe(&self) -> String {
        let (kind, what) = match self {
            Action::Folder { path, .. } => ("folder", path.join("/")),
            Action::Note { dir, file, .. } => ("note", join_rel(dir, file)),
            Action::Dir { path, .. } => ("dir", path.clone()),
            Action::File { rel_dir, name, .. } => ("file", join_rel(rel_dir, name)),
        };
        let verb = match self.outcome() {
            Outcome::Create => "create",
            Outcome::Skip => "skip",
            Outcome::Conflict => "CONFLICT (differs on ship, not overwritten)",
        };
        format!("{kind:<7}{verb:<8}{what}")
    }
}

fn exists_outcome(exists: bool) -> Outcome {
    if exists {
        Outcome::Skip
    } else {
        Outcome::Create
    }
}

fn join_rel(dir: &[String], name: &str) -> String {
    let mut s = dir.join("/");
    if !s.is_empty() {
        s.push('/');
    }
    s + name
}

/// Created / skipped / conflict counts for one kind of thing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub created: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

impl Counts {
    fn add(&mut self, o: Outcome) {
        match o {
            Outcome::Create => self.created += 1,
            Outcome::Skip => self.skipped += 1,
            Outcome::Conflict => self.conflicts += 1,
        }
    }
}

/// `(notes, folders, files)`; folders cover notebook folders and `%files` dirs.
pub fn summarize(actions: &[Action]) -> (Counts, Counts, Counts) {
    let (mut notes, mut folders, mut files) = Default::default();
    for a in actions {
        match a {
            Action::Note { .. } => Counts::add(&mut notes, a.outcome()),
            Action::File { .. } => Counts::add(&mut files, a.outcome()),
            _ => Counts::add(&mut folders, a.outcome()),
        }
    }
    (notes, folders, files)
}

/// Decide what to do. `files_dir` is the absolute `%files` folder that mirrors
/// the vault root (no trailing slash). Folders and dirs are emitted parent
/// first, and only when something goes into them.
pub fn plan(items: &[VaultItem], files_dir: &str, ex: &Existing) -> Vec<Action> {
    let mut sorted: Vec<&VaultItem> = items.iter().collect();
    sorted.sort_by(|a, b| (&a.dir, &a.name).cmp(&(&b.dir, &b.name)));

    let mut folder_ids: HashMap<Vec<String>, Option<u64>> = HashMap::new();
    folder_ids.insert(Vec::new(), Some(ex.root));
    let known_dirs: HashSet<&str> = ex.dirs.iter().map(String::as_str).collect();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    let mut actions = Vec::new();

    for item in sorted {
        match &item.content {
            Content::Note(body) => {
                for n in 1..=item.dir.len() {
                    let path = &item.dir[..n];
                    if !folder_ids.contains_key(path) {
                        let parent = folder_ids[&path[..n - 1]];
                        let id = parent.and_then(|p| {
                            ex.folders
                                .iter()
                                .find(|f| f.parent == Some(p) && f.name == path[n - 1])
                                .map(|f| f.id)
                        });
                        folder_ids.insert(path.to_vec(), id);
                        actions.push(Action::Folder {
                            path: path.to_vec(),
                            id,
                        });
                    }
                }
                let title = item
                    .name
                    .rsplit_once('.')
                    .map_or(item.name.as_str(), |(stem, _)| stem)
                    .to_string();
                let folder = folder_ids[&item.dir[..]];
                let mut same_title = ex
                    .notes
                    .iter()
                    .filter(|n| Some(n.folder) == folder && n.title == title)
                    .peekable();
                let outcome = if same_title.peek().is_none() {
                    Outcome::Create
                } else if same_title.any(|n| n.body == *body) {
                    Outcome::Skip
                } else {
                    Outcome::Conflict
                };
                actions.push(Action::Note {
                    dir: item.dir.clone(),
                    file: item.name.clone(),
                    title,
                    outcome,
                });
            }
            Content::Binary { sha256 } => {
                let mut chain = vec![files_dir.to_string()];
                for part in &item.dir {
                    chain.push(format!("{}/{part}", chain.last().unwrap()));
                }
                // Ancestors of `files_dir` itself, outermost first.
                let mut ancestors = Vec::new();
                let mut acc = String::new();
                for part in files_dir.split('/').filter(|p| !p.is_empty()) {
                    acc = format!("{acc}/{part}");
                    ancestors.push(acc.clone());
                }
                ancestors.extend(chain.iter().skip(1).cloned());
                for path in ancestors {
                    if seen_dirs.insert(path.clone()) {
                        let exists = known_dirs.contains(path.as_str());
                        actions.push(Action::Dir { path, exists });
                    }
                }
                let abs_dir = chain.last().unwrap().clone();
                let mut same_name = ex
                    .files
                    .iter()
                    .filter(|f| f.dir == abs_dir && f.name == item.name)
                    .peekable();
                let outcome = if same_name.peek().is_none() {
                    Outcome::Create
                } else if same_name.any(|f| f.sha256 == *sha256) {
                    Outcome::Skip
                } else {
                    Outcome::Conflict
                };
                actions.push(Action::File {
                    rel_dir: item.dir.clone(),
                    abs_dir,
                    name: item.name.clone(),
                    mime: mime_for(&item.name).into(),
                    outcome,
                });
            }
        }
    }
    actions
}

/// Mime type from the file extension.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e).to_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "json" | "canvas" => "application/json",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Walk `vault`, skipping dot-entries and symlinks. Read only.
pub fn scan(vault: &Path) -> Result<Vec<VaultItem>, String> {
    let mut items = Vec::new();
    scan_dir(vault, &mut Vec::new(), &mut items)?;
    Ok(items)
}

fn scan_dir(dir: &Path, rel: &mut Vec<String>, out: &mut Vec<VaultItem>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    let mut entries: Vec<_> = entries
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading {}: {e}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            eprintln!("warning: skipping non-UTF-8 name in {}", dir.display());
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|e| format!("{}: {e}", entry.path().display()))?;
        if kind.is_dir() {
            rel.push(name);
            scan_dir(&entry.path(), rel, out)?;
            rel.pop();
        } else if kind.is_file() {
            let path = entry.path();
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let is_md = name.to_lowercase().ends_with(".md");
            let content = if is_md {
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("{}: not valid UTF-8", path.display()))?;
                Content::Note(text)
            } else {
                Content::Binary {
                    sha256: sha256_hex(&bytes),
                }
            };
            out.push(VaultItem {
                dir: rel.clone(),
                name,
                content,
            });
        }
    }
    Ok(())
}

fn normalize_dir(s: &str) -> String {
    format!("/{}", s.trim_matches('/'))
}

/// Run the import. Prints one line per action and a summary.
pub fn run(opts: Options) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_async(opts))
}

async fn run_async(opts: Options) -> Result<(), String> {
    let items = scan(&opts.vault)?;
    let code = std::fs::read_to_string(&opts.code_file)
        .map_err(|e| format!("reading code file {}: {e}", opts.code_file.display()))?;
    let base = Url::parse(&opts.url).map_err(|e| format!("bad --url: {e}"))?;
    let client = Client::new(base).map_err(|e| e.to_string())?;
    client
        .login(code.trim())
        .await
        .map_err(|e| format!("login failed: {e}"))?;

    let files_dir = normalize_dir(
        &opts
            .files_dir
            .clone()
            .unwrap_or_else(|| format!("/obsidian/{}", opts.notebook)),
    );

    let notebooks = client
        .list_notebooks()
        .await
        .map_err(|e| format!("listing notebooks: {e}"))?;
    let mut nb = notebooks
        .into_iter()
        .find(|n| n.notebook.title == opts.notebook);
    if nb.is_none() {
        println!("notebook create  {}", opts.notebook);
        if !opts.dry_run {
            nb = Some(
                client
                    .create_notebook(&opts.notebook)
                    .await
                    .map_err(|e| format!("creating notebook: {e}"))?,
            );
        }
    }

    let mut ex = Existing::default();
    if let Some(nb) = &nb {
        let (host, name) = (nb.host.as_str(), nb.flag_name.as_str());
        ex.root = nb.notebook.root_folder_id;
        let folders = client
            .list_folders(host, name)
            .await
            .map_err(|e| format!("listing folders: {e}"))?;
        ex.folders = folders.into_iter().map(existing_folder).collect();
        let notes = client
            .list_notes(host, name)
            .await
            .map_err(|e| format!("listing notes: {e}"))?;
        ex.notes = notes.into_iter().map(existing_note).collect();
    }
    ex.files = client
        .list_files(None)
        .await
        .map_err(|e| format!("listing files: {e}"))?
        .into_iter()
        .map(existing_file)
        .collect();
    ex.dirs = client
        .list_dirs()
        .await
        .map_err(|e| format!("listing file dirs: {e}"))?;

    let actions = plan(&items, &files_dir, &ex);
    let mut folder_ids: HashMap<Vec<String>, u64> = HashMap::new();
    folder_ids.insert(Vec::new(), ex.root);

    for action in &actions {
        println!("{}", action.describe());
        if opts.dry_run {
            continue;
        }
        apply(&client, nb.as_ref(), &opts.vault, action, &mut folder_ids)
            .await
            .map_err(|e| format!("{}: {e}", action.describe().trim_end()))?;
    }

    let (notes, folders, files) = summarize(&actions);
    let mode = if opts.dry_run {
        "dry run, nothing written: "
    } else {
        ""
    };
    println!("{mode}summary");
    for (label, c) in [("notes", notes), ("folders", folders), ("files", files)] {
        println!(
            "  {label:<8}{} created, {} skipped, {} conflicts",
            c.created, c.skipped, c.conflicts
        );
    }
    Ok(())
}

fn existing_folder(f: Folder) -> ExistingFolder {
    ExistingFolder {
        id: f.id,
        parent: f.parent_folder_id,
        name: f.name,
    }
}

fn existing_note(n: Note) -> ExistingNote {
    ExistingNote {
        folder: n.folder_id,
        title: n.title,
        body: n.body_md,
    }
}

fn existing_file(f: FileEntry) -> ExistingFile {
    ExistingFile {
        dir: f.dir,
        name: f.name,
        sha256: f.sha256,
    }
}

async fn apply(
    client: &Client,
    nb: Option<&locus_client::notes::NotebookSummary>,
    vault: &Path,
    action: &Action,
    folder_ids: &mut HashMap<Vec<String>, u64>,
) -> Result<(), String> {
    let src = |dir: &[String], name: &str| {
        let mut p = vault.to_path_buf();
        p.extend(dir);
        p.join(name)
    };
    let nb = || nb.ok_or("notebook missing");
    match action {
        Action::Folder { path, id } => {
            let id = match id {
                Some(id) => *id,
                None => {
                    let nb = nb()?;
                    let parent = folder_ids[&path[..path.len() - 1]];
                    client
                        .create_folder(&nb.host, &nb.flag_name, parent, &path[path.len() - 1])
                        .await
                        .map_err(|e| e.to_string())?
                        .id
                }
            };
            folder_ids.insert(path.clone(), id);
        }
        Action::Note {
            dir,
            file,
            title,
            outcome: Outcome::Create,
        } => {
            let nb = nb()?;
            let body = std::fs::read_to_string(src(dir, file)).map_err(|e| e.to_string())?;
            client
                .create_note(&nb.host, &nb.flag_name, folder_ids[dir], title, &body)
                .await
                .map_err(|e| e.to_string())?;
        }
        Action::Dir {
            path,
            exists: false,
        } => {
            client.create_dir(path).await.map_err(|e| e.to_string())?;
        }
        Action::File {
            rel_dir,
            abs_dir,
            name,
            mime,
            outcome: Outcome::Create,
        } => {
            let bytes = std::fs::read(src(rel_dir, name)).map_err(|e| e.to_string())?;
            client
                .upload_file(abs_dir, name, mime, bytes)
                .await
                .map_err(|e| e.to_string())?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(dir: &[&str], name: &str, body: &str) -> VaultItem {
        VaultItem {
            dir: dir.iter().map(|s| s.to_string()).collect(),
            name: name.into(),
            content: Content::Note(body.into()),
        }
    }

    fn bin(dir: &[&str], name: &str, sha: &str) -> VaultItem {
        VaultItem {
            dir: dir.iter().map(|s| s.to_string()).collect(),
            name: name.into(),
            content: Content::Binary { sha256: sha.into() },
        }
    }

    fn ex() -> Existing {
        Existing {
            root: 1,
            ..Default::default()
        }
    }

    fn outcomes(actions: &[Action]) -> Vec<Outcome> {
        actions.iter().map(Action::outcome).collect()
    }

    #[test]
    fn new_vault_creates_everything() {
        let items = [note(&[], "A.md", "a"), note(&[], "B.md", "b")];
        let acts = plan(&items, "/obsidian/T", &ex());
        assert_eq!(outcomes(&acts), [Outcome::Create, Outcome::Create]);
        assert!(matches!(&acts[0], Action::Note { title, .. } if title == "A"));
    }

    #[test]
    fn rerun_is_all_skips() {
        let items = [
            note(&["School"], "Maths.md", "x"),
            bin(&["School"], "p.png", "abc"),
        ];
        let e = Existing {
            root: 1,
            folders: vec![ExistingFolder {
                id: 5,
                parent: Some(1),
                name: "School".into(),
            }],
            notes: vec![ExistingNote {
                folder: 5,
                title: "Maths".into(),
                body: "x".into(),
            }],
            files: vec![ExistingFile {
                dir: "/obsidian/T/School".into(),
                name: "p.png".into(),
                sha256: "abc".into(),
            }],
            dirs: vec![
                "/".into(),
                "/obsidian".into(),
                "/obsidian/T".into(),
                "/obsidian/T/School".into(),
            ],
        };
        let acts = plan(&items, "/obsidian/T", &e);
        assert!(!acts.is_empty());
        assert!(outcomes(&acts).iter().all(|o| *o == Outcome::Skip));
        let (n, f, fl) = summarize(&acts);
        assert_eq!((n.created, f.created, fl.created), (0, 0, 0));
    }

    #[test]
    fn edited_note_is_conflict() {
        let e = Existing {
            root: 1,
            notes: vec![ExistingNote {
                folder: 1,
                title: "A".into(),
                body: "edited".into(),
            }],
            ..Default::default()
        };
        let acts = plan(&[note(&[], "A.md", "orig")], "/o", &e);
        assert_eq!(outcomes(&acts), [Outcome::Conflict]);
        assert_eq!(summarize(&acts).0.conflicts, 1);
    }

    #[test]
    fn changed_binary_is_conflict() {
        let e = Existing {
            root: 1,
            files: vec![ExistingFile {
                dir: "/o".into(),
                name: "p.png".into(),
                sha256: "old".into(),
            }],
            dirs: vec!["/o".into()],
            ..Default::default()
        };
        let acts = plan(&[bin(&[], "p.png", "new")], "/o", &e);
        assert_eq!(outcomes(&acts), [Outcome::Skip, Outcome::Conflict]);
    }

    #[test]
    fn nested_folders_are_created_parent_first_once() {
        let items = [
            note(&["a", "b"], "X.md", "1"),
            note(&["a", "b"], "Y.md", "2"),
            note(&["a"], "Z.md", "3"),
        ];
        let acts = plan(&items, "/o", &ex());
        let folders: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Action::Folder { path, id } => Some((path.join("/"), *id)),
                _ => None,
            })
            .collect();
        assert_eq!(
            folders,
            [("a".to_string(), None), ("a/b".to_string(), None)]
        );
        assert_eq!(acts.len(), 2 + 3);
    }

    #[test]
    fn existing_nested_folder_is_reused_and_notes_look_inside_it() {
        let e = Existing {
            root: 1,
            folders: vec![
                ExistingFolder {
                    id: 2,
                    parent: Some(1),
                    name: "a".into(),
                },
                ExistingFolder {
                    id: 3,
                    parent: Some(2),
                    name: "b".into(),
                },
            ],
            notes: vec![ExistingNote {
                folder: 3,
                title: "X".into(),
                body: "1".into(),
            }],
            ..Default::default()
        };
        let acts = plan(&[note(&["a", "b"], "X.md", "1")], "/o", &e);
        assert_eq!(
            outcomes(&acts),
            [Outcome::Skip, Outcome::Skip, Outcome::Skip]
        );
    }

    #[test]
    fn same_title_in_other_folder_is_not_a_match() {
        let e = Existing {
            root: 1,
            folders: vec![ExistingFolder {
                id: 2,
                parent: Some(1),
                name: "a".into(),
            }],
            notes: vec![ExistingNote {
                folder: 1,
                title: "X".into(),
                body: "other".into(),
            }],
            ..Default::default()
        };
        let acts = plan(&[note(&["a"], "X.md", "1")], "/o", &e);
        assert_eq!(outcomes(&acts), [Outcome::Skip, Outcome::Create]);
    }

    #[test]
    fn non_md_goes_to_files_with_dirs_and_mime() {
        let acts = plan(&[bin(&["img"], "Pic.JPG", "h")], "/obsidian/T", &ex());
        let dirs: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Action::Dir { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(dirs, ["/obsidian", "/obsidian/T", "/obsidian/T/img"]);
        match acts.last().unwrap() {
            Action::File {
                abs_dir,
                mime,
                outcome,
                ..
            } => {
                assert_eq!(abs_dir, "/obsidian/T/img");
                assert_eq!(mime, "image/jpeg");
                assert_eq!(*outcome, Outcome::Create);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(!acts.iter().any(|a| matches!(a, Action::Folder { .. })));
    }

    #[test]
    fn mime_defaults_to_octet_stream() {
        assert_eq!(mime_for("x.weird"), "application/octet-stream");
        assert_eq!(mime_for("noext"), "application/octet-stream");
        assert_eq!(mime_for("a.pdf"), "application/pdf");
    }

    #[test]
    fn scan_skips_hidden_and_splits_kinds() {
        let root = std::env::temp_dir().join(format!("locus-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for d in [".obsidian", ".trash", "sub", "sub/.git"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for (f, c) in [
            (".obsidian/app.json", "{}"),
            (".trash/old.md", "x"),
            (".hidden.md", "x"),
            ("sub/.git/config", "x"),
            ("Top.md", "---\ntag: a\n---\n[[Link]]\n"),
            ("sub/Deep.md", "d"),
            ("sub/pic.png", "png"),
        ] {
            std::fs::write(root.join(f), c).unwrap();
        }
        let items = scan(&root).unwrap();
        let names: Vec<String> = items.iter().map(|i| join_rel(&i.dir, &i.name)).collect();
        assert_eq!(names, ["Top.md", "sub/Deep.md", "sub/pic.png"]);
        assert_eq!(
            items[0].content,
            Content::Note("---\ntag: a\n---\n[[Link]]\n".into())
        );
        assert!(matches!(items[2].content, Content::Binary { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }
}
