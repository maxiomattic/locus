//! Named functions the window and the `notes` CLI call.
//!
//! Live HTTP to the ship, plus a local copy-on-write snapshot store.

use notes_protocol::{Client, Error as ProtoError, NotebookSummary};
pub use notes_protocol::Note;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod store;
pub use store::{Snapshot, Store, VaultFile, VAULT_ID_BASE};

mod vault;
pub use vault::{root as vault_root, sync as vault_sync, Report as VaultReport};

pub const APP_ID: &str = "org.digital-workspace.notes";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Protocol(#[from] ProtoError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no notebook on this ship yet")]
    NoNotebook,
}

impl Error {
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::Protocol(ProtoError::Conflict))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedSession {
    pub eyre_url: String,
    pub api_key: Option<String>,
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_ID)
}

pub fn store_path() -> PathBuf {
    data_dir().join("cache.sqlite")
}

pub fn session_path() -> PathBuf {
    data_dir().join("session.json")
}

pub fn load_saved() -> SavedSession {
    let raw = std::fs::read_to_string(session_path()).ok();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| SavedSession {
            eyre_url: std::env::var("NOTES_EYRE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            api_key: None,
        })
}

pub fn save_session(session: &Session) {
    let path = session_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let saved = SavedSession {
        eyre_url: session.eyre_url().to_string(),
        api_key: session.api_key().map(str::to_string),
    };
    let _ = std::fs::write(
        path,
        serde_json::to_string_pretty(&saved).unwrap_or_default(),
    );
}

pub fn search_local(store: &Store, query: &str) -> Result<Vec<Note>> {
    let q = query.trim().to_lowercase();
    let mut notes = store.list_heads()?;
    if !q.is_empty() {
        notes.retain(|n| {
            n.title.to_lowercase().contains(&q) || n.body_md.to_lowercase().contains(&q)
        });
    }
    Ok(notes)
}

pub fn keep_both(local_body: &str, host: &Note) -> String {
    format!(
        "{}\n\n---\n## Other version (ship r{})\n\n{}",
        local_body.trim_end(),
        host.revision,
        host.body_md
    )
}

/// Push queued creates/deletes and dirty heads. Stops on the first conflict.
pub fn push_dirty(session: &Session, store: &Store) -> Result<usize> {
    let mut n = 0usize;
    for (oid, title, body) in store.pending_creates()? {
        let note = session.note_create(&title, &body)?;
        store.upsert_head(&note)?;
        let _ = store.snapshot(note.id, &note.title, &note.body_md);
        store.remove_outbox(oid)?;
        n += 1;
    }
    for id in store.pending_deletes()? {
        session.note_delete(id)?;
        store.delete_note(id)?;
        store.remove_delete(id)?;
        n += 1;
    }
    for note in store.dirty_notes()? {
        let saved = session.note_patch(note.id, &note.title, &note.body_md, note.revision)?;
        store.upsert_head(&saved)?;
        store.clear_dirty(saved.id)?;
        n += 1;
    }
    Ok(n)
}

#[derive(Clone)]
pub struct Session {
    client: Client,
    notebook: NotebookSummary,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("notebook", &self.notebook.notebook.title)
            .finish()
    }
}

impl Session {
    /// Log in and pick the first notebook, or create "Notes".
    pub fn connect(eyre_url: &str, web_code: &str) -> Result<Self> {
        let mut client = Client::new(eyre_url);
        client.login(web_code)?;
        Self::from_client(client)
    }

    pub fn connect_with_api_key(eyre_url: &str, api_key: &str) -> Result<Self> {
        Self::from_client(Client::with_api_key(eyre_url, api_key))
    }

    fn from_client(client: Client) -> Result<Self> {
        let mut books = client.list_notebooks()?;
        if books.is_empty() {
            books.push(client.create_notebook("Notes")?);
        }
        let notebook = books
            .into_iter()
            .next()
            .ok_or(Error::NoNotebook)?;
        Ok(Self { client, notebook })
    }

    pub fn eyre_url(&self) -> &str {
        self.client.base_url()
    }

    pub fn api_key(&self) -> Option<&str> {
        self.client.api_key()
    }

    pub fn notebook_title(&self) -> &str {
        &self.notebook.notebook.title
    }

    pub fn note_create(&self, title: &str, body: &str) -> Result<Note> {
        Ok(self.client.create_note(
            &self.notebook.host,
            &self.notebook.flag_name,
            self.notebook.notebook.root_folder_id,
            title,
            body,
        )?)
    }

    pub fn note_open(&self, id: u64) -> Result<Note> {
        Ok(self
            .client
            .get_note(&self.notebook.host, &self.notebook.flag_name, id)?)
    }

    pub fn note_search(&self, query: &str) -> Result<Vec<Note>> {
        let q = query.trim().to_lowercase();
        let mut notes = self
            .client
            .list_notes(&self.notebook.host, &self.notebook.flag_name)?;
        if !q.is_empty() {
            notes.retain(|n| n.title.to_lowercase().contains(&q));
        }
        Ok(notes)
    }

    pub fn note_patch(
        &self,
        id: u64,
        title: &str,
        body: &str,
        expected_revision: u64,
    ) -> Result<Note> {
        // Two PUTs: body-present writes ignore title on the ship.
        self.client.rename_note(
            &self.notebook.host,
            &self.notebook.flag_name,
            id,
            title,
        )?;
        self.client.save_note(
            &self.notebook.host,
            &self.notebook.flag_name,
            id,
            body,
            expected_revision,
        )?;
        self.note_open(id)
    }

    pub fn note_list(&self) -> Result<Vec<Note>> {
        Ok(self
            .client
            .list_notes(&self.notebook.host, &self.notebook.flag_name)?)
    }

    pub fn note_delete(&self, id: u64) -> Result<()> {
        Ok(self.client.delete_note(
            &self.notebook.host,
            &self.notebook.flag_name,
            id,
        )?)
    }
}

/// Fetch the ship's web login code from the bring-up VM over SSH.
/// Does not print the code.
pub fn fetch_vm_web_code() -> std::result::Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME is unset".to_string())?;
    let key = std::path::Path::new(&home).join("vms/locus-server/id_ed25519");
    if !key.exists() {
        return Err("no VM key at ~/vms/locus-server/id_ed25519 — is the test VM set up?".into());
    }
    let known = std::path::Path::new(&home).join("vms/locus-server/known_hosts");
    let out = std::process::Command::new("ssh")
        .args([
            "-i",
            key.to_str().unwrap(),
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "IdentityAgent=none",
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            &format!("UserKnownHostsFile={}", known.display()),
        ])
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-p",
            "22221",
            "locus@127.0.0.1",
            "sudo -u urbit urbit-code",
        ])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "could not reach the test VM (is it up? ./scripts/server-vm.sh up)\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let code = String::from_utf8_lossy(&out.stdout)
        .trim()
        .trim_matches('\r')
        .to_string();
    if code.is_empty() {
        return Err("VM returned an empty login code".into());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_both_appends_host() {
        let host = Note {
            id: 1,
            title: "A".into(),
            body_md: "theirs".into(),
            revision: 4,
            folder_id: 0,
        };
        let merged = keep_both("mine", &host);
        assert!(merged.contains("mine"));
        assert!(merged.contains("theirs"));
        assert!(merged.contains("r4"));
    }
}
