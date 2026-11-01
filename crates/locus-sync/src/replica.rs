//! The client side: a partial copy of each home, changed at once.
//!
//! The confirmed state holds what the home servers have decided. Unconfirmed
//! operations sit in `pending`, and `overlay` holds the objects they touch as
//! they would look with those operations applied. A read prefers the overlay.
//! When the server's order arrives it goes into the confirmed state, and the
//! pending operations replay on top of it.

use crate::protocol::{Request, Response, SubmitResult, Subscription, Transport, Unreachable};
use locus_model::proposal::{DiffEntry, diff, encode_ops, inverse, proposal_ops};
use locus_model::{
    BlobHash, Effect, Fields, Id, Object, ObjectId, Op, OpId, PlanError, Principal, Purge,
    Reference, ServerId, Value, ViewSpec, apply, op, plan,
};
use locus_store::rusqlite::{OptionalExtension, params};
use locus_store::{LogEntry, Store, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CHUNK: u32 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Offline(#[from] Unreachable),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("the server said: {0}")]
    Server(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = SyncError> = std::result::Result<T, E>;

struct Link {
    transport: Box<dyn Transport>,
    sub: Subscription,
}

/// An object as this client sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Visible {
    pub object: Object,
    /// It includes changes the home server has not accepted yet.
    pub provisional: bool,
    /// When this client last heard from the object's home.
    pub as_of: Option<i64>,
}

/// An operation the server would not take. Nothing is thrown away: the
/// operation stays here so its content can be looked at or redone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Refusal {
    pub op: Op,
    pub reason: String,
    pub at: i64,
}

/// What one sync did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SyncReport {
    pub submitted: usize,
    pub accepted: usize,
    pub refused: usize,
    pub proposed: Vec<ObjectId>,
    pub received: usize,
    pub evicted: usize,
}

pub struct Replica {
    store: Store,
    who: Principal,
    client: String,
    links: BTreeMap<ServerId, Link>,
}

impl Replica {
    /// A replica over a store. `client` names this copy to its home servers.
    pub fn new(store: Store, who: Principal, client: &str) -> Self {
        Replica {
            store,
            who,
            client: client.to_string(),
            links: BTreeMap::new(),
        }
    }

