//! `locus backup export <dir>` and `locus backup import <dir>`: copy a ship's
//! Locus data out to a folder and back into another ship.
//!
//! This is how data moves to a new identity (a comet reset, or a comet to a
//! planet). A ZFS snapshot of the pier restores the same ship; this restores
//! the data onto a different one.
//!
//! The folder holds `locus-backup.json` (notebooks with folders and notes,
//! files, photos, events, contacts, and message threads) and `blobs/<sha256>`
//! (the bytes of every file). Import recreates notebooks, folders, notes,
//! files, photos, events and contacts. It skips anything the target already
//! has, so it is safe to rerun. Not carried over: note revision history,
//! timestamps (items get the time of the import) and messages, which stay in
//! the backup file because replaying them would send them to people again.

use locus_client::calendar::Event;
use locus_client::contacts::Contact;
use locus_client::files::FileEntry;
use locus_client::messages::Message;
use locus_client::notes::{Folder, Note};
use locus_client::photos::Photo;
use locus_client::{Client, config};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::apps::{fmt, parse_server};

pub const USAGE: &str = "usage: locus backup export <dir>   # copy this ship's data to <dir>\n\
\x20      locus backup import <dir>   # recreate it on the connected ship (safe to rerun)";

const MANIFEST: &str = "locus-backup.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    pub version: u32,
    /// The ship the data came from.
    pub ship: String,
    pub notebooks: Vec<NotebookDump>,
    pub files: Vec<FileEntry>,
    pub dirs: Vec<String>,
    pub photos: Vec<Photo>,
    pub events: Vec<Event>,
    pub contacts: Vec<Contact>,
    /// Kept for reference only; import does not replay them.
    #[serde(default)]
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotebookDump {
    pub title: String,
    pub root_folder_id: u64,
    pub folders: Vec<Folder>,
    pub notes: Vec<Note>,
}

/// `locus backup <export|import> <dir>`.
pub fn run(args: Vec<String>) -> Result<(), String> {
    let [cmd, dir] = args.as_slice() else {
        return Err(USAGE.into());
    };
    let dir = PathBuf::from(dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let client = connect().await?;
        match cmd.as_str() {
            "export" => export(&client, &dir).await,
            "import" => import(&client, &dir).await,
            _ => Err(USAGE.into()),
        }
    })
}

async fn connect() -> Result<Client, String> {
    let settings = config::load();
    if settings.server.is_empty() {
        return Err("not connected; run: locus connect <server-url>".into());
    }
    let client = Client::new(parse_server(&settings.server)?).map_err(fmt::client_error)?;
    if !settings.code.is_empty() {
        client
            .login(&settings.code)
            .await
            .map_err(fmt::client_error)?;
    }
    Ok(client)
}

fn err(what: &str) -> impl Fn(locus_client::Error) -> String + '_ {
    move |e| format!("{what}: {}", fmt::client_error(e))
}

