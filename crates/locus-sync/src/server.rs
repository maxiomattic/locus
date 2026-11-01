//! The home server side: orders operations and checks permissions.

use crate::protocol::{Request, Response, SubmitResult, Subscription};
use locus_model::policy::BLOB_PUT;
use locus_model::{
    Decision, Effect, Id, Object, ObjectId, Op, Principal, Request as PolicyRequest, Value,
};
use locus_store::rusqlite::{self, OptionalExtension, params};
use locus_store::{LogEntry, Outcome, Store, StoreError};
use std::collections::BTreeSet;
use std::io::{Seek, SeekFrom, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const MAX_ENTRIES: u32 = 1000;
const MAX_CHUNK: u32 = 4 * 1024 * 1024;

/// A store acting as a home server. Clones share it.
#[derive(Clone)]
pub struct HomeServer {
    inner: Arc<Inner>,
}

struct Inner {
    store: Mutex<Store>,
    changed: Condvar,
}

fn sql_error(e: rusqlite::Error) -> StoreError {
    StoreError::Sql(e)
}

impl HomeServer {
    pub fn new(store: Store) -> Self {
        HomeServer {
            inner: Arc::new(Inner {
                store: Mutex::new(store),
                changed: Condvar::new(),
            }),
        }
    }

    /// Use the store directly, for setup and for the host process.
    pub fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> T) -> T {
        let mut s = self.inner.store.lock().expect("store lock");
        let out = f(&mut s);
        self.inner.changed.notify_all();
        out
    }

    /// Serve one request from an authenticated principal.
    pub fn handle(&self, who: &Principal, req: Request) -> Response {
        let result = match req {
            Request::Pull {
                client,
                since,
                sub,
                limit,
                wait_ms,
            } => self.pull(who, &client, since, &sub, limit, wait_ms),
            Request::Submit { ops } => self.submit(who, ops),
            Request::GetBlob { hash, offset, len } => self.with_store(|s| {
                if !s.may_read_blob(who, &hash)? {
                    return Err(StoreError::Refused(format!(
                        "{} may not read {hash}",
                        who.actor
                    )));
                }
                read_chunk(s, &hash, offset, len)
            }),
            Request::PutBlob {
                hash,
                offset,
                bytes,
                last,
            } => self.with_store(|s| put_chunk(s, who, hash, offset, &bytes, last)),
            Request::History { object } => self.with_store(|s| {
                s.get_as(who, object)?;
                Ok(Response::History {
                    entries: s.history(object)?,
                })
            }),
        };
        result.unwrap_or_else(|e| Response::Error(e.to_string()))
    }

    fn submit(&self, who: &Principal, ops: Vec<Op>) -> Result<Response, StoreError> {
        let results = self.with_store(|s| {
            ops.into_iter()
                .map(|op| match s.execute(op, who) {
                    Ok(Outcome::Applied { seq, .. } | Outcome::Duplicate { seq }) => {
                        Ok(SubmitResult::Accepted { seq })
                    }
                    Ok(Outcome::Proposed { proposal, .. }) => {
                        Ok(SubmitResult::Proposed { proposal })
                    }
                    Err(
                        e @ (StoreError::Refused(_) | StoreError::Plan(_) | StoreError::Invalid(_)),
                    ) => Ok(SubmitResult::Refused {
                        reason: e.to_string(),
                    }),
                    Err(e) => Err(e),
                })
                .collect::<Result<Vec<_>, StoreError>>()
        })?;
        Ok(Response::Submitted { results })
    }

    fn pull(
        &self,
        who: &Principal,
        client: &str,
        since: i64,
        sub: &Subscription,
        limit: u32,
        wait_ms: u32,
    ) -> Result<Response, StoreError> {
        let deadline = Instant::now() + Duration::from_millis(wait_ms.into());
        let mut guard = self.inner.store.lock().expect("store lock");
        loop {
            let reply = pull_now(&mut guard, who, client, since, sub, limit)?;
            let idle = matches!(&reply, Response::Pulled { entries, evict, .. }
                if entries.is_empty() && evict.is_empty());
            let left = deadline.saturating_duration_since(Instant::now());
            if !idle || left.is_zero() {
                return Ok(reply);
            }
            guard = self
                .inner
                .changed
                .wait_timeout(guard, left)
                .expect("store lock")
                .0;
        }
    }
}

fn read_chunk(
    s: &Store,
    hash: &locus_model::BlobHash,
    offset: u64,
    len: u32,
) -> Result<Response, StoreError> {
    use std::io::Read;
    let total = s
        .blob_size(hash)?
        .ok_or_else(|| StoreError::NotFound(hash.to_string()))?;
    let mut f = std::fs::File::open(s.blob_path(hash))?;
    f.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    f.take(len.min(MAX_CHUNK).into()).read_to_end(&mut bytes)?;
    Ok(Response::Blob { bytes, total })
}

