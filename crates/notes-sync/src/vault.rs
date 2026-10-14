//! Two-way sync between a local Obsidian vault and the notes cache.
//!
//! Testing harness only — vault notes stay off the ship (`source=vault`,
//! skipped by `push_dirty`). A symlink into `files/` would not populate the
//! sidebar; this maps `.md` paths onto high-range note ids.

use crate::store::{Store, VaultFile};
use crate::Note;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const SKIP_DIRS: &[&str] = &[".obsidian", ".git", ".trash", ".stfolder", "node_modules"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub pulled: usize,
    pub pushed: usize,
    pub conflicts: usize,
    pub removed: usize,
}

pub fn root() -> Option<PathBuf> {
    for key in ["NOTES_VAULT", "OBSIDIAN_VAULT_PATH"] {
        if let Ok(p) = std::env::var(key) {
            let p = PathBuf::from(p);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let home = dirs::home_dir()?;
    let p = home.join("Documents/new");
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

pub fn sync(store: &Store, root: &Path) -> crate::Result<Report> {
    let mut report = Report::default();
    let files = walk_md(root)?;
    let mut seen = HashSet::new();

    for path in &files {
        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        seen.insert(rel.clone());
        let bytes = fs::read(path)?;
        let body = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let file_hash = sha_bytes(body.as_bytes());
        let mtime = mtime_of(path);
        let title = title_of(path, &body);
        let head_hash = Store::content_hash(&title, &body);

        match store.vault_get(&rel)? {
            None => {
                let id = store.next_vault_id()?;
                let note = Note {
                    id,
                    title: title.clone(),
                    body_md: body.clone(),
                    revision: 0,
                    folder_id: 0,
                };
                store.put_vault_head(&note)?;
                let _ = store.snapshot(id, &title, &body)?;
                store.vault_put(&VaultFile {
                    path: rel,
                    note_id: id,
                    file_hash,
                    mtime,
                    head_hash,
                })?;
                report.pulled += 1;
            }
            Some(map) => {
                let Some(head) = store.head(map.note_id)? else {
                    import_existing(
                        store,
                        map.note_id,
                        &title,
                        &body,
                        &rel,
                        &file_hash,
                        mtime,
                        &head_hash,
                    )?;
                    report.pulled += 1;
                    continue;
                };
                let now_hash = Store::content_hash(&head.title, &head.body_md);
                let file_changed = file_hash != map.file_hash;
                let app_changed = now_hash != map.head_hash;

                if !file_changed && !app_changed {
                    continue;
                }
                if file_changed && app_changed && body != head.body_md {
                    let merged = keep_both(&head.body_md, &body);
                    let title = title_of(path, &merged);
                    store.put_vault_head(&Note {
                        id: map.note_id,
                        title: title.clone(),
                        body_md: merged.clone(),
                        revision: 0,
                        folder_id: 0,
                    })?;
                    let _ = store.snapshot(map.note_id, &title, &merged)?;
                    write_file(path, &merged)?;
                    remember(store, &rel, map.note_id, path, &title, &merged)?;
                    report.conflicts += 1;
                } else if file_changed {
                    store.put_vault_head(&Note {
                        id: map.note_id,
                        title: title.clone(),
                        body_md: body.clone(),
                        revision: 0,
                        folder_id: 0,
                    })?;
                    let _ = store.snapshot(map.note_id, &title, &body)?;
                    remember(store, &rel, map.note_id, path, &title, &body)?;
                    report.pulled += 1;
                } else {
                    write_file(path, &head.body_md)?;
                    remember(store, &rel, map.note_id, path, &head.title, &head.body_md)?;
                    report.pushed += 1;
                }
            }
        }
    }

    for map in store.vault_all()? {
        if seen.contains(&map.path) {
            continue;
        }
        store.delete_note(map.note_id)?;
        report.removed += 1;
    }

    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn import_existing(
    store: &Store,
    id: u64,
    title: &str,
    body: &str,
    rel: &str,
    file_hash: &str,
    mtime: i64,
    head_hash: &str,
) -> crate::Result<()> {
    store.put_vault_head(&Note {
        id,
        title: title.to_string(),
        body_md: body.to_string(),
        revision: 0,
        folder_id: 0,
    })?;
    let _ = store.snapshot(id, title, body)?;
    store.vault_put(&VaultFile {
        path: rel.to_string(),
        note_id: id,
        file_hash: file_hash.to_string(),
        mtime,
        head_hash: head_hash.to_string(),
    })?;
    Ok(())
}

fn remember(
    store: &Store,
    rel: &str,
    id: u64,
    path: &Path,
    title: &str,
    body: &str,
) -> crate::Result<()> {
    store.vault_put(&VaultFile {
        path: rel.to_string(),
        note_id: id,
        file_hash: sha_bytes(body.as_bytes()),
        mtime: mtime_of(path),
        head_hash: Store::content_hash(title, body),
    })?;
    Ok(())
}

fn keep_both(app: &str, file: &str) -> String {
    format!(
        "{}\n\n---\n## Other version (vault file)\n\n{}",
        app.trim_end(),
        file
    )
}

fn write_file(path: &Path, body: &str) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "note.md".into());
    let tmp = path.with_file_name(format!(".{name}.tmp-notes"));
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn walk_md(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_rec(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_rec(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
            continue;
        }
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            if ft.is_symlink() {
                continue;
            }
            walk_rec(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

fn title_of(path: &Path, body: &str) -> String {
    let body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let (yaml_title, rest) = split_frontmatter(body);
    for line in rest.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix("# ") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
        break;
    }
    if let Some(title) = yaml_title {
        return title;
    }
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled".into())
}

fn split_frontmatter(body: &str) -> (Option<String>, &str) {
    let Some(after) = body.strip_prefix("---\n") else {
        return (None, body);
    };
    let Some(end) = after.find("\n---") else {
        return (None, body);
    };
    let yaml = &after[..end];
    let rest = after[end + 4..].strip_prefix('\n').unwrap_or(&after[end + 4..]);
    let mut title = None;
    for line in yaml.lines() {
        let line = line.trim();
        let Some(value) = line.strip_prefix("title:") else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            title = Some(value.to_string());
            break;
        }
    }
    (title, rest)
}

fn sha_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn mtime_of(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use crate::VAULT_ID_BASE;

    fn tmp_vault() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "locus-vault-test-{}",
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn pull_then_push_and_conflict() {
        let root = tmp_vault();
        fs::write(root.join("hello.md"), "# Hello\n\nfrom disk\n").unwrap();
        let store = Store::open_memory().unwrap();
        let r = sync(&store, &root).unwrap();
        assert_eq!(r.pulled, 1);
        let notes = store.list_heads().unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].id >= VAULT_ID_BASE);
        assert!(store.is_vault(notes[0].id).unwrap());
        assert!(store.dirty_notes().unwrap().is_empty());

        store
            .snapshot(notes[0].id, "Hello", "# Hello\n\nfrom app\n")
            .unwrap();
        assert!(store.dirty_notes().unwrap().is_empty());
        let r = sync(&store, &root).unwrap();
        assert_eq!(r.pushed, 1);
        let disk = fs::read_to_string(root.join("hello.md")).unwrap();
        assert!(disk.contains("from app"));

        fs::write(root.join("hello.md"), "# Hello\n\nfrom obsidian\n").unwrap();
        store
            .snapshot(notes[0].id, "Hello", "# Hello\n\nfrom app again\n")
            .unwrap();
        let r = sync(&store, &root).unwrap();
        assert_eq!(r.conflicts, 1);
        let disk = fs::read_to_string(root.join("hello.md")).unwrap();
        assert!(disk.contains("from app again"));
        assert!(disk.contains("from obsidian"));

        fs::remove_file(root.join("hello.md")).unwrap();
        let r = sync(&store, &root).unwrap();
        assert_eq!(r.removed, 1);
        assert!(store.list_heads().unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn yaml_title_when_no_heading() {
        let root = tmp_vault();
        fs::write(
            root.join("plain.md"),
            "---\ntitle: From YAML\n---\n\nno heading here\n",
        )
        .unwrap();
        let store = Store::open_memory().unwrap();
        sync(&store, &root).unwrap();
        let notes = store.list_heads().unwrap();
        assert_eq!(notes[0].title, "From YAML");
        let _ = fs::remove_dir_all(&root);
    }
}
