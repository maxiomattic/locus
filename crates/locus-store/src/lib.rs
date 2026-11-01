//! The Locus store: one SQLite database on each machine, plus blobs on disk.
//!
//! [`Store::execute`] is the operation layer. It checks permissions, plans the
//! operation, and writes the log entry, the new object state and the indexes
//! in one transaction, then audits the attempt. A store acts as the home
//! server for objects whose `home` is its own name. Replicas of other homes
//! use [`Store::apply_entry`], which takes a home's decisions as given.

mod blob;
mod react;
mod schema;

use locus_model::proposal::{DiffEntry, diff, encode_ops, inverse, proposal_ops};
use locus_model::{
    Decision, Effect, Fields, Filter, Id, Object, ObjectId, Op, OpId, PlanError, Policy, Principal,
    Purge, Reference, Registry, Request, ServerId, TypeDef, Value, ViewSpec, apply, op, plan,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub use rusqlite;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("refused: {0}")]
    Refused(String),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error("no such object or entry: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("database: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("disk: {0}")]
    Io(#[from] std::io::Error),
    #[error("encoding: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// One entry of the operation log.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: i64,
    pub op: Op,
    pub effects: Vec<Effect>,
    pub rev: u64,
    pub home: ServerId,
    pub home_seq: i64,
}

/// What happened to an executed operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outcome {
    Applied {
        seq: i64,
        effects: Vec<Effect>,
    },
    /// The actor may only propose this, so it became a proposal.
    Proposed {
        proposal: ObjectId,
        seq: i64,
    },
    /// This operation ID was applied before. Retries are harmless.
    Duplicate {
        seq: i64,
    },
}

/// One audit row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: i64,
    pub actor: String,
    pub invoker: Option<String>,
    pub action: String,
    pub object: Option<String>,
    pub op_id: Option<String>,
    pub decision: String,
    pub detail: String,
    pub intent: String,
}

/// The body rule in force. Replaced by the HTML sanitizer in `locus-text`.
pub fn default_body_check(html: &str) -> std::result::Result<String, String> {
    Ok(html.to_string())
}

pub struct Store {
    conn: Connection,
    dir: PathBuf,
    home: ServerId,
    reg: Registry,
    policy: Policy,
    body_check: op::BodyCheck,
    /// Bumped when grants or actors change, so replicas recheck what they hold.
    epoch: i64,
    in_reaction: bool,
}

fn open_conn(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;
         PRAGMA secure_delete=ON; PRAGMA busy_timeout=5000;",
    )?;
    Ok(conn)
}

