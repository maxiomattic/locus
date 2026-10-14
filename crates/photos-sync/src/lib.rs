//! Thumbnail cache for the photo grid. First paint reads here; the ship
//! catalog + ZFS bytes fill holes. Not a second photo library.

use photos_protocol::{zfs_rel_path, Result as PathResult};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Catalog(#[from] photos_protocol::Error),
    #[error("{0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct ThumbCache {
    conn: Connection,
}

impl ThumbCache {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS thumbs (
                id INTEGER PRIMARY KEY,
                rel TEXT NOT NULL,
                jpeg BLOB NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn put(&self, id: u64, rel: &str, jpeg: &[u8]) -> Result<()> {
        let rel = zfs_rel_path(rel)?;
        self.conn.execute(
            "INSERT INTO thumbs(id, rel, jpeg) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET rel=excluded.rel, jpeg=excluded.jpeg",
            params![id as i64, rel, jpeg],
        )?;
        Ok(())
    }

    pub fn get(&self, id: u64) -> Result<Option<Vec<u8>>> {
        let mut stmt = self.conn.prepare("SELECT jpeg FROM thumbs WHERE id = ?1")?;
        let mut rows = stmt.query(params![id as i64])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    pub fn len(&self) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM thumbs", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("org.locus.photos")
        .join("thumbs.sqlite")
}

/// Grid paint: cache hit is instant; miss means fetch catalog/bytes next.
pub fn grid_jpeg(cache: &ThumbCache, id: u64) -> Result<Option<Vec<u8>>> {
    cache.get(id)
}

pub fn assert_rel(rel: &str) -> PathResult<&str> {
    zfs_rel_path(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn put_get_is_instant_hit() {
        let dir = TempDir::new().unwrap();
        let cache = ThumbCache::open(&dir.path().join("t.sqlite")).unwrap();
        cache.put(1, "a/b.jpg", b"\xff\xd8fake").unwrap();
        assert_eq!(grid_jpeg(&cache, 1).unwrap().as_deref(), Some(&b"\xff\xd8fake"[..]));
        assert!(grid_jpeg(&cache, 2).unwrap().is_none());
        assert_eq!(cache.len().unwrap(), 1);
    }

    #[test]
    fn cache_rejects_escape_paths() {
        let dir = TempDir::new().unwrap();
        let cache = ThumbCache::open(&dir.path().join("t.sqlite")).unwrap();
        assert!(cache.put(1, "../x", b"x").is_err());
    }
}
