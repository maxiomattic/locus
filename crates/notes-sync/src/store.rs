//! Local copy-on-write note history.
//!
//! Blobs are keyed by content hash. A snapshot is a pointer at a blob plus
//! a sequence number. Repeating the same title+body does not allocate.

use crate::Note;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VAULT_ID_BASE: u64 = 1_000_000_000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS blobs (
    hash TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS snapshots (
    note_id INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    blob_hash TEXT NOT NULL REFERENCES blobs(hash),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (note_id, seq)
);
CREATE INDEX IF NOT EXISTS snapshots_note_created ON snapshots(note_id, created_at DESC);
CREATE TABLE IF NOT EXISTS heads (
    note_id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    ship_revision INTEGER NOT NULL,
    folder_id INTEGER NOT NULL,
    blob_hash TEXT NOT NULL,
    local_seq INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS heads_title ON heads(title);
CREATE TABLE IF NOT EXISTS outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    op TEXT NOT NULL,
    note_id INTEGER,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    expected_revision INTEGER,
    created_at INTEGER NOT NULL
);
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub note_id: u64,
    pub seq: u64,
    pub title: String,
    pub body: String,
    pub hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFile {
    pub path: String,
    pub note_id: u64,
    pub file_hash: String,
    pub mtime: i64,
    pub head_hash: String,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> crate::Result<Self> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_memory() -> crate::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn content_hash(title: &str, body: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(title.as_bytes());
        hasher.update([0]);
        hasher.update(body.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn upsert_head(&self, note: &Note) -> crate::Result<()> {
        let hash = Self::content_hash(&note.title, &note.body_md);
        self.conn.execute(
            "INSERT OR IGNORE INTO blobs (hash, title, body) VALUES (?1, ?2, ?3)",
            params![hash, note.title, note.body_md],
        )?;
        self.conn.execute(
            "INSERT INTO heads (note_id, title, body, ship_revision, folder_id, blob_hash, local_seq)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
             ON CONFLICT(note_id) DO UPDATE SET
                title = excluded.title,
                body = excluded.body,
                ship_revision = excluded.ship_revision,
                folder_id = excluded.folder_id,
                blob_hash = excluded.blob_hash,
                dirty = 0",
            params![
                note.id as i64,
                note.title,
                note.body_md,
                note.revision as i64,
                note.folder_id as i64,
                hash
            ],
        )?;
        Ok(())
    }

    /// Record a version if the content changed. Returns the new snapshot,
    /// or `None` when the hash matches the current head (no write).
    pub fn snapshot(&self, note_id: u64, title: &str, body: &str) -> crate::Result<Option<Snapshot>> {
        let hash = Self::content_hash(title, body);
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT blob_hash FROM heads WHERE note_id = ?1",
                params![note_id as i64],
                |row| row.get(0),
            )
            .optional()?;
        if current.as_deref() == Some(hash.as_str()) {
            return Ok(None);
        }

        self.conn.execute(
            "INSERT OR IGNORE INTO blobs (hash, title, body) VALUES (?1, ?2, ?3)",
            params![hash, title, body],
        )?;

        let next_seq: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM snapshots WHERE note_id = ?1",
                params![note_id as i64],
                |row| row.get(0),
            )?;
        let created_at = now_secs();
        self.conn.execute(
            "INSERT INTO snapshots (note_id, seq, blob_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![note_id as i64, next_seq, hash, created_at],
        )?;
        self.conn.execute(
            "INSERT INTO heads (note_id, title, body, ship_revision, folder_id, blob_hash, local_seq, dirty, source)
             VALUES (?1, ?2, ?3, COALESCE((SELECT ship_revision FROM heads WHERE note_id = ?1), 0),
                     COALESCE((SELECT folder_id FROM heads WHERE note_id = ?1), 0), ?4, ?5,
                     CASE WHEN COALESCE((SELECT source FROM heads WHERE note_id = ?1), 'ship') = 'vault' THEN 0 ELSE 1 END,
                     COALESCE((SELECT source FROM heads WHERE note_id = ?1), 'ship'))
             ON CONFLICT(note_id) DO UPDATE SET
                title = excluded.title,
                body = excluded.body,
                blob_hash = excluded.blob_hash,
                local_seq = excluded.local_seq,
                dirty = CASE WHEN heads.source = 'vault' THEN 0 ELSE 1 END",
            params![note_id as i64, title, body, hash, next_seq],
        )?;

        Ok(Some(Snapshot {
            note_id,
            seq: next_seq as u64,
            title: title.to_string(),
            body: body.to_string(),
            hash,
            created_at,
        }))
    }

    pub fn set_ship_revision(&self, note_id: u64, revision: u64) -> crate::Result<()> {
        self.conn.execute(
            "UPDATE heads SET ship_revision = ?2 WHERE note_id = ?1",
            params![note_id as i64, revision as i64],
        )?;
        Ok(())
    }

    pub fn mark_dirty(&self, note_id: u64) -> crate::Result<()> {
        self.conn.execute(
            "UPDATE heads SET dirty = 1 WHERE note_id = ?1 AND source != 'vault'",
            params![note_id as i64],
        )?;
        Ok(())
    }

    pub fn clear_dirty(&self, note_id: u64) -> crate::Result<()> {
        self.conn.execute(
            "UPDATE heads SET dirty = 0 WHERE note_id = ?1",
            params![note_id as i64],
        )?;
        Ok(())
    }

    pub fn dirty_notes(&self) -> crate::Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT note_id, title, body, ship_revision, folder_id FROM heads WHERE dirty = 1 AND source != 'vault' ORDER BY note_id",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        let mut notes = Vec::new();
        for row in rows {
            notes.push(row?);
        }
        Ok(notes)
    }

    /// Ids of ship notes with local edits not yet pushed to the ship.
    pub fn dirty_ids(&self) -> crate::Result<std::collections::HashSet<u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT note_id FROM heads WHERE dirty = 1 AND source != 'vault'")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        let mut out = std::collections::HashSet::new();
        for row in rows {
            out.insert(row? as u64);
        }
        Ok(out)
    }

    pub fn enqueue_create(&self, title: &str, body: &str) -> crate::Result<i64> {
        self.conn.execute(
            "INSERT INTO outbox (op, title, body, created_at) VALUES ('create', ?1, ?2, ?3)",
            params![title, body, now_secs()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn enqueue_delete(&self, note_id: u64) -> crate::Result<()> {
        self.conn.execute(
            "INSERT INTO outbox (op, note_id, created_at) VALUES ('delete', ?1, ?2)",
            params![note_id as i64, now_secs()],
        )?;
        Ok(())
    }

    pub fn pending_creates(&self) -> crate::Result<Vec<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, body FROM outbox WHERE op = 'create' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn pending_deletes(&self) -> crate::Result<Vec<u64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT note_id FROM outbox WHERE op = 'delete' ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row? as u64);
        }
        Ok(out)
    }

    pub fn remove_outbox(&self, id: i64) -> crate::Result<()> {
        self.conn
            .execute("DELETE FROM outbox WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn remove_delete(&self, note_id: u64) -> crate::Result<()> {
        self.conn.execute(
            "DELETE FROM outbox WHERE op = 'delete' AND note_id = ?1",
            params![note_id as i64],
        )?;
        Ok(())
    }

    pub fn head(&self, note_id: u64) -> crate::Result<Option<Note>> {
        self.conn
            .query_row(
                "SELECT note_id, title, body, ship_revision, folder_id FROM heads WHERE note_id = ?1",
                params![note_id as i64],
                row_to_note,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn list_heads(&self) -> crate::Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(
            "SELECT note_id, title, body, ship_revision, folder_id FROM heads ORDER BY note_id DESC",
        )?;
        let rows = stmt.query_map([], row_to_note)?;
        let mut notes = Vec::new();
        for row in rows {
            notes.push(row?);
        }
        Ok(notes)
    }

    pub fn history(&self, note_id: u64, limit: u32) -> crate::Result<Vec<Snapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.note_id, s.seq, b.title, b.body, s.blob_hash, s.created_at
             FROM snapshots s
             JOIN blobs b ON b.hash = s.blob_hash
             WHERE s.note_id = ?1
             ORDER BY s.seq DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![note_id as i64, limit], |row| {
            Ok(Snapshot {
                note_id: row.get::<_, i64>(0)? as u64,
                seq: row.get::<_, i64>(1)? as u64,
                title: row.get(2)?,
                body: row.get(3)?,
                hash: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn snapshot_at(&self, note_id: u64, seq: u64) -> crate::Result<Option<Snapshot>> {
        self.conn
            .query_row(
                "SELECT s.note_id, s.seq, b.title, b.body, s.blob_hash, s.created_at
                 FROM snapshots s
                 JOIN blobs b ON b.hash = s.blob_hash
                 WHERE s.note_id = ?1 AND s.seq = ?2",
                params![note_id as i64, seq as i64],
                |row| {
                    Ok(Snapshot {
                        note_id: row.get::<_, i64>(0)? as u64,
                        seq: row.get::<_, i64>(1)? as u64,
                        title: row.get(2)?,
                        body: row.get(3)?,
                        hash: row.get(4)?,
                        created_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn blob_count(&self) -> crate::Result<u64> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))?;
        Ok(n as u64)
    }

    pub fn delete_note(&self, note_id: u64) -> crate::Result<()> {
        self.conn.execute(
            "DELETE FROM vault_files WHERE note_id = ?1",
            params![note_id as i64],
        )?;
        self.conn.execute(
            "DELETE FROM snapshots WHERE note_id = ?1",
            params![note_id as i64],
        )?;
        self.conn.execute(
            "DELETE FROM heads WHERE note_id = ?1",
            params![note_id as i64],
        )?;
        Ok(())
    }

    pub fn is_vault(&self, note_id: u64) -> crate::Result<bool> {
        let source: Option<String> = self
            .conn
            .query_row(
                "SELECT source FROM heads WHERE note_id = ?1",
                params![note_id as i64],
                |row| row.get(0),
            )
            .optional()?;
        Ok(source.as_deref() == Some("vault"))
    }

    pub fn put_vault_head(&self, note: &Note) -> crate::Result<()> {
        let hash = Self::content_hash(&note.title, &note.body_md);
        self.conn.execute(
            "INSERT OR IGNORE INTO blobs (hash, title, body) VALUES (?1, ?2, ?3)",
            params![hash, note.title, note.body_md],
        )?;
        self.conn.execute(
            "INSERT INTO heads (note_id, title, body, ship_revision, folder_id, blob_hash, local_seq, dirty, source)
             VALUES (?1, ?2, ?3, 0, 0, ?4, 0, 0, 'vault')
             ON CONFLICT(note_id) DO UPDATE SET
                title = excluded.title,
                body = excluded.body,
                blob_hash = excluded.blob_hash,
                dirty = 0",
            params![note.id as i64, note.title, note.body_md, hash],
        )?;
        Ok(())
    }

    pub fn next_vault_id(&self) -> crate::Result<u64> {
        let n: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(note_id), ?1 - 1) + 1 FROM heads WHERE note_id >= ?1",
            params![VAULT_ID_BASE as i64],
            |row| row.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn vault_get(&self, path: &str) -> crate::Result<Option<VaultFile>> {
        self.conn
            .query_row(
                "SELECT path, note_id, file_hash, mtime, head_hash FROM vault_files WHERE path = ?1",
                params![path],
                row_to_vault,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn vault_by_id(&self, note_id: u64) -> crate::Result<Option<VaultFile>> {
        self.conn
            .query_row(
                "SELECT path, note_id, file_hash, mtime, head_hash FROM vault_files WHERE note_id = ?1",
                params![note_id as i64],
                row_to_vault,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn vault_all(&self) -> crate::Result<Vec<VaultFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, note_id, file_hash, mtime, head_hash FROM vault_files ORDER BY path",
        )?;
        let rows = stmt.query_map([], row_to_vault)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn vault_put(&self, row: &VaultFile) -> crate::Result<()> {
        self.conn.execute(
            "INSERT INTO vault_files (path, note_id, file_hash, mtime, head_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                note_id = excluded.note_id,
                file_hash = excluded.file_hash,
                mtime = excluded.mtime,
                head_hash = excluded.head_hash",
            params![row.path, row.note_id as i64, row.file_hash, row.mtime, row.head_hash],
        )?;
        Ok(())
    }

    pub fn vault_remove(&self, path: &str) -> crate::Result<()> {
        self.conn
            .execute("DELETE FROM vault_files WHERE path = ?1", params![path])?;
        Ok(())
    }
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let _ = conn.execute(
        "ALTER TABLE heads ADD COLUMN dirty INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE heads ADD COLUMN source TEXT NOT NULL DEFAULT 'ship'",
        [],
    );
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault_files (
            path TEXT PRIMARY KEY,
            note_id INTEGER NOT NULL UNIQUE,
            file_hash TEXT NOT NULL,
            mtime INTEGER NOT NULL,
            head_hash TEXT NOT NULL
        );",
    )?;
    Ok(())
}

fn row_to_vault(row: &rusqlite::Row<'_>) -> rusqlite::Result<VaultFile> {
    Ok(VaultFile {
        path: row.get(0)?,
        note_id: row.get::<_, i64>(1)? as u64,
        file_hash: row.get(2)?,
        mtime: row.get(3)?,
        head_hash: row.get(4)?,
    })
}

fn row_to_note(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get::<_, i64>(0)? as u64,
        title: row.get(1)?,
        body_md: row.get(2)?,
        revision: row.get::<_, i64>(3)? as u64,
        folder_id: row.get::<_, i64>(4)? as u64,
    })
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u64, title: &str, body: &str) -> Note {
        Note {
            id,
            title: title.into(),
            body_md: body.into(),
            revision: 0,
            folder_id: 0,
        }
    }

    #[test]
    fn identical_content_is_one_blob() {
        let store = Store::open_memory().unwrap();
        store.upsert_head(&note(1, "A", "hello")).unwrap();
        assert!(store.snapshot(1, "A", "hello").unwrap().is_none());
        let first = store.snapshot(1, "A", "hello world").unwrap().unwrap();
        assert_eq!(first.seq, 1);
        let again = store.snapshot(1, "A", "hello").unwrap().unwrap();
        assert_eq!(again.seq, 2);
        assert_eq!(store.blob_count().unwrap(), 2);
        assert_eq!(store.history(1, 10).unwrap().len(), 2);
        assert_eq!(store.dirty_notes().unwrap().len(), 1);
        store.clear_dirty(1).unwrap();
        assert!(store.dirty_notes().unwrap().is_empty());
        let qid = store.enqueue_create("Queued", "body").unwrap();
        assert_eq!(store.pending_creates().unwrap().len(), 1);
        store.remove_outbox(qid).unwrap();
        assert!(store.pending_creates().unwrap().is_empty());
    }
}