impl Store {
    /// Open or create the store in `dir`. `home` names this machine when it
    /// acts as a home server.
    pub fn open(dir: impl AsRef<Path>, home: &str) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(dir.join("blobs"))?;
        let conn = open_conn(&dir.join("locus.db"))?;
        let exists: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='meta'",
            [],
            |r| r.get(0),
        )?;
        if exists == 0 {
            conn.execute_batch(&format!("BEGIN;{}COMMIT;", schema::SCHEMA))?;
            conn.execute(
                "INSERT INTO meta VALUES ('schema', ?1), ('home', ?2), ('epoch', '1')",
                params![schema::SCHEMA_VERSION.to_string(), home],
            )?;
        }
        let stored: String =
            conn.query_row("SELECT value FROM meta WHERE key='home'", [], |r| r.get(0))?;
        let version: String =
            conn.query_row("SELECT value FROM meta WHERE key='schema'", [], |r| {
                r.get(0)
            })?;
        if version != schema::SCHEMA_VERSION.to_string() {
            return Err(StoreError::Invalid(format!(
                "store schema {version} needs a migration"
            )));
        }
        let epoch: String =
            conn.query_row("SELECT value FROM meta WHERE key='epoch'", [], |r| r.get(0))?;
        let mut store = Store {
            conn,
            dir,
            home: ServerId(stored),
            reg: Registry::builtin(),
            policy: Policy::default(),
            body_check: default_body_check,
            epoch: epoch.parse().unwrap_or(1),
            in_reaction: false,
        };
        store.reload()?;
        Ok(store)
    }

    pub fn home(&self) -> &ServerId {
        &self.home
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn registry(&self) -> &Registry {
        &self.reg
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn epoch(&self) -> i64 {
        self.epoch
    }

    pub fn body_check(&self) -> op::BodyCheck {
        self.body_check
    }

    pub fn set_body_check(&mut self, check: op::BodyCheck) {
        self.body_check = check;
    }

    /// Reload the types and policy from their objects.
    pub fn reload(&mut self) -> Result<()> {
        let mut reg = Registry::builtin();
        for o in self.objects_of_type("locus.type")? {
            if o.deleted {
                continue;
            }
            let def = o.fields.get("def").and_then(Value::as_text).unwrap_or("{}");
            let def: TypeDef = serde_json::from_str(def)?;
            reg.define(def).map_err(StoreError::Invalid)?;
        }
        self.reg = reg;
        let mut objs = Vec::new();
        for t in ["locus.member", "locus.agent", "locus.grant"] {
            objs.extend(self.objects_of_type(t)?);
        }
        self.policy = Policy::from_objects(&objs);
        let epoch: String =
            self.conn
                .query_row("SELECT value FROM meta WHERE key='epoch'", [], |r| r.get(0))?;
        self.epoch = epoch.parse().unwrap_or(1);
        Ok(())
    }

    // Reading.

    pub fn get(&self, id: ObjectId) -> Result<Option<Object>> {
        let doc: Option<String> = self
            .conn
            .query_row(
                "SELECT doc FROM objects WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(doc.map(|d| serde_json::from_str(&d)).transpose()?)
    }

    /// An object, if `who` may read it.
    pub fn get_as(&self, who: &Principal, id: ObjectId) -> Result<Object> {
        let o = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        if self.policy.may_read(who, &o, &self.reg) {
            Ok(o)
        } else {
            self.audit_row(who, "read", Some(id), None, "refused", "no read grant", "")?;
            Err(StoreError::Refused(format!(
                "{} may not read {id}",
                who.actor
            )))
        }
    }

    pub fn objects_of_type(&self, ty: &str) -> Result<Vec<Object>> {
        let mut s = self.conn.prepare("SELECT doc FROM objects WHERE type=?1")?;
        let rows = s.query_map([ty], |r| r.get::<_, String>(0))?;
        rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
    }

    pub fn all_objects(&self) -> Result<Vec<Object>> {
        let mut s = self.conn.prepare("SELECT doc FROM objects")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
    }

    /// Evaluate a view, showing `who` only what it may read.
    pub fn view(&self, who: &Principal, spec: &ViewSpec) -> Result<Vec<Object>> {
        let objs = self
            .all_objects()?
            .into_iter()
            .filter(|o| self.policy.may_read(who, o, &self.reg))
            .collect();
        Ok(locus_model::evaluate(spec, objs, &self.reg))
    }

    /// A view saved as a `locus.view` object.
    pub fn saved_view(&self, who: &Principal, id: ObjectId) -> Result<Vec<Object>> {
        let v = self.get_as(who, id)?;
        let spec = v
            .fields
            .get("spec")
            .and_then(Value::as_text)
            .ok_or_else(|| StoreError::Invalid("the view has no spec".to_string()))?;
        self.view(who, &serde_json::from_str(spec)?)
    }

    /// Objects that reference `id`.
    pub fn refs_to(&self, id: ObjectId) -> Result<Vec<ObjectId>> {
        let mut s = self
            .conn
            .prepare("SELECT DISTINCT src FROM refs WHERE dst=?1")?;
        let rows = s.query_map([id.to_string()], |r| r.get::<_, String>(0))?;
        rows.map(|r| {
            r?.parse()
                .map_err(|_| StoreError::Invalid("bad id in refs".to_string()))
        })
        .collect()
    }

    fn entry_from(
        row: &rusqlite::Row,
    ) -> rusqlite::Result<(i64, String, String, u64, String, i64)> {
        Ok((
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
        ))
    }

    fn entries(&self, sql: &str, p: impl rusqlite::Params) -> Result<Vec<LogEntry>> {
        let mut s = self.conn.prepare(sql)?;
        let rows = s.query_map(p, Self::entry_from)?;
        let mut out = Vec::new();
        for r in rows {
            let (seq, op, effects, rev, home, home_seq) = r?;
            out.push(LogEntry {
                seq,
                op: serde_json::from_str(&op)?,
                effects: serde_json::from_str(&effects)?,
                rev,
                home: ServerId(home),
                home_seq,
            });
        }
        Ok(out)
    }

    /// The object's history, oldest first.
    pub fn history(&self, id: ObjectId) -> Result<Vec<LogEntry>> {
        self.entries(
            "SELECT seq, op, effects, rev, home, home_seq FROM log WHERE object=?1 ORDER BY seq",
            [id.to_string()],
        )
    }

    /// Log entries after `seq` at this home, oldest first.
    pub fn log_since(&self, seq: i64, limit: i64) -> Result<Vec<LogEntry>> {
        self.entries(
            "SELECT seq, op, effects, rev, home, home_seq FROM log WHERE seq>?1 ORDER BY seq LIMIT ?2",
            params![seq, limit],
        )
    }

    pub fn entry(&self, op: OpId) -> Result<LogEntry> {
        self.entries(
            "SELECT seq, op, effects, rev, home, home_seq FROM log WHERE op_id=?1",
            [op.to_string()],
        )?
        .pop()
        .ok_or_else(|| StoreError::NotFound(op.to_string()))
    }

    pub fn batch_entries(&self, batch: Id) -> Result<Vec<LogEntry>> {
        self.entries(
            "SELECT seq, op, effects, rev, home, home_seq FROM log WHERE batch=?1 ORDER BY seq",
            [batch.to_string()],
        )
    }

    /// The last sequence number of the log.
    pub fn head(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM log", [], |r| r.get(0))?)
    }

    /// An object as it was at a revision, rebuilt from its log. This is what
    /// resolves a pinned reference.
    pub fn object_at(&self, id: ObjectId, rev: u64) -> Result<Option<Object>> {
        let mut doc = None;
        for e in self.history(id)? {
            if e.rev > rev {
                break;
            }
            apply(&mut doc, &e.effects, id, e.op.at);
        }
        Ok(doc)
    }

    /// Follow a reference: a pinned one gives that revision.
    pub fn resolve(&self, r: &Reference) -> Result<Option<Object>> {
        match r.pin {
            Some(rev) => self.object_at(r.id, rev),
            None => self.get(r.id),
        }
    }

    /// Deleted objects that can still be restored.
    pub fn deleted(&self, who: &Principal) -> Result<Vec<Object>> {
        let mut s = self
            .conn
            .prepare("SELECT doc FROM objects WHERE deleted=1")?;
        let rows = s.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for d in rows {
            let o: Object = serde_json::from_str(&d?)?;
            if !o.purged && self.policy.may_read(who, &o, &self.reg) {
                out.push(o);
            }
        }
        Ok(out)
    }

    pub fn audit(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        let mut s = self.conn.prepare(
            "SELECT id, at, actor, invoker, action, object, op_id, decision, detail, intent
             FROM audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = s.query_map([limit], |r| {
            Ok(AuditEntry {
                id: r.get(0)?,
                at: r.get(1)?,
                actor: r.get(2)?,
                invoker: r.get(3)?,
                action: r.get(4)?,
                object: r.get(5)?,
                op_id: r.get(6)?,
                decision: r.get(7)?,
                detail: r.get(8)?,
                intent: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn audit_row(
        &self,
        who: &Principal,
        action: &str,
        object: Option<ObjectId>,
        op_id: Option<OpId>,
        decision: &str,
        detail: &str,
        intent: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO audit (at, actor, invoker, action, object, op_id, decision, detail, intent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                locus_model::now_ms(),
                who.actor.to_string(),
                who.invoker.map(|i| i.to_string()),
                action,
                object.map(|o| o.to_string()),
                op_id.map(|o| o.to_string()),
                decision,
                detail,
                intent,
            ],
        )?;
        Ok(())
    }

    // Writing.

    /// Create the first administrator of a new clan. Runs once.
    pub fn genesis(&mut self, admin_name: &str) -> Result<ObjectId> {
        if !self.objects_of_type("locus.member")?.is_empty() {
            return Err(StoreError::Invalid(
                "this clan already has members".to_string(),
            ));
        }
        let id = Id::new();
        let op = self.create_op(
            id,
            "locus.member",
            locus_model::fields([("title", admin_name.into()), ("admin", true.into())]),
            None,
            Id::SYSTEM,
        );
        self.execute(
            op.intent("create the clan's administrator"),
            &Principal::SYSTEM,
        )?;
        Ok(id)
    }

    /// An `object.create` for this home.
    pub fn create_op(
        &self,
        id: ObjectId,
        ty: &str,
        fields: Fields,
        body: Option<&str>,
        actor: ObjectId,
    ) -> Op {
        let mut args = locus_model::fields([
            ("type", ty.into()),
            ("home", self.home.0.as_str().into()),
            ("fields", Value::Map(fields)),
        ]);
        if let Some(b) = body {
            args.insert("body".to_string(), b.into());
        }
        Op::new(id, "object.create", args, actor)
    }

    /// An operation on an existing object, recording what the author sees.
    pub fn op_on(&self, id: ObjectId, name: &str, args: Fields, who: &Principal) -> Result<Op> {
        let doc = self.get(id)?;
        let mut op = Op::new(id, name, args, who.actor).seeing(doc.as_ref(), &self.reg);
        op.invoker = who.invoker;
        Ok(op)
    }

    /// Run an operation as the home server: the one way anything changes.
    pub fn execute(&mut self, op: Op, who: &Principal) -> Result<Outcome> {
        let action = op.name.clone();
        let (object, id, intent) = (op.object, op.id, op.intent.clone());
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.run(op, who);
        match result {
            Ok(outcome) => {
                self.conn.execute_batch("COMMIT")?;
                self.after_commit()?;
                if !self.in_reaction {
                    self.run_reactions()?;
                }
                Ok(outcome)
            }
            Err(e) => {
                self.conn.execute_batch("ROLLBACK")?;
                self.reload()?;
                let reason = e.to_string();
                self.audit_row(
                    who,
                    &action,
                    Some(object),
                    Some(id),
                    "refused",
                    &reason,
                    &intent,
                )?;
                Err(e)
            }
        }
    }

    fn after_commit(&mut self) -> Result<()> {
        let dirty: Option<String> = self
            .conn
            .query_row("SELECT value FROM meta WHERE key='vacuum'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if dirty.is_some() {
            self.conn
                .execute("DELETE FROM meta WHERE key='vacuum'", [])?;
            self.conn
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        }
        Ok(())
    }

    fn duplicate(&self, id: OpId) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT seq FROM log WHERE op_id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn type_of(&self, op: &Op, doc: Option<&Object>) -> Result<String> {
        if let Some(d) = doc {
            return Ok(d.type_name.clone());
        }
        match op.name.as_str() {
            "proposal.create" => Ok("locus.proposal".to_string()),
            "object.create" => op
                .args
                .get("type")
                .and_then(Value::as_text)
                .map(str::to_string)
                .ok_or_else(|| StoreError::Invalid("object.create needs a type".to_string())),
            _ => Err(StoreError::Plan(PlanError::NotFound)),
        }
    }

    fn run(&mut self, op: Op, who: &Principal) -> Result<Outcome> {
        if let Some(seq) = self.duplicate(op.id)? {
            return Ok(Outcome::Duplicate { seq });
        }
        if op.actor != who.actor || op.invoker != who.invoker {
            return Err(StoreError::Refused(
                "the operation does not match the authenticated actor".to_string(),
            ));
        }
        let doc = self.get(op.object)?;
        let ty = self.type_of(&op, doc.as_ref())?;
        let home = match &doc {
            Some(d) => d.home.0.clone(),
            None => op
                .args
                .get("home")
                .and_then(Value::as_text)
                .unwrap_or_default()
                .to_string(),
        };
        if home != self.home.0 {
            return Err(StoreError::Refused(format!(
                "{} is not the home of this object ({home})",
                self.home
            )));
        }
        if op.name == "proposal.create" {
            self.check_proposal(&op, who)?;
            return self.commit_with_audit(op, doc, who, "applied");
        }
        let req = Request {
            action: &op.name,
            type_name: &ty,
            object: doc.as_ref().map(|d| d.id),
            implementation: op.implementation.as_ref(),
        };
        match self.policy.decide(who, &req, &self.reg) {
            Decision::Refuse(why) => Err(StoreError::Refused(why)),
            Decision::Propose => self.branch(op, who),
            Decision::Apply if op.name == "proposal.approve" => self.approve(op, doc, who),
            Decision::Apply => self.commit_with_audit(op, doc, who, "applied"),
        }
    }

    fn check_proposal(&self, op: &Op, who: &Principal) -> Result<()> {
        let list = op
            .args
            .get("ops")
            .cloned()
            .unwrap_or(Value::List(Vec::new()));
        let probe = Object {
            id: op.object,
            type_name: "locus.proposal".to_string(),
            home: self.home.clone(),
            rev: 0,
            deleted: false,
            purged: false,
            fields: locus_model::fields([("ops", list)]),
            body: None,
            conflicts: Vec::new(),
            created_at: 0,
            updated_at: 0,
        };
        let inner = proposal_ops(&probe).map_err(StoreError::Invalid)?;
        for i in &inner {
            if i.actor != who.actor || i.invoker != who.invoker {
                return Err(StoreError::Refused(
                    "a proposal holds only its author's operations".to_string(),
                ));
            }
            self.check_inner(i, who, false)?;
        }
        Ok(())
    }

    /// The check on one proposed op, for the author (`Propose` is enough) or
    /// the approver (`Apply` is needed).
    fn check_inner(&self, inner: &Op, who: &Principal, apply_needed: bool) -> Result<()> {
        let doc = self.get(inner.object)?;
        let ty = self.type_of(inner, doc.as_ref())?;
        let home = doc.as_ref().map(|d| d.home.0.clone()).or_else(|| {
            inner
                .args
                .get("home")
                .and_then(Value::as_text)
                .map(str::to_string)
        });
        if home.as_deref() != Some(self.home.0.as_str()) {
            return Err(StoreError::Invalid(format!(
                "operation on {} belongs to another home",
                inner.object
            )));
        }
        let req = Request {
            action: &inner.name,
            type_name: &ty,
            object: doc.as_ref().map(|d| d.id),
            implementation: inner.implementation.as_ref(),
        };
        match self.policy.decide(who, &req, &self.reg) {
            Decision::Refuse(why) => Err(StoreError::Refused(format!("{}: {why}", inner.name))),
            Decision::Propose if apply_needed => Err(StoreError::Refused(format!(
                "{} cannot apply {}",
                who.actor, inner.name
            ))),
            _ => Ok(()),
        }
    }

    /// Turn an operation the actor may only propose into a proposal.
    fn branch(&mut self, op: Op, who: &Principal) -> Result<Outcome> {
        let title = if op.intent.is_empty() {
            op.name.clone()
        } else {
            op.intent.clone()
        };
        let mut wrap = Op::new(
            Id::new(),
            "proposal.create",
            locus_model::fields([
                ("title", title.into()),
                ("home", self.home.0.as_str().into()),
                ("ops", encode_ops(std::slice::from_ref(&op))),
            ]),
            op.actor,
        );
        wrap.id = op.id;
        wrap.invoker = op.invoker;
        wrap.intent = op.intent.clone();
        wrap.at = op.at;
        let proposal = wrap.object;
        match self.commit_with_audit(wrap, None, who, "proposed")? {
            Outcome::Applied { seq, .. } => Ok(Outcome::Proposed { proposal, seq }),
            other => Ok(other),
        }
    }

    fn approve(&mut self, op: Op, doc: Option<Object>, who: &Principal) -> Result<Outcome> {
        let d = doc.as_ref().ok_or(StoreError::Plan(PlanError::NotFound))?;
        let ops = proposal_ops(d).map_err(StoreError::Invalid)?;
        let only: Option<BTreeSet<usize>> = match op.args.get("only") {
            Some(Value::List(l)) => Some(
                l.iter()
                    .filter_map(|v| match v {
                        Value::Int(n) => Some(*n as usize),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        };
        let chosen: Vec<Op> = ops
            .into_iter()
            .enumerate()
            .filter(|(i, _)| only.as_ref().is_none_or(|s| s.contains(i)))
            .map(|(_, o)| o)
            .collect();
        for inner in &chosen {
            self.check_inner(inner, who, true)?;
            // The author's grants may have changed since it proposed this.
            let author = Principal {
                actor: inner.actor,
                invoker: inner.invoker,
            };
            self.check_inner(inner, &author, false)?;
        }
        let proposal = d.id;
        let approver = who.actor;
        let outcome = self.commit_with_audit(op, doc, who, "applied")?;
        for mut inner in chosen {
            inner.id = Id::new();
            inner.proposal = Some(proposal);
            inner.approved_by = Some(approver);
            let author = Principal {
                actor: inner.actor,
                invoker: inner.invoker,
            };
            let target = self.get(inner.object)?;
            self.commit_with_audit(inner, target, &author, "applied")?;
        }
        Ok(outcome)
    }

    fn commit_with_audit(
        &mut self,
        op: Op,
        doc: Option<Object>,
        who: &Principal,
        decision: &str,
    ) -> Result<Outcome> {
        let effects = plan(doc.as_ref(), &op, &self.reg, self.body_check)?;
        if op.name == "object.create"
            && op.args.get("type").and_then(Value::as_text) == Some("locus.type")
        {
            self.check_type_def(&op)?;
        }
        let conflicts = effects
            .iter()
            .filter(|e| matches!(e, Effect::ConflictOpened(_)))
            .count();
        let detail = if conflicts > 0 {
            format!("kept both versions of {conflicts} change(s)")
        } else if op.proposal.is_some() {
            format!(
                "from proposal, approved by {}",
                op.approved_by.unwrap_or(Id::SYSTEM)
            )
        } else {
            String::new()
        };
        let seq = self.write_entry(&op, doc, &effects, None)?;
        self.audit_row(
            who,
            &op.name,
            Some(op.object),
            Some(op.id),
            decision,
            &detail,
            &op.intent,
        )?;
        for e in &effects {
            if let Effect::Purged(scope) = e {
                self.scrub(op.object, scope, op.id)?;
            }
        }
        Ok(Outcome::Applied { seq, effects })
    }

    fn check_type_def(&self, op: &Op) -> Result<()> {
        let Some(Value::Map(f)) = op.args.get("fields") else {
            return Err(StoreError::Invalid("a type needs fields".to_string()));
        };
        let def = f.get("def").and_then(Value::as_text).unwrap_or("{}");
        let def: TypeDef = serde_json::from_str(def)?;
        self.reg.clone().define(def).map_err(StoreError::Invalid)
    }

    /// Write one entry and its results. `at_home` carries a home's ordering
    /// when this store is a replica.
    fn write_entry(
        &mut self,
        op: &Op,
        doc: Option<Object>,
        effects: &[Effect],
        at_home: Option<(&ServerId, i64)>,
    ) -> Result<i64> {
        let mut doc = doc;
        apply(&mut doc, effects, op.object, op.at);
        let rev = doc.as_ref().map_or(0, |d| d.rev);
        let doc_home = doc
            .as_ref()
            .map_or_else(|| self.home.clone(), |d| d.home.clone());
        let (home, given) = match at_home {
            Some((h, s)) => (h.clone(), Some(s)),
            None => (doc_home, None),
        };
        self.conn.execute(
            "INSERT INTO log (op_id, object, home, home_seq, rev, batch, op, effects)
             VALUES (?1, ?2, ?3,
                     COALESCE(?4, (SELECT COALESCE(MAX(seq), 0) + 1 FROM log)),
                     ?5, ?6, ?7, ?8)",
            params![
                op.id.to_string(),
                op.object.to_string(),
                home.0,
                given,
                rev,
                op.batch.map(|b| b.to_string()),
                serde_json::to_string(op)?,
                serde_json::to_string(effects)?,
            ],
        )?;
        let seq = self.conn.last_insert_rowid();
        if let Some(d) = &doc {
            self.save(d)?;
        }
        if let Some(d) = &doc
            && matches!(
                d.type_name.as_str(),
                "locus.member" | "locus.agent" | "locus.grant" | "locus.type"
            )
        {
            self.bump_epoch()?;
            self.reload()?;
        }
        Ok(seq)
    }

    fn bump_epoch(&mut self) -> Result<()> {
        self.epoch += 1;
        self.conn.execute(
            "UPDATE meta SET value=?1 WHERE key='epoch'",
            [self.epoch.to_string()],
        )?;
        Ok(())
    }

    /// Save an object's state and its derived index rows.
    pub fn save(&self, d: &Object) -> Result<()> {
        let id = d.id.to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO objects (id, type, home, rev, deleted, doc)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                d.type_name,
                d.home.0,
                d.rev,
                d.deleted,
                serde_json::to_string(d)?
            ],
        )?;
        self.index(d)
    }

    fn index(&self, d: &Object) -> Result<()> {
        let id = d.id.to_string();
        self.conn.execute("DELETE FROM refs WHERE src=?1", [&id])?;
        self.conn
            .execute("DELETE FROM blob_refs WHERE object=?1", [&id])?;
        let mut refs = Vec::new();
        let mut blobs = Vec::new();
        for v in d.fields.values() {
            v.references(&mut refs);
            v.blobs(&mut blobs);
        }
        if let Some(b) = &d.body {
            refs.extend(locus_model::body_refs(b).into_iter().map(Reference::to));
            blobs.extend(locus_model::body_blobs(b));
        }
        for r in refs {
            self.conn.execute(
                "INSERT INTO refs (src, dst, pin) VALUES (?1, ?2, ?3)",
                params![id, r.id.to_string(), r.pin],
            )?;
        }
        for b in blobs {
            self.conn.execute(
                "INSERT INTO blob_refs (hash, object) VALUES (?1, ?2)",
                params![b.to_string(), id],
            )?;
        }
        Ok(())
    }

    /// Take in an entry a home server has decided. Used by replicas.
    pub fn apply_entry(&mut self, e: &LogEntry) -> Result<()> {
        if self.duplicate(e.op.id)?.is_some() {
            return Ok(());
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let doc = self.get(e.op.object)?;
            self.write_entry(&e.op, doc, &e.effects, Some((&e.home, e.home_seq)))?;
            for x in &e.effects {
                if let Effect::Purged(scope) = x {
                    self.scrub(e.op.object, scope, e.op.id)?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT")?;
                self.after_commit()
            }
            Err(e) => {
                self.conn.execute_batch("ROLLBACK")?;
                Err(e)
            }
        }
    }

    /// Remove an object from this replica, because it may no longer be read.
    pub fn evict(&mut self, id: ObjectId) -> Result<()> {
        let s = id.to_string();
        self.conn.execute("DELETE FROM objects WHERE id=?1", [&s])?;
        self.conn.execute("DELETE FROM refs WHERE src=?1", [&s])?;
        self.conn
            .execute("DELETE FROM blob_refs WHERE object=?1", [&s])?;
        Ok(())
    }

    // Recovery.

    /// Undo one log entry with new operations. The log only grows: the undone
    /// entry stays, and the undo can itself be undone.
    pub fn revert(&mut self, op_id: OpId, who: &Principal, intent: &str) -> Result<Vec<Outcome>> {
        let entry = self.entry(op_id)?;
        let steps = inverse(&entry.op, &entry.effects).map_err(StoreError::Invalid)?;
        let mut out = Vec::new();
        for (name, args) in steps {
            let op = self
                .op_on(entry.op.object, &name, args, who)?
                .intent(intent);
            out.push(self.execute(op, who)?);
        }
        Ok(out)
    }

    /// Undo a whole batch, such as an import, newest first.
    pub fn revert_batch(&mut self, batch: Id, who: &Principal, intent: &str) -> Result<usize> {
        let entries = self.batch_entries(batch)?;
        for e in entries.iter().rev() {
            self.revert(e.op.id, who, intent)?;
        }
        Ok(entries.len())
    }

    /// Scrub a purge through the log. Everything but the purge's own record
    /// and the bare fact of each entry goes.
    fn scrub(&mut self, object: ObjectId, scope: &Purge, purge_op: OpId) -> Result<()> {
        self.conn.execute("UPDATE purge_guard SET active=1", [])?;
        let result = self.scrub_guarded(object, scope, purge_op);
        self.conn.execute("UPDATE purge_guard SET active=0", [])?;
        result?;
        self.conn
            .execute("INSERT OR REPLACE INTO meta VALUES ('vacuum', '1')", [])?;
        Ok(())
    }

    fn scrub_guarded(&mut self, object: ObjectId, scope: &Purge, purge_op: OpId) -> Result<()> {
        let entries = match scope {
            Purge::Blob(_) => self.entries(
                "SELECT seq, op, effects, rev, home, home_seq FROM log ORDER BY seq",
                [],
            )?,
            _ => self.history(object)?,
        };
        let mut secrets = Vec::new();
        if let Purge::Field(f) = scope {
            for e in &entries {
                secrets.extend(op::field_values(f, &e.effects));
            }
        }
        for e in entries {
            if e.op.id == purge_op {
                continue;
            }
            let (mut o, mut fx) = (e.op, e.effects);
            let changed = op::scrub_entry(&mut o, &mut fx, scope, &secrets);
            if changed {
                self.conn.execute(
                    "UPDATE log SET op=?1, effects=?2 WHERE seq=?3",
                    params![
                        serde_json::to_string(&o)?,
                        serde_json::to_string(&fx)?,
                        e.seq
                    ],
                )?;
            }
        }
        if !matches!(scope, Purge::Blob(_)) {
            self.conn.execute(
                "UPDATE audit SET intent='[purged]' WHERE object=?1",
                [object.to_string()],
            )?;
        }
        self.scrub_proposals(object, scope, &secrets)?;
        if let Purge::Blob(h) = scope {
            self.purge_blob(h)?;
            for mut d in self.objects_holding(h)? {
                d.fields.values_mut().for_each(|v| {
                    v.scrub_blob(h);
                });
                self.save(&d)?;
            }
        }
        Ok(())
    }

    fn objects_holding(&self, h: &locus_model::BlobHash) -> Result<Vec<Object>> {
        let mut s = self
            .conn
            .prepare("SELECT object FROM blob_refs WHERE hash=?1")?;
        let ids = s.query_map([h.to_string()], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for id in ids {
            let id: ObjectId = id?
                .parse()
                .map_err(|_| StoreError::Invalid("bad id".to_string()))?;
            out.extend(self.get(id)?);
        }
        Ok(out)
    }

    /// Proposals hold copies of operations, so a purge has to reach them too.
    fn scrub_proposals(
        &mut self,
        object: ObjectId,
        scope: &Purge,
        secrets: &[Value],
    ) -> Result<()> {
        for mut p in self.objects_of_type("locus.proposal")? {
            let Ok(mut ops) = proposal_ops(&p) else {
                continue;
            };
            let mut touched = false;
            for o in &mut ops {
                if o.object == object || matches!(scope, Purge::Blob(_)) {
                    touched |= op::scrub_entry(o, &mut Vec::new(), scope, secrets);
                }
            }
            if !touched {
                continue;
            }
            let encoded = encode_ops(&ops);
            p.fields.insert("ops".to_string(), encoded.clone());
            self.save(&p)?;
            for e in self.history(p.id)? {
                let (mut o, mut fx) = (e.op, e.effects);
                if o.args.contains_key("ops") {
                    o.args.insert("ops".to_string(), encoded.clone());
                }
                for x in &mut fx {
                    if let Effect::Created { fields, .. } = x
                        && fields.contains_key("ops")
                    {
                        fields.insert("ops".to_string(), encoded.clone());
                    }
                }
                self.conn.execute(
                    "UPDATE log SET op=?1, effects=?2 WHERE seq=?3",
                    params![
                        serde_json::to_string(&o)?,
                        serde_json::to_string(&fx)?,
                        e.seq
                    ],
                )?;
            }
        }
        Ok(())
    }

    // Proposals.

    /// A proposal from `who`, holding operations it has built. Nothing applies
    /// until a person approves it.
    pub fn propose(&mut self, title: &str, ops: &[Op], who: &Principal) -> Result<ObjectId> {
        let mut op = Op::new(
            Id::new(),
            "proposal.create",
            locus_model::fields([
                ("title", title.into()),
                ("home", self.home.0.as_str().into()),
                ("ops", encode_ops(ops)),
            ]),
            who.actor,
        );
        op.invoker = who.invoker;
        op.intent = title.to_string();
        let id = op.object;
        self.execute(op, who)?;
        Ok(id)
    }

    /// The proposal as a diff against current state. Nothing changes.
    pub fn review(&self, id: ObjectId, who: &Principal) -> Result<Vec<DiffEntry>> {
        let doc = self.get_as(who, id)?;
        let ops = proposal_ops(&doc).map_err(StoreError::Invalid)?;
        Ok(diff(
            &ops,
            |oid| self.get(oid).ok().flatten(),
            &self.reg,
            self.body_check,
        ))
    }

    /// Open proposals `who` can see.
    pub fn proposals(&self, who: &Principal) -> Result<Vec<Object>> {
        self.view(
            who,
            &ViewSpec::of(Filter::And(vec![
                Filter::Type("locus.proposal".to_string()),
                Filter::Eq("status".to_string(), "open".into()),
            ])),
        )
    }

    /// Rebuild every object and index from the log, proving that the log is
    /// the source of truth. Returns how many objects differed.
    pub fn rebuild(&mut self) -> Result<usize> {
        let before = self.all_objects()?;
        self.conn.execute_batch(
            "BEGIN IMMEDIATE; DELETE FROM objects; DELETE FROM refs; DELETE FROM blob_refs;",
        )?;
        let result: Result<usize> = (|| {
            let mut after = std::collections::BTreeMap::<ObjectId, Option<Object>>::new();
            let mut seq = 0;
            loop {
                let batch = self.log_since(seq, 1000)?;
                let Some(last) = batch.last() else { break };
                seq = last.seq;
                for e in &batch {
                    let doc = after.entry(e.op.object).or_default();
                    apply(doc, &e.effects, e.op.object, e.op.at);
                }
            }
            let mut differ = 0;
            for doc in after.values().flatten() {
                self.save(doc)?;
                if !before.iter().any(|b| b == doc) {
                    differ += 1;
                }
            }
            Ok(differ)
        })();
        match result {
            Ok(n) => {
                self.conn.execute_batch("COMMIT")?;
                self.reload()?;
                Ok(n)
            }
            Err(e) => {
                self.conn.execute_batch("ROLLBACK")?;
                Err(e)
            }
        }
    }
}