fn put_chunk(
    s: &mut Store,
    who: &Principal,
    hash: locus_model::BlobHash,
    offset: u64,
    bytes: &[u8],
    last: bool,
) -> Result<Response, StoreError> {
    let req = PolicyRequest {
        action: BLOB_PUT,
        type_name: "blob",
        object: None,
        implementation: None,
    };
    if let Decision::Refuse(why) = s.policy().decide(who, &req, s.registry()) {
        return Err(StoreError::Refused(why));
    }
    let tmp = s
        .dir()
        .join("blobs")
        .join(format!(".in-{}-{hash}", who.actor));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(offset == 0)
        .open(&tmp)?;
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(bytes)?;
    drop(f);
    if last {
        let stored = s.put_blob_file(&tmp);
        std::fs::remove_file(&tmp)?;
        let got = stored?;
        if got != hash {
            return Err(StoreError::Invalid(format!(
                "uploaded bytes hash to {got}, not {hash}"
            )));
        }
    }
    Ok(Response::BlobStored)
}

/// True if this client should hold this object.
fn wanted(s: &Store, who: &Principal, sub: &Subscription, o: &Object) -> bool {
    if o.type_name == "locus.type" || o.id == who.actor {
        return true;
    }
    if o.type_name == "locus.grant"
        && o.fields.get("subject").and_then(Value::as_ref_id) == Some(who.actor)
    {
        return true;
    }
    (sub.all || sub.types.contains(&o.type_name) || sub.objects.contains(&o.id))
        && s.policy().may_read(who, o, s.registry())
}

fn held(s: &Store, key: &str) -> Result<BTreeSet<ObjectId>, StoreError> {
    let mut q = s
        .conn()
        .prepare("SELECT object FROM replica_view WHERE client=?1")?;
    let rows = q.query_map([key], |r| r.get::<_, String>(0))?;
    let mut out = BTreeSet::new();
    for r in rows {
        out.insert(
            r?.parse()
                .map_err(|_| StoreError::Invalid("bad id in replica_view".to_string()))?,
        );
    }
    Ok(out)
}

fn hold(s: &Store, key: &str, id: ObjectId) -> Result<(), StoreError> {
    s.conn()
        .execute(
            "INSERT OR IGNORE INTO replica_view (client, object) VALUES (?1, ?2)",
            params![key, id.to_string()],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn snapshot(o: Object, head: i64) -> LogEntry {
    let mut op = Op::new(o.id, "sync.snapshot", Default::default(), Id::SYSTEM);
    op.at = o.updated_at;
    LogEntry {
        seq: 0,
        rev: o.rev,
        home: o.home.clone(),
        home_seq: head,
        effects: vec![Effect::Snapshot(Box::new(o))],
        op,
    }
}

fn pull_now(
    s: &mut Store,
    who: &Principal,
    client: &str,
    since: i64,
    sub: &Subscription,
    limit: u32,
) -> Result<Response, StoreError> {
    let key = format!("{}:{client}", who.actor);
    if !matches!(
        s.policy().kind(who.actor),
        Some(locus_model::ActorKind::Human { .. } | locus_model::ActorKind::Agent { .. })
    ) {
        return Err(StoreError::Refused(format!(
            "{} is not known here",
            who.actor
        )));
    }
    let state = format!("{}|{}", s.epoch(), serde_json::to_string(sub)?);
    let old: Option<String> = s
        .conn()
        .query_row(
            "SELECT key FROM replica_state WHERE client=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()?;
    let head = s.head()?;
    let mut entries = Vec::new();
    let mut evict = Vec::new();
    let mut holding = held(s, &key)?;
    // Grants or the subscription changed, so what the client holds may too.
    if old.as_deref() != Some(&state) {
        for o in s.all_objects()? {
            let want = wanted(s, who, sub, &o);
            let has = holding.contains(&o.id);
            if want && !has {
                hold(s, &key, o.id)?;
                holding.insert(o.id);
                entries.push(snapshot(o, head));
            } else if !want && has {
                s.conn().execute(
                    "DELETE FROM replica_view WHERE client=?1 AND object=?2",
                    params![key, o.id.to_string()],
                )?;
                holding.remove(&o.id);
                evict.push(o.id);
            }
        }
        s.conn().execute(
            "INSERT OR REPLACE INTO replica_state (client, key) VALUES (?1, ?2)",
            params![key, state],
        )?;
    }
    let batch = s.log_since(since, limit.clamp(1, MAX_ENTRIES).into())?;
    let more = batch.len() == limit.clamp(1, MAX_ENTRIES) as usize;
    let last = batch.last().map_or(since, |e| e.seq);
    for e in batch {
        let id = e.op.object;
        if holding.contains(&id) {
            entries.push(e);
        } else if let Some(o) = s.get(id)?
            && wanted(s, who, sub, &o)
        {
            hold(s, &key, id)?;
            holding.insert(id);
            if e.rev == 1 {
                entries.push(e);
            } else {
                entries.push(snapshot(o, head));
            }
        }
    }
    Ok(Response::Pulled {
        entries,
        evict,
        head: if more { last } else { head.max(last) },
        more,
    })
}