    pub fn principal(&self) -> Principal {
        self.who
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Connect a home server. Replaces any earlier connection to it.
    pub fn connect(&mut self, home: &str, transport: Box<dyn Transport>, sub: Subscription) {
        self.links
            .insert(ServerId(home.to_string()), Link { transport, sub });
    }

    fn link(&self, home: &ServerId) -> Result<&Link> {
        self.links
            .get(home)
            .ok_or_else(|| SyncError::Invalid(format!("not connected to {home}")))
    }

    // Reading.

    fn overlay(&self, id: ObjectId) -> Result<Option<Object>> {
        let doc: Option<String> = self
            .store
            .conn()
            .query_row(
                "SELECT doc FROM overlay WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()
            .map_err(StoreError::from)?;
        doc.map(|d| serde_json::from_str(&d).map_err(|e| StoreError::Json(e).into()))
            .transpose()
    }

    fn current(&self, id: ObjectId) -> Result<Option<Object>> {
        match self.overlay(id)? {
            Some(o) => Ok(Some(o)),
            None => Ok(self.store.get(id)?),
        }
    }

    pub fn get(&self, id: ObjectId) -> Result<Option<Visible>> {
        let Some(over) = self.overlay(id)? else {
            let Some(object) = self.store.get(id)? else {
                return Ok(None);
            };
            let as_of = self.synced_at(&object.home)?;
            return Ok(Some(Visible {
                object,
                provisional: false,
                as_of,
            }));
        };
        let as_of = self.synced_at(&over.home)?;
        Ok(Some(Visible {
            object: over,
            provisional: true,
            as_of,
        }))
    }

    /// Follow a reference. A reference to another clan's object gives the
    /// cached copy, and `as_of` shows how stale it is.
    pub fn resolve(&self, r: &Reference) -> Result<Option<Visible>> {
        match r.pin {
            None => self.get(r.id),
            Some(rev) => {
                let Some(object) = self.store.object_at(r.id, rev)? else {
                    return Ok(None);
                };
                let as_of = self.synced_at(&object.home)?;
                Ok(Some(Visible {
                    object,
                    provisional: false,
                    as_of,
                }))
            }
        }
    }

    fn synced_at(&self, home: &ServerId) -> Result<Option<i64>> {
        Ok(self
            .store
            .conn()
            .query_row(
                "SELECT synced_at FROM cursors WHERE home=?1",
                [&home.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(StoreError::from)?)
    }

    fn all_visible(&self) -> Result<Vec<Object>> {
        let mut objs: BTreeMap<ObjectId, Object> = self
            .store
            .all_objects()?
            .into_iter()
            .map(|o| (o.id, o))
            .collect();
        let mut s = self
            .store
            .conn()
            .prepare("SELECT doc FROM overlay")
            .map_err(StoreError::from)?;
        let rows = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(StoreError::from)?;
        for d in rows {
            let o: Object =
                serde_json::from_str(&d.map_err(StoreError::from)?).map_err(StoreError::from)?;
            objs.insert(o.id, o);
        }
        Ok(objs.into_values().collect())
    }

    /// A live query over this client's copy, unconfirmed changes included.
    pub fn view(&self, spec: &ViewSpec) -> Result<Vec<Object>> {
        Ok(locus_model::evaluate(
            spec,
            self.all_visible()?,
            self.store.registry(),
        ))
    }

    /// Objects with a conflict waiting for a decision.
    pub fn conflicts(&self) -> Result<Vec<Object>> {
        Ok(self
            .all_visible()?
            .into_iter()
            .filter(|o| !o.conflicts.is_empty())
            .collect())
    }

    /// Operations not yet accepted by their home server.
    pub fn provisional(&self) -> Result<Vec<Op>> {
        Ok(self.pending()?.into_iter().map(|(_, o)| o).collect())
    }

    fn pending(&self) -> Result<Vec<(ServerId, Op)>> {
        let mut s = self
            .store
            .conn()
            .prepare("SELECT home, op FROM pending ORDER BY n")
            .map_err(StoreError::from)?;
        let rows = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(StoreError::from)?;
        let mut out = Vec::new();
        for r in rows {
            let (home, op) = r.map_err(StoreError::from)?;
            out.push((
                ServerId(home),
                serde_json::from_str(&op).map_err(StoreError::from)?,
            ));
        }
        Ok(out)
    }

    /// Operations the server refused, with the reason.
    pub fn refused(&self) -> Result<Vec<Refusal>> {
        let mut s = self
            .store
            .conn()
            .prepare("SELECT op, reason, at FROM refused ORDER BY at")
            .map_err(StoreError::from)?;
        let rows = s
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(StoreError::from)?;
        let mut out = Vec::new();
        for r in rows {
            let (op, reason, at) = r.map_err(StoreError::from)?;
            out.push(Refusal {
                op: serde_json::from_str(&op).map_err(StoreError::from)?,
                reason,
                at,
            });
        }
        Ok(out)
    }

    // Writing.

    /// Create an object, at once and offline if need be.
    pub fn create(
        &mut self,
        home: &str,
        ty: &str,
        fields: Fields,
        body: Option<&str>,
        intent: &str,
    ) -> Result<ObjectId> {
        let id = Id::new();
        let mut args = locus_model::fields([
            ("type", ty.into()),
            ("home", home.into()),
            ("fields", Value::Map(fields)),
        ]);
        if let Some(b) = body {
            args.insert("body".to_string(), b.into());
        }
        self.submit_local(Op::new(id, "object.create", args, self.who.actor), intent)?;
        Ok(id)
    }

    /// Run an operation on an object, at once and offline if need be.
    pub fn act(&mut self, id: ObjectId, name: &str, args: Fields, intent: &str) -> Result<OpId> {
        let doc = self.current(id)?;
        let op =
            Op::new(id, name, args, self.who.actor).seeing(doc.as_ref(), self.store.registry());
        self.submit_local(op, intent)
    }

    /// Set fields, the most common operation.
    pub fn set(&mut self, id: ObjectId, fields: Fields, intent: &str) -> Result<OpId> {
        self.act(
            id,
            "object.set",
            locus_model::fields([("fields", Value::Map(fields))]),
            intent,
        )
    }

    fn submit_local(&mut self, mut op: Op, intent: &str) -> Result<OpId> {
        op.invoker = self.who.invoker;
        op.intent = intent.to_string();
        let doc = self.current(op.object)?;
        let effects = plan(
            doc.as_ref(),
            &op,
            self.store.registry(),
            self.store.body_check(),
        )?;
        let home = home_of(doc.as_ref(), &op)?;
        let mut next = doc;
        apply(&mut next, &effects, op.object, op.at);
        if let Some(d) = &next {
            self.put_overlay(d)?;
        }
        self.store
            .conn()
            .execute(
                "INSERT INTO pending (home, op_id, op) VALUES (?1, ?2, ?3)",
                params![
                    home.0,
                    op.id.to_string(),
                    serde_json::to_string(&op).map_err(StoreError::from)?
                ],
            )
            .map_err(StoreError::from)?;
        Ok(op.id)
    }

    fn put_overlay(&self, d: &Object) -> Result<()> {
        self.store
            .conn()
            .execute(
                "INSERT OR REPLACE INTO overlay (id, doc) VALUES (?1, ?2)",
                params![
                    d.id.to_string(),
                    serde_json::to_string(d).map_err(StoreError::from)?
                ],
            )
            .map_err(StoreError::from)?;
        Ok(())
    }

    /// Replay what is still pending on top of the confirmed state, in the
    /// order it was made. The same merge code runs as on the server, so a
    /// conflict shows here before the server rules on it.
    fn replay(&mut self) -> Result<()> {
        self.store
            .conn()
            .execute("DELETE FROM overlay", [])
            .map_err(StoreError::from)?;
        for (_, op) in self.pending()? {
            let doc = self.current(op.object)?;
            let reg = self.store.registry();
            // An op that no longer plans stays pending. The server's answer
            // is the one that counts, and it will land in `refused`.
            if let Ok(e) = plan(doc.as_ref(), &op, reg, self.store.body_check()) {
                let mut next = doc;
                apply(&mut next, &e, op.object, op.at);
                if let Some(d) = &next {
                    self.put_overlay(d)?;
                }
            }
        }
        Ok(())
    }

    // Blobs.

    /// Keep bytes here, to upload at the next sync with `home`.
    pub fn put_blob(&mut self, home: &str, bytes: &[u8]) -> Result<BlobHash> {
        let h = self.store.put_blob(bytes)?;
        self.store
            .conn()
            .execute(
                "INSERT OR IGNORE INTO pending_blobs (hash, home) VALUES (?1, ?2)",
                params![h.to_string(), home],
            )
            .map_err(StoreError::from)?;
        Ok(h)
    }

    /// Bytes of a blob, fetched from `home` and verified if not held here.
    pub fn blob(&mut self, home: &str, h: &BlobHash) -> Result<Vec<u8>> {
        if self.store.has_blob(h)? {
            return Ok(self.store.get_blob(h)?);
        }
        let link = self.link(&ServerId(home.to_string()))?;
        let mut bytes = Vec::new();
        loop {
            let reply = link.transport.call(Request::GetBlob {
                hash: *h,
                offset: bytes.len() as u64,
                len: CHUNK,
            })?;
            match reply {
                Response::Blob { bytes: b, total } => {
                    let empty = b.is_empty();
                    bytes.extend(b);
                    if bytes.len() as u64 >= total {
                        break;
                    }
                    if empty {
                        return Err(SyncError::Server("the blob ended early".to_string()));
                    }
                }
                other => return Err(unexpected(other)),
            }
        }
        if blake3::hash(&bytes).as_bytes() != &h.0 {
            return Err(SyncError::Server(
                "the blob's bytes do not match".to_string(),
            ));
        }
        self.store.put_blob(&bytes)?;
        Ok(bytes)
    }

    fn upload_blobs(&self, home: &ServerId, link: &Link) -> Result<()> {
        let hashes: Vec<String> = {
            let mut s = self
                .store
                .conn()
                .prepare("SELECT hash FROM pending_blobs WHERE home=?1")
                .map_err(StoreError::from)?;
            s.query_map([&home.0], |r| r.get(0))
                .map_err(StoreError::from)?
                .collect::<Result<_, _>>()
                .map_err(StoreError::from)?
        };
        for hs in hashes {
            let h: BlobHash = hs
                .parse()
                .map_err(|_| SyncError::Invalid("bad blob hash".to_string()))?;
            let bytes = self.store.get_blob(&h)?;
            let mut offset = 0;
            loop {
                let end = (offset + CHUNK as usize).min(bytes.len());
                let last = end == bytes.len();
                let reply = link.transport.call(Request::PutBlob {
                    hash: h,
                    offset: offset as u64,
                    bytes: bytes[offset..end].to_vec(),
                    last,
                })?;
                match reply {
                    Response::BlobStored => {}
                    other => return Err(unexpected(other)),
                }
                offset = end;
                if last {
                    break;
                }
            }
            self.store
                .conn()
                .execute("DELETE FROM pending_blobs WHERE hash=?1", [&hs])
                .map_err(StoreError::from)?;
        }
        Ok(())
    }

    // Sync.

    /// Sync with every connected home. Fails on the first it cannot reach,
    /// leaving everything pending and safe to retry.
    pub fn sync(&mut self) -> Result<SyncReport> {
        let homes: Vec<ServerId> = self.links.keys().cloned().collect();
        let mut total = SyncReport::default();
        for h in homes {
            let r = self.sync_home(&h)?;
            total.submitted += r.submitted;
            total.accepted += r.accepted;
            total.refused += r.refused;
            total.proposed.extend(r.proposed);
            total.received += r.received;
            total.evicted += r.evicted;
        }
        Ok(total)
    }

    /// Send what is pending for one home, then take in its order.
    pub fn sync_home(&mut self, home: &ServerId) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        let ops: Vec<Op> = self
            .pending()?
            .into_iter()
            .filter(|(h, _)| h == home)
            .map(|(_, o)| o)
            .collect();
        {
            let link = self.link(home)?;
            self.upload_blobs(home, link)?;
        }
        let mut settled = Vec::new();
        for chunk in ops.chunks(200) {
            report.submitted += chunk.len();
            let reply = self.link(home)?.transport.call(Request::Submit {
                ops: chunk.to_vec(),
            })?;
            let Response::Submitted { results } = reply else {
                return Err(unexpected(reply));
            };
            for (op, r) in chunk.iter().zip(results) {
                match r {
                    SubmitResult::Accepted { .. } => {
                        report.accepted += 1;
                        settled.push(op.id);
                    }
                    SubmitResult::Proposed { proposal } => {
                        report.proposed.push(proposal);
                        settled.push(op.id);
                    }
                    SubmitResult::Refused { reason } => {
                        report.refused += 1;
                        self.refuse(op, &reason)?;
                    }
                }
            }
        }
        self.pull(home, 0, &mut report)?;
        // Only now, so a client that lost the reply resubmits, and the server
        // recognises the operation IDs.
        for id in settled {
            self.drop_pending(id)?;
        }
        self.replay()?;
        Ok(report)
    }

    fn drop_pending(&self, id: OpId) -> Result<()> {
        self.store
            .conn()
            .execute("DELETE FROM pending WHERE op_id=?1", [id.to_string()])
            .map_err(StoreError::from)?;
        Ok(())
    }

    fn refuse(&self, op: &Op, reason: &str) -> Result<()> {
        self.store
            .conn()
            .execute(
                "INSERT OR REPLACE INTO refused (op_id, op, reason, at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    op.id.to_string(),
                    serde_json::to_string(op).map_err(StoreError::from)?,
                    reason,
                    locus_model::now_ms()
                ],
            )
            .map_err(StoreError::from)?;
        self.drop_pending(op.id)
    }

    /// Take in the server's order. `wait_ms` holds the first request open
    /// until something happens, for a client that wants live updates.
    pub fn pull(&mut self, home: &ServerId, wait_ms: u32, report: &mut SyncReport) -> Result<()> {
        loop {
            let since = self.cursor(home)?;
            let sub = self.link(home)?.sub.clone();
            let reply = self.link(home)?.transport.call(Request::Pull {
                client: self.client.clone(),
                since,
                sub,
                limit: 500,
                wait_ms,
            })?;
            let Response::Pulled {
                entries,
                evict,
                head,
                more,
            } = reply
            else {
                return Err(unexpected(reply));
            };
            report.received += entries.len();
            report.evicted += evict.len();
            for e in &entries {
                self.take_entry(e)?;
            }
            for id in evict {
                self.store.evict(id)?;
            }
            self.set_cursor(home, head)?;
            if !more {
                return Ok(());
            }
        }
    }

    /// Follow a live server: wait up to `wait_ms` for news, then replay.
    pub fn wait(&mut self, home: &ServerId, wait_ms: u32) -> Result<SyncReport> {
        let mut report = SyncReport::default();
        self.pull(home, wait_ms, &mut report)?;
        self.replay()?;
        Ok(report)
    }

    fn take_entry(&mut self, e: &LogEntry) -> Result<()> {
        let mut secrets = Vec::new();
        if let Some(Purge::Field(f)) = purge_in(e) {
            for h in self.store.history(e.op.object)? {
                secrets.extend(op::field_values(&f, &h.effects));
            }
        }
        self.store.apply_entry(e)?;
        if let Some(scope) = purge_in(e) {
            self.scrub_unconfirmed(e.op.object, &scope, &mut secrets)?;
        }
        Ok(())
    }

    /// A purge also reaches what has not been synced: pending operations and
    /// refused ones may hold the same data.
    fn scrub_unconfirmed(
        &self,
        object: ObjectId,
        scope: &Purge,
        secrets: &mut Vec<Value>,
    ) -> Result<()> {
        for (_, mut o) in self.pending()? {
            if o.object == object || matches!(scope, Purge::Blob(_)) {
                if let Purge::Field(f) = scope {
                    for (name, v) in o.seen.iter() {
                        if name == f {
                            secrets.extend(v.clone());
                        }
                    }
                }
                if matches!(scope, Purge::Object) {
                    self.drop_pending(o.id)?;
                    continue;
                }
                if op::scrub_entry(&mut o, &mut [], scope, secrets) {
                    self.store
                        .conn()
                        .execute(
                            "UPDATE pending SET op=?1 WHERE op_id=?2",
                            params![
                                serde_json::to_string(&o).map_err(StoreError::from)?,
                                o.id.to_string()
                            ],
                        )
                        .map_err(StoreError::from)?;
                }
            }
        }
        for mut r in self.refused()? {
            if (r.op.object == object || matches!(scope, Purge::Blob(_)))
                && op::scrub_entry(&mut r.op, &mut [], scope, secrets)
            {
                self.store
                    .conn()
                    .execute(
                        "UPDATE refused SET op=?1 WHERE op_id=?2",
                        params![
                            serde_json::to_string(&r.op).map_err(StoreError::from)?,
                            r.op.id.to_string()
                        ],
                    )
                    .map_err(StoreError::from)?;
            }
        }
        Ok(())
    }

    fn cursor(&self, home: &ServerId) -> Result<i64> {
        Ok(self
            .store
            .conn()
            .query_row("SELECT seq FROM cursors WHERE home=?1", [&home.0], |r| {
                r.get(0)
            })
            .optional()
            .map_err(StoreError::from)?
            .unwrap_or(0))
    }

    fn set_cursor(&self, home: &ServerId, seq: i64) -> Result<()> {
        self.store
            .conn()
            .execute(
                "INSERT OR REPLACE INTO cursors (home, seq, synced_at) VALUES (?1, ?2, ?3)",
                params![home.0, seq, locus_model::now_ms()],
            )
            .map_err(StoreError::from)?;
        Ok(())
    }

    // Recovery and review.

    /// Full history from the object's home, which holds all of it.
    pub fn history(&self, id: ObjectId) -> Result<Vec<LogEntry>> {
        let home = self
            .get(id)?
            .map(|v| v.object.home)
            .ok_or_else(|| SyncError::Invalid(format!("{id} is not held here")))?;
        match self
            .link(&home)?
            .transport
            .call(Request::History { object: id })?
        {
            Response::History { entries } => Ok(entries),
            other => Err(unexpected(other)),
        }
    }

    /// Undo an entry with new operations. They apply at once and stay
    /// provisional like any other change.
    pub fn revert(&mut self, op_id: OpId, object: ObjectId, intent: &str) -> Result<()> {
        let local = self.store.history(object)?;
        let found = local.into_iter().find(|e| e.op.id == op_id);
        let entry = match found {
            Some(e) => e,
            None => self
                .history(object)?
                .into_iter()
                .find(|e| e.op.id == op_id)
                .ok_or_else(|| SyncError::Invalid(format!("no entry {op_id}")))?,
        };
        let steps = inverse(&entry.op, &entry.effects).map_err(SyncError::Invalid)?;
        for (name, args) in steps {
            self.act(object, &name, args, intent)?;
        }
        Ok(())
    }

    /// Propose operations as a branch. Build them with [`Replica::draft`].
    pub fn propose(&mut self, home: &str, title: &str, ops: &[Op]) -> Result<ObjectId> {
        let id = Id::new();
        let op = Op::new(
            id,
            "proposal.create",
            locus_model::fields([
                ("title", title.into()),
                ("home", home.into()),
                ("ops", encode_ops(ops)),
            ]),
            self.who.actor,
        );
        self.submit_local(op, title)?;
        Ok(id)
    }

    /// An operation for a proposal: built but not applied anywhere.
    pub fn draft(&self, id: ObjectId, name: &str, args: Fields, intent: &str) -> Result<Op> {
        let doc = self.current(id)?;
        let mut op = Op::new(id, name, args, self.who.actor)
            .seeing(doc.as_ref(), self.store.registry())
            .intent(intent);
        op.invoker = self.who.invoker;
        Ok(op)
    }

    /// A proposal as a diff against what this client sees.
    pub fn review(&self, id: ObjectId) -> Result<Vec<DiffEntry>> {
        let doc = self
            .current(id)?
            .ok_or_else(|| SyncError::Invalid(format!("{id} is not held here")))?;
        let ops = proposal_ops(&doc).map_err(SyncError::Invalid)?;
        Ok(diff(
            &ops,
            |o| self.current(o).ok().flatten(),
            self.store.registry(),
            self.store.body_check(),
        ))
    }

    /// Homes with changes still waiting.
    pub fn homes_waiting(&self) -> Result<BTreeSet<ServerId>> {
        Ok(self.pending()?.into_iter().map(|(h, _)| h).collect())
    }
}

fn purge_in(e: &LogEntry) -> Option<Purge> {
    e.effects.iter().find_map(|x| match x {
        Effect::Purged(p) => Some(p.clone()),
        _ => None,
    })
}

fn home_of(doc: Option<&Object>, op: &Op) -> Result<ServerId> {
    if let Some(d) = doc {
        return Ok(d.home.clone());
    }
    op.args
        .get("home")
        .and_then(Value::as_text)
        .map(ServerId::from)
        .ok_or_else(|| SyncError::Invalid("the operation has no home".to_string()))
}

fn unexpected(r: Response) -> SyncError {
    match r {
        Response::Error(e) => SyncError::Server(e),
        other => SyncError::Server(format!("unexpected reply: {other:?}")),
    }
}
