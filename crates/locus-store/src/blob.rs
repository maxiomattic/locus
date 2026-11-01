//! Blobs: immutable bytes on disk, addressed by BLAKE3 hash.

use crate::{Result, Store, StoreError};
use locus_model::policy::{BLOB_PUT, READ};
use locus_model::{BlobHash, Decision, ObjectId, Principal, Request};
use rusqlite::{OptionalExtension, params};
use std::io::Write;
use std::path::PathBuf;

impl Store {
    pub fn blob_path(&self, h: &BlobHash) -> PathBuf {
        let s = h.to_string();
        self.dir().join("blobs").join(&s[..2]).join(&s[2..])
    }

    /// Store bytes, returning their address. Storing the same bytes twice is
    /// the same blob. Purged bytes stay refused, so a client that was offline
    /// cannot put them back.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<BlobHash> {
        let h = BlobHash(*blake3::hash(bytes).as_bytes());
        self.refuse_purged(&h)?;
        let path = self.blob_path(&h);
        if !path.exists() {
            let dir = path.parent().expect("blob paths have a parent");
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join(format!(".tmp-{}", locus_model::Id::new()));
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            std::fs::rename(&tmp, &path)?;
        }
        self.record_blob(&h, bytes.len() as u64)?;
        Ok(h)
    }

    /// Store a file's bytes without reading them all into memory.
    pub fn put_blob_file(&self, file: &std::path::Path) -> Result<BlobHash> {
        let mut hasher = blake3::Hasher::new();
        let mut f = std::fs::File::open(file)?;
        let size = std::io::copy(&mut f, &mut hasher)?;
        let h = BlobHash(*hasher.finalize().as_bytes());
        self.refuse_purged(&h)?;
        let path = self.blob_path(&h);
        if !path.exists() {
            let dir = path.parent().expect("blob paths have a parent");
            std::fs::create_dir_all(dir)?;
            let tmp = dir.join(format!(".tmp-{}", locus_model::Id::new()));
            std::fs::copy(file, &tmp)?;
            std::fs::rename(&tmp, &path)?;
        }
        self.record_blob(&h, size)?;
        Ok(h)
    }

    fn refuse_purged(&self, h: &BlobHash) -> Result<()> {
        let purged: Option<i64> = self
            .conn()
            .query_row(
                "SELECT at FROM purged_blobs WHERE hash=?1",
                [h.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        if purged.is_some() {
            return Err(StoreError::Refused(format!("blob {h} was purged")));
        }
        Ok(())
    }

    fn record_blob(&self, h: &BlobHash, size: u64) -> Result<()> {
        self.conn().execute(
            "INSERT OR IGNORE INTO blobs (hash, size, at) VALUES (?1, ?2, ?3)",
            params![h.to_string(), size, locus_model::now_ms()],
        )?;
        Ok(())
    }

    /// Upload as an actor, which is an audited action.
    pub fn put_blob_as(&self, who: &Principal, bytes: &[u8]) -> Result<BlobHash> {
        let req = Request {
            action: BLOB_PUT,
            type_name: "blob",
            object: None,
            implementation: None,
        };
        match self.policy().decide(who, &req, self.registry()) {
            Decision::Apply | Decision::Propose => {}
            Decision::Refuse(why) => {
                self.audit_row(who, BLOB_PUT, None, None, "refused", &why, "")?;
                return Err(StoreError::Refused(why));
            }
        }
        let h = self.put_blob(bytes)?;
        self.audit_row(who, BLOB_PUT, None, None, "applied", &h.to_string(), "")?;
        Ok(h)
    }

    pub fn has_blob(&self, h: &BlobHash) -> Result<bool> {
        Ok(self.blob_size(h)?.is_some())
    }

    pub fn blob_size(&self, h: &BlobHash) -> Result<Option<u64>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT size FROM blobs WHERE hash=?1",
                [h.to_string()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Read bytes, verifying them against their address.
    pub fn get_blob(&self, h: &BlobHash) -> Result<Vec<u8>> {
        let bytes = std::fs::read(self.blob_path(h))?;
        if blake3::hash(&bytes).as_bytes() != &h.0 {
            return Err(StoreError::Invalid(format!("blob {h} is damaged on disk")));
        }
        Ok(bytes)
    }

    /// May `who` read this blob? Only if it can read an object that holds it.
    pub fn may_read_blob(&self, who: &Principal, h: &BlobHash) -> Result<bool> {
        let mut s = self
            .conn()
            .prepare("SELECT object FROM blob_refs WHERE hash=?1")?;
        let ids = s.query_map([h.to_string()], |r| r.get::<_, String>(0))?;
        for id in ids {
            let id: ObjectId = id?
                .parse()
                .map_err(|_| StoreError::Invalid("bad id".to_string()))?;
            if let Some(o) = self.get(id)? {
                let req = Request {
                    action: READ,
                    type_name: &o.type_name,
                    object: Some(id),
                    implementation: None,
                };
                if !matches!(
                    self.policy().decide(who, &req, self.registry()),
                    Decision::Refuse(_)
                ) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Drop the local bytes. The address is not refused afterwards.
    pub(crate) fn forget_blob(&self, h: &BlobHash) -> Result<()> {
        match std::fs::remove_file(self.blob_path(h)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        self.conn()
            .execute("DELETE FROM blobs WHERE hash=?1", [h.to_string()])?;
        Ok(())
    }

    pub(crate) fn purge_blob(&self, h: &BlobHash) -> Result<()> {
        self.forget_blob(h)?;
        self.conn().execute(
            "INSERT OR REPLACE INTO purged_blobs (hash, at) VALUES (?1, ?2)",
            params![h.to_string(), locus_model::now_ms()],
        )?;
        Ok(())
    }
}