async fn export(client: &Client, dir: &Path) -> Result<(), String> {
    let blobs = dir.join("blobs");
    std::fs::create_dir_all(&blobs).map_err(|e| format!("{}: {e}", blobs.display()))?;
    let ship = client.whoami().await.map(|m| m.ship).unwrap_or_default();

    let mut notebooks = Vec::new();
    for nb in client.list_notebooks().await.map_err(err("notebooks"))? {
        let (host, name) = (nb.host.as_str(), nb.flag_name.as_str());
        let folders = client
            .list_folders(host, name)
            .await
            .map_err(err("folders"))?;
        let notes = client.list_notes(host, name).await.map_err(err("notes"))?;
        println!(
            "notebook {:<40} {} folders, {} notes",
            nb.notebook.title,
            folders.len(),
            notes.len()
        );
        notebooks.push(NotebookDump {
            title: nb.notebook.title,
            root_folder_id: nb.notebook.root_folder_id,
            folders,
            notes,
        });
    }

    let files = client.list_files(None).await.map_err(err("files"))?;
    let dirs = client.list_dirs().await.map_err(err("folders of files"))?;
    for f in &files {
        let path = blobs.join(&f.sha256);
        if path.exists() {
            continue;
        }
        let bytes = client
            .download_blob(&f.sha256)
            .await
            .map_err(err(&f.name))?;
        if sha256_hex(&bytes) != f.sha256 {
            return Err(format!(
                "{}{}: bytes do not match their hash",
                f.dir, f.name
            ));
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    println!("files    {} files in {} folders", files.len(), dirs.len());

    let photos = client.list_photos().await.map_err(err("photos"))?;
    let events = client.list_events().await.map_err(err("events"))?;
    let contacts = client.list_contacts().await.map_err(err("contacts"))?;
    let mut messages = Vec::new();
    // A ship without the messages agent, or an older one, still exports the rest.
    if let Ok(convs) = client.list_conversations().await {
        for c in convs {
            messages.extend(client.thread(&c.peer).await.map_err(err("messages"))?);
        }
    }
    if let Ok(groups) = client.list_groups().await {
        for g in groups {
            messages.extend(client.group_thread(&g.gid).await.map_err(err("messages"))?);
        }
    }
    println!(
        "other    {} photos, {} events, {} contacts, {} messages",
        photos.len(),
        events.len(),
        contacts.len(),
        messages.len()
    );

    let backup = Backup {
        version: 1,
        ship,
        notebooks,
        files,
        dirs,
        photos,
        events,
        contacts,
        messages,
    };
    let path = dir.join(MANIFEST);
    let json = serde_json::to_vec_pretty(&backup).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

async fn import(client: &Client, dir: &Path) -> Result<(), String> {
    let path = dir.join(MANIFEST);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let backup: Backup =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;

    let existing = client.list_notebooks().await.map_err(err("notebooks"))?;
    for nb in &backup.notebooks {
        let target = match existing.iter().find(|e| e.notebook.title == nb.title) {
            Some(t) => t.clone(),
            None => client
                .create_notebook(&nb.title)
                .await
                .map_err(err(&nb.title))?,
        };
        let (host, name) = (target.host.clone(), target.flag_name.clone());
        let have_folders = client
            .list_folders(&host, &name)
            .await
            .map_err(err("folders"))?;
        let have_notes = client
            .list_notes(&host, &name)
            .await
            .map_err(err("notes"))?;
        let mut ids = HashMap::from([(nb.root_folder_id, target.notebook.root_folder_id)]);
        let (mut folders, mut notes) = (0, 0);
        for f in folder_order(&nb.folders, nb.root_folder_id) {
            let Some(&parent) = f.parent_folder_id.and_then(|p| ids.get(&p)) else {
                continue;
            };
            let id = match have_folders
                .iter()
                .find(|h| h.parent_folder_id == Some(parent) && h.name == f.name)
            {
                Some(h) => h.id,
                None => {
                    folders += 1;
                    client
                        .create_folder(&host, &name, parent, &f.name)
                        .await
                        .map_err(err(&f.name))?
                        .id
                }
            };
            ids.insert(f.id, id);
        }
        for n in &nb.notes {
            let Some(&folder) = ids.get(&n.folder_id) else {
                return Err(format!(
                    "{}: note {} is in a folder the backup lacks",
                    nb.title, n.title
                ));
            };
            if have_notes
                .iter()
                .any(|h| h.folder_id == folder && h.title == n.title)
            {
                continue;
            }
            client
                .create_note(&host, &name, folder, &n.title, &n.body_md)
                .await
                .map_err(err(&n.title))?;
            notes += 1;
        }
        println!(
            "notebook {:<40} {folders} folders, {notes} notes created",
            nb.title
        );
    }

    let have_dirs: HashSet<String> = client
        .list_dirs()
        .await
        .map_err(err("folders of files"))?
        .into_iter()
        .collect();
    for d in &backup.dirs {
        if d != "/" && !have_dirs.contains(d) {
            client.create_dir(d).await.map_err(err(d))?;
        }
    }
    let have_files = client.list_files(None).await.map_err(err("files"))?;
    let mut files = 0;
    for f in &backup.files {
        if have_files
            .iter()
            .any(|h| h.dir == f.dir && h.name == f.name)
        {
            continue;
        }
        let blob = dir.join("blobs").join(&f.sha256);
        let bytes = std::fs::read(&blob).map_err(|e| format!("{}: {e}", blob.display()))?;
        client.upload_blob(bytes).await.map_err(err(&f.name))?;
        client
            .create_file(&f.dir, &f.name, &f.sha256, f.size, &f.mime)
            .await
            .map_err(err(&f.name))?;
        files += 1;
    }
    println!("files    {files} created");

    let have = client.list_photos().await.map_err(err("photos"))?;
    let mut photos = 0;
    for p in &backup.photos {
        if have.iter().any(|h| h.path == p.path && h.album == p.album) {
            continue;
        }
        let tags: Vec<&str> = p.tags.iter().map(String::as_str).collect();
        client
            .create_photo(&p.album, &p.title, &p.path, &tags)
            .await
            .map_err(err(&p.title))?;
        photos += 1;
    }
    let have = client.list_events().await.map_err(err("events"))?;
    let mut events = 0;
    for e in &backup.events {
        if have
            .iter()
            .any(|h| h.title == e.title && h.start == e.start)
        {
            continue;
        }
        client
            .create_event(&e.title, e.start, e.end, &e.body)
            .await
            .map_err(err(&e.title))?;
        events += 1;
    }
    let have = client.list_contacts().await.map_err(err("contacts"))?;
    let mut contacts = 0;
    for c in &backup.contacts {
        if have.iter().any(|h| h.name == c.name && h.email == c.email) {
            continue;
        }
        client
            .create_contact(&c.name, &c.email, &c.ship)
            .await
            .map_err(err(&c.name))?;
        contacts += 1;
    }
    println!("other    {photos} photos, {events} events, {contacts} contacts created");
    if !backup.messages.is_empty() {
        println!(
            "messages {} kept in {} and not replayed",
            backup.messages.len(),
            path.display()
        );
    }
    Ok(())
}

/// The folders below `root`, each after its parent. Folders whose parent is
/// missing are left out.
pub fn folder_order(folders: &[Folder], root: u64) -> Vec<&Folder> {
    let mut placed = HashSet::from([root]);
    let mut out = Vec::new();
    loop {
        let before = out.len();
        for f in folders {
            if f.id != root
                && !placed.contains(&f.id)
                && f.parent_folder_id.is_some_and(|p| placed.contains(&p))
            {
                placed.insert(f.id);
                out.push(f);
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: u64, parent: Option<u64>, name: &str) -> Folder {
        Folder {
            id,
            notebook_id: 1,
            name: name.into(),
            parent_folder_id: parent,
        }
    }

    #[test]
    fn parents_come_first() {
        let fs = [
            folder(4, Some(3), "c"),
            folder(1, None, ""),
            folder(3, Some(2), "b"),
            folder(2, Some(1), "a"),
            folder(9, Some(8), "orphan"),
        ];
        let ids: Vec<u64> = folder_order(&fs, 1).iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![2, 3, 4]);
    }
}
