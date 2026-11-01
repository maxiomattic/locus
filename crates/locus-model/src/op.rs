//! Operations: the only way anything changes.
//!
//! [`plan`] and [`apply`] are pure. Planning takes an object and an operation
//! and works out the effects, including every merge decision. Applying writes
//! those effects into the object. The home server's plan is the one that
//! counts, and a client runs the same code to predict it, so a client's view
//! of its unconfirmed changes matches what the server will do with them.

use crate::id::{BlobHash, Id, ObjectId, OpId, ServerId};
use crate::types::{Merge, Registry, TypeDef};
use crate::value::{Fields, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `seen` key for the body.
pub const SEEN_BODY: &str = "$body";

/// Who may invoke an operation, beyond what grants say.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKindRequired {
    #[default]
    Any,
    Human,
}

/// Which implementation carried out an operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplUse {
    pub model: String,
    pub effort: String,
}

/// One intended change to one object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    pub object: ObjectId,
    pub name: String,
    pub args: Fields,
    /// What the author saw of each field it is changing. This is how a
    /// concurrent change is told from a plain one.
    pub seen: BTreeMap<String, Option<Value>>,
    pub actor: ObjectId,
    /// The person who invoked the agent, if an agent is acting for one.
    pub invoker: Option<ObjectId>,
    /// What the actor meant to do, in words.
    pub intent: String,
    pub at: i64,
    /// Set when an agent or model carried this out.
    pub implementation: Option<ImplUse>,
    /// Ops created together, such as one import, can be undone together.
    pub batch: Option<Id>,
    /// The proposal this came from.
    pub proposal: Option<ObjectId>,
    /// Who approved that proposal.
    pub approved_by: Option<ObjectId>,
}

impl Op {
    pub fn new(object: ObjectId, name: &str, args: Fields, actor: ObjectId) -> Self {
        Op {
            id: Id::new(),
            object,
            name: name.to_string(),
            args,
            seen: BTreeMap::new(),
            actor,
            invoker: None,
            intent: String::new(),
            at: crate::now_ms(),
            implementation: None,
            batch: None,
            proposal: None,
            approved_by: None,
        }
    }

    pub fn intent(mut self, intent: &str) -> Self {
        self.intent = intent.to_string();
        self
    }

    pub fn invoked_by(mut self, member: ObjectId) -> Self {
        self.invoker = Some(member);
        self
    }

    /// Record what the author can see of `doc`, for the fields it changes.
    pub fn seeing(mut self, doc: Option<&Object>, reg: &Registry) -> Self {
        self.seen = seen_for(&self, doc, reg);
        self
    }
}

fn seen_for(op: &Op, doc: Option<&Object>, reg: &Registry) -> BTreeMap<String, Option<Value>> {
    let Some(doc) = doc else {
        return BTreeMap::new();
    };
    let mut seen = BTreeMap::new();
    let mut note = |f: &str| {
        seen.insert(f.to_string(), doc.fields.get(f).cloned());
    };
    match op.name.as_str() {
        "object.set" | "object.revert" => {
            if let Some(Value::Map(m)) = op.args.get("fields") {
                m.keys().for_each(|k| note(k));
            }
        }
        "object.delete" => {
            doc.fields.keys().for_each(|k| note(k));
        }
        _ => {}
    }
    if let Some((_, def)) = reg.op(&op.name) {
        def.sets.keys().for_each(|k| note(k));
    }
    if matches!(op.name.as_str(), "body.set" | "object.delete") {
        seen.insert(
            SEEN_BODY.to_string(),
            doc.body.as_ref().map(|b| Value::text(b.clone())),
        );
    }
    seen
}

/// A value that lost a race and is kept until someone decides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    /// The field concerned, or None if the conflict is a delete against a
    /// concurrent edit. [`SEEN_BODY`] stands for the body.
    pub field: Option<String>,
    /// The version that did not become the current value.
    pub value: Option<Value>,
    pub op: OpId,
    pub actor: ObjectId,
    pub at: i64,
}

/// An object's state, derived entirely from its operations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    pub type_name: String,
    pub home: ServerId,
    pub rev: u64,
    pub deleted: bool,
    /// Purged: nothing is left but the ID.
    pub purged: bool,
    pub fields: Fields,
    /// Sanitized HTML.
    pub body: Option<String>,
    pub conflicts: Vec<Conflict>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Object {
    pub fn title(&self) -> Option<&str> {
        self.fields.get("title").and_then(Value::as_text)
    }
}

/// What a purge removes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purge {
    Object,
    Field(String),
    Blob(BlobHash),
}

/// A resulting change, with what it replaced. This is what lets deletes and
/// overwrites be recovered from the log alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Created {
        type_name: String,
        home: ServerId,
        fields: Fields,
        body: Option<String>,
    },
    Set {
        field: String,
        old: Option<Value>,
        new: Option<Value>,
    },
    Added {
        field: String,
        value: Value,
    },
    Removed {
        field: String,
        value: Value,
    },
    BodySet {
        old: Option<String>,
        new: Option<String>,
    },
    Deleted,
    Restored,
    ConflictOpened(Conflict),
    /// A later edit from the same author replaces its own losing version.
    ConflictAmended {
        id: String,
        old: Option<Value>,
        new: Option<Value>,
    },
    ConflictClosed(Conflict),
    Purged(Purge),
    /// The whole object as it stands. Clients start from these, because they
    /// keep a partial copy and need not hold its history.
    Snapshot(Box<Object>),
    /// What a purge left in place of an effect.
    Redacted,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("no such object")]
    NotFound,
    #[error("the object already exists")]
    Exists,
    #[error("the object was purged")]
    Purged,
    #[error("unknown type {0}")]
    UnknownType(String),
    #[error("unknown operation {0}")]
    UnknownOp(String),
    #[error("{0}")]
    Invalid(String),
}

fn invalid(s: impl Into<String>) -> PlanError {
    PlanError::Invalid(s.into())
}

/// Checks a body before it is stored, returning the form to keep.
pub type BodyCheck = fn(&str) -> Result<String, String>;

fn arg<'a>(op: &'a Op, name: &str) -> Result<&'a Value, PlanError> {
    op.args
        .get(name)
        .ok_or_else(|| invalid(format!("{} needs {name}", op.name)))
}

fn text_arg<'a>(op: &'a Op, name: &str) -> Result<&'a str, PlanError> {
    arg(op, name)?
        .as_text()
        .ok_or_else(|| invalid(format!("{name} must be text")))
}

fn check_field(ty: &TypeDef, name: &str, v: &Value) -> Result<(), PlanError> {
    if let Some(f) = ty.fields.get(name) {
        if !f.kind.accepts(v) {
            return Err(invalid(format!(
                "field {name} of {} must be {:?}",
                ty.name, f.kind
            )));
        }
        Ok(())
    } else if ty.open {
        Ok(())
    } else {
        Err(invalid(format!("type {} has no field {name}", ty.name)))
    }
}

/// The effects of changing fields, each compared with what the author saw.
/// `None` unsets a field.
fn set_effects(
    doc: &Object,
    ty: &TypeDef,
    changes: Vec<(String, Option<Value>)>,
    op: &Op,
) -> Result<Vec<Effect>, PlanError> {
    let mut out = Vec::new();
    for (name, new) in changes {
        if let Some(v) = &new {
            check_field(ty, &name, v)?;
        }
        let cur = doc.fields.get(&name);
        let seen = op.seen.get(&name).cloned().flatten();
        if ty.fields.get(&name).is_some_and(|f| f.merge == Merge::Set) {
            let as_list = |v: &Option<Value>| match v {
                Some(Value::List(l)) => l.clone(),
                _ => Vec::new(),
            };
            let (was, now) = (as_list(&seen), as_list(&new));
            let present = as_list(&cur.cloned());
            for v in now.iter().filter(|v| !was.contains(v)) {
                if !present.contains(v) {
                    out.push(Effect::Added {
                        field: name.clone(),
                        value: v.clone(),
                    });
                }
            }
            for v in was.iter().filter(|v| !now.contains(v)) {
                if present.contains(v) {
                    out.push(Effect::Removed {
                        field: name.clone(),
                        value: v.clone(),
                    });
                }
            }
            continue;
        }
        if cur == new.as_ref() {
            continue;
        }
        if doc.deleted {
            out.push(Effect::ConflictOpened(conflict(op, Some(&name), new)));
        } else if cur == seen.as_ref() {
            out.push(Effect::Set {
                field: name,
                old: cur.cloned(),
                new,
            });
        } else if let Some(c) = doc
            .conflicts
            .iter()
            .find(|c| c.actor == op.actor && c.field.as_deref() == Some(&name) && c.value == seen)
        {
            out.push(Effect::ConflictAmended {
                id: c.id.clone(),
                old: c.value.clone(),
                new,
            });
        } else {
            out.push(Effect::ConflictOpened(conflict(op, Some(&name), new)));
        }
    }
    Ok(out)
}

fn conflict(op: &Op, field: Option<&str>, value: Option<Value>) -> Conflict {
    Conflict {
        id: format!("{}:{}", op.id, field.unwrap_or("$delete")),
        field: field.map(str::to_string),
        value,
        op: op.id,
        actor: op.actor,
        at: op.at,
    }
}

fn typedef<'a>(reg: &'a Registry, name: &str) -> Result<&'a TypeDef, PlanError> {
    reg.get(name)
        .ok_or_else(|| PlanError::UnknownType(name.to_string()))
}

fn plan_create(op: &Op, reg: &Registry, check: BodyCheck) -> Result<Vec<Effect>, PlanError> {
    let (type_name, mut fields) = if op.name == "proposal.create" {
        let mut f = Fields::new();
        f.insert("title".into(), arg(op, "title")?.clone());
        f.insert("ops".into(), arg(op, "ops")?.clone());
        f.insert("status".into(), Value::text("open"));
        ("locus.proposal".to_string(), f)
    } else {
        let f = match op.args.get("fields") {
            Some(Value::Map(m)) => m.clone(),
            None => Fields::new(),
            Some(_) => return Err(invalid("fields must be a map")),
        };
        (text_arg(op, "type")?.to_string(), f)
    };
    let ty = typedef(reg, &type_name)?;
    if ty.sealed && op.name == "object.create" {
        return Err(invalid(format!(
            "{type_name} is created by its own operation"
        )));
    }
    for (name, v) in &fields {
        check_field(ty, name, v)?;
    }
    for (name, f) in &ty.fields {
        if f.required && !fields.contains_key(name) {
            return Err(invalid(format!("{type_name} needs field {name}")));
        }
        if f.merge == Merge::Set && !fields.contains_key(name) {
            fields.insert(name.clone(), Value::List(Vec::new()));
        }
    }
    let body = match op.args.get("body") {
        Some(Value::Text(h)) if ty.body => Some(check(h).map_err(PlanError::Invalid)?),
        Some(_) => return Err(invalid(format!("{type_name} takes no body"))),
        None => None,
    };
    Ok(vec![Effect::Created {
        type_name,
        home: ServerId(text_arg(op, "home")?.to_string()),
        fields,
        body,
    }])
}

fn plan_resolve(doc: &Object, op: &Op) -> Result<Vec<Effect>, PlanError> {
    let id = text_arg(op, "conflict")?;
    let c = doc
        .conflicts
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| invalid("no such conflict"))?;
    let take = arg(op, "take")?
        .as_bool()
        .ok_or_else(|| invalid("take must be true or false"))?;
    let mut out = Vec::new();
    if take && doc.deleted {
        out.push(Effect::Restored);
    }
    if take {
        match c.field.as_deref() {
            Some(SEEN_BODY) => out.push(Effect::BodySet {
                old: doc.body.clone(),
                new: c
                    .value
                    .as_ref()
                    .and_then(Value::as_text)
                    .map(str::to_string),
            }),
            Some(f) => out.push(Effect::Set {
                field: f.to_string(),
                old: doc.fields.get(f).cloned(),
                new: c.value.clone(),
            }),
            None => {}
        }
    }
    out.push(Effect::ConflictClosed(c.clone()));
    Ok(out)
}

fn plan_proposal(doc: &Object, op: &Op) -> Result<Vec<Effect>, PlanError> {
    if doc.type_name != "locus.proposal" {
        return Err(invalid("not a proposal"));
    }
    if doc.fields.get("status").and_then(Value::as_text) != Some("open") {
        return Err(invalid("the proposal was already decided"));
    }
    let set = |field: &str, new: Value| Effect::Set {
        field: field.to_string(),
        old: doc.fields.get(field).cloned(),
        new: Some(new),
    };
    let status = if op.name == "proposal.approve" {
        "approved"
    } else {
        "rejected"
    };
    let mut out = vec![
        set("status", Value::text(status)),
        set("decided_by", op.actor.into()),
    ];
    if let Some(r) = op.args.get("reason") {
        out.push(set("reason", r.clone()));
    }
    if let Some(a) = op.args.get("only") {
        out.push(set("applied", a.clone()));
    }
    Ok(out)
}

/// Work out what `op` does to `doc`. Pure, and the same on server and client.
pub fn plan(
    doc: Option<&Object>,
    op: &Op,
    reg: &Registry,
    check: BodyCheck,
) -> Result<Vec<Effect>, PlanError> {
    if matches!(op.name.as_str(), "object.create" | "proposal.create") {
        if doc.is_some() {
            return Err(PlanError::Exists);
        }
        return plan_create(op, reg, check);
    }
    let doc = doc.ok_or(PlanError::NotFound)?;
    if doc.purged {
        return Err(PlanError::Purged);
    }
    let ty = typedef(reg, &doc.type_name)?;
    if ty.sealed && op.name.starts_with("object.") && op.name != "object.delete" {
        return Err(invalid(format!(
            "{} only changes through its own operations",
            doc.type_name
        )));
    }
    match op.name.as_str() {
        "object.set" | "object.revert" => {
            let mut changes = Vec::new();
            if let Some(Value::Map(m)) = op.args.get("fields") {
                changes.extend(m.iter().map(|(k, v)| (k.clone(), Some(v.clone()))));
            }
            if let Some(Value::List(l)) = op.args.get("unset") {
                for v in l {
                    let f = v.as_text().ok_or_else(|| invalid("unset takes names"))?;
                    if ty.fields.get(f).is_some_and(|d| d.required) {
                        return Err(invalid(format!("field {f} is required")));
                    }
                    changes.push((f.to_string(), None));
                }
            }
            set_effects(doc, ty, changes, op)
        }
        "object.add" | "object.remove" => {
            let field = text_arg(op, "field")?;
            let def = ty.fields.get(field);
            if def.is_none_or(|d| d.merge != Merge::Set) {
                return Err(invalid(format!("{field} is not a set")));
            }
            let value = arg(op, "value")?.clone();
            let has = matches!(doc.fields.get(field), Some(Value::List(l)) if l.contains(&value));
            let adding = op.name == "object.add";
            Ok(match (adding, has) {
                (true, false) => vec![Effect::Added {
                    field: field.to_string(),
                    value,
                }],
                (false, true) => vec![Effect::Removed {
                    field: field.to_string(),
                    value,
                }],
                _ => Vec::new(),
            })
        }
        "body.set" => {
            if !ty.body {
                return Err(invalid(format!("{} has no body", doc.type_name)));
            }
            let new = check(text_arg(op, "html")?).map_err(PlanError::Invalid)?;
            if doc.body.as_deref() == Some(&new) {
                return Ok(Vec::new());
            }
            let seen = op.seen.get(SEEN_BODY).cloned().flatten();
            let cur = doc.body.as_ref().map(|b| Value::text(b.clone()));
            let new_v = Some(Value::text(new.clone()));
            if doc.deleted || cur != seen {
                if let Some(c) = doc.conflicts.iter().find(|c| {
                    c.actor == op.actor && c.field.as_deref() == Some(SEEN_BODY) && c.value == seen
                }) {
                    return Ok(vec![Effect::ConflictAmended {
                        id: c.id.clone(),
                        old: c.value.clone(),
                        new: new_v,
                    }]);
                }
                return Ok(vec![Effect::ConflictOpened(conflict(
                    op,
                    Some(SEEN_BODY),
                    new_v,
                ))]);
            }
            Ok(vec![Effect::BodySet {
                old: doc.body.clone(),
                new: Some(new),
            }])
        }
        "object.delete" => {
            if doc.deleted {
                return Ok(Vec::new());
            }
            let body = doc.body.as_ref().map(|b| Value::text(b.clone()));
            let changed = !op.seen.is_empty()
                && (doc
                    .fields
                    .iter()
                    .any(|(k, v)| op.seen.get(k).cloned().flatten().as_ref() != Some(v))
                    || op.seen.get(SEEN_BODY).cloned().flatten() != body);
            let mut out = vec![Effect::Deleted];
            if changed {
                out.push(Effect::ConflictOpened(conflict(op, None, None)));
            }
            Ok(out)
        }
        "object.restore" => Ok(if doc.deleted {
            vec![Effect::Restored]
        } else {
            Vec::new()
        }),
        "object.resolve" => plan_resolve(doc, op),
        "object.purge" => {
            let scope = match text_arg(op, "scope")? {
                "object" => Purge::Object,
                "field" => Purge::Field(text_arg(op, "field")?.to_string()),
                "blob" => match arg(op, "blob")? {
                    Value::Blob(h) => Purge::Blob(*h),
                    _ => return Err(invalid("blob must be a blob")),
                },
                other => return Err(invalid(format!("unknown purge scope {other}"))),
            };
            Ok(vec![Effect::Purged(scope)])
        }
        "proposal.approve" | "proposal.reject" => plan_proposal(doc, op),
        name => {
            let Some((owner, def)) = reg.op(name) else {
                return Err(PlanError::UnknownOp(name.to_string()));
            };
            if owner.name != doc.type_name {
                return Err(invalid(format!(
                    "{name} does not apply to {}",
                    doc.type_name
                )));
            }
            for (param, kind) in &def.params {
                if let Some(v) = op.args.get(param)
                    && !kind.accepts(v)
                {
                    return Err(invalid(format!("{param} must be {kind:?}")));
                }
            }
            for name in op.args.keys() {
                if !def.params.contains_key(name) {
                    return Err(invalid(format!("{} takes no {name}", op.name)));
                }
            }
            let changes = def
                .sets
                .iter()
                .filter_map(|(f, p)| op.args.get(p).map(|v| (f.clone(), Some(v.clone()))))
                .collect();
            set_effects(doc, ty, changes, op)
        }
    }
}

/// Apply effects to an object. Empty effects leave it untouched.
pub fn apply(doc: &mut Option<Object>, effects: &[Effect], op_object: ObjectId, at: i64) {
    let mut changed = false;
    for e in effects {
        changed = true;
        match e {
            Effect::Created {
                type_name,
                home,
                fields,
                body,
            } => {
                *doc = Some(Object {
                    id: op_object,
                    type_name: type_name.clone(),
                    home: home.clone(),
                    rev: 0,
                    deleted: false,
                    purged: false,
                    fields: fields.clone(),
                    body: body.clone(),
                    conflicts: Vec::new(),
                    created_at: at,
                    updated_at: at,
                });
            }
            Effect::Snapshot(o) => {
                *doc = Some((**o).clone());
                return;
            }
            other => {
                if let Some(d) = doc {
                    apply_one(d, other);
                }
            }
        }
    }
    if changed && let Some(d) = doc {
        d.rev += 1;
        d.updated_at = at;
    }
}

fn apply_one(d: &mut Object, e: &Effect) {
    match e {
        Effect::Set { field, new, .. } => {
            match new {
                Some(v) => d.fields.insert(field.clone(), v.clone()),
                None => d.fields.remove(field),
            };
        }
        Effect::Added { field, value } => {
            let slot = d
                .fields
                .entry(field.clone())
                .or_insert_with(|| Value::List(Vec::new()));
            if let Value::List(l) = slot
                && !l.contains(value)
            {
                l.push(value.clone());
            }
        }
        Effect::Removed { field, value } => {
            if let Some(Value::List(l)) = d.fields.get_mut(field) {
                l.retain(|v| v != value);
            }
        }
        Effect::BodySet { new, .. } => d.body = new.clone(),
        Effect::Deleted => d.deleted = true,
        Effect::Restored => d.deleted = false,
        Effect::ConflictOpened(c) => d.conflicts.push(c.clone()),
        Effect::ConflictAmended { id, new, .. } => {
            if let Some(c) = d.conflicts.iter_mut().find(|c| &c.id == id) {
                c.value = new.clone();
            }
        }
        Effect::ConflictClosed(c) => d.conflicts.retain(|x| x.id != c.id),
        Effect::Purged(scope) => purge_object(d, scope),
        Effect::Created { .. } | Effect::Snapshot(_) | Effect::Redacted => {}
    }
}

fn purge_object(d: &mut Object, scope: &Purge) {
    match scope {
        Purge::Object => {
            d.purged = true;
            d.deleted = true;
            d.fields.clear();
            d.body = None;
            d.conflicts.clear();
        }
        Purge::Field(f) if f == SEEN_BODY => {
            d.body = None;
            d.conflicts
                .retain(|c| c.field.as_deref() != Some(SEEN_BODY));
        }
        Purge::Field(f) => {
            d.fields.remove(f);
            d.conflicts.retain(|c| c.field.as_deref() != Some(f));
        }
        Purge::Blob(h) => {
            d.fields.values_mut().for_each(|v| {
                v.scrub_blob(h);
            });
            for c in &mut d.conflicts {
                if let Some(v) = &mut c.value {
                    v.scrub_blob(h);
                }
            }
        }
    }
}

/// Every value an operation carries or left behind.
fn entry_values<'a>(op: &'a mut Op, effects: &'a mut [Effect]) -> Vec<&'a mut Value> {
    let mut out: Vec<&mut Value> = op.args.values_mut().collect();
    out.extend(op.seen.values_mut().flatten());
    for e in effects {
        match e {
            Effect::Created { fields, .. } => out.extend(fields.values_mut()),
            Effect::Set { old, new, .. } | Effect::ConflictAmended { old, new, .. } => {
                out.extend(old.iter_mut());
                out.extend(new.iter_mut());
            }
            Effect::Added { value, .. } | Effect::Removed { value, .. } => out.push(value),
            Effect::ConflictOpened(c) | Effect::ConflictClosed(c) => out.extend(c.value.iter_mut()),
            Effect::Snapshot(o) => {
                out.extend(o.fields.values_mut());
                for c in &mut o.conflicts {
                    out.extend(c.value.iter_mut());
                }
            }
            _ => {}
        }
    }
    out
}

fn replace_equal(v: &mut Value, secrets: &[Value]) {
    if secrets.contains(v) {
        *v = Value::Redacted;
        return;
    }
    match v {
        Value::List(l) => l.iter_mut().for_each(|x| replace_equal(x, secrets)),
        Value::Map(m) => m.values_mut().for_each(|x| replace_equal(x, secrets)),
        _ => {}
    }
}

/// Values a field has held according to this entry.
pub fn field_values(field: &str, effects: &[Effect]) -> Vec<Value> {
    let mut out = Vec::new();
    for e in effects {
        match e {
            Effect::Created { fields, body, .. } => {
                out.extend(fields.get(field).cloned());
                if field == SEEN_BODY {
                    out.extend(body.clone().map(Value::Text));
                }
            }
            Effect::Set { field: f, old, new } if f == field => {
                out.extend(old.clone());
                out.extend(new.clone());
            }
            Effect::Added { field: f, value } | Effect::Removed { field: f, value }
                if f == field =>
            {
                out.push(value.clone());
            }
            Effect::BodySet { old, new } if field == SEEN_BODY => {
                out.extend(old.clone().map(Value::Text));
                out.extend(new.clone().map(Value::Text));
            }
            Effect::ConflictOpened(c) | Effect::ConflictClosed(c)
                if c.field.as_deref() == Some(field) =>
            {
                out.extend(c.value.clone());
            }
            Effect::ConflictAmended { old, new, .. } => {
                out.extend(old.clone());
                out.extend(new.clone());
            }
            Effect::Snapshot(o) => {
                out.extend(o.fields.get(field).cloned());
                if field == SEEN_BODY {
                    out.extend(o.body.clone().map(Value::Text));
                }
            }
            _ => {}
        }
    }
    out
}

/// Remove purged content from one log entry of the purged object, or from any
/// entry for a blob. `secrets` are the values the field has ever held, which
/// also catches them in arguments under other names. Returns true if changed.
pub fn scrub_entry(op: &mut Op, effects: &mut [Effect], scope: &Purge, secrets: &[Value]) -> bool {
    match scope {
        Purge::Object => {
            op.args.clear();
            op.seen.clear();
            op.intent = "[purged]".to_string();
            for e in effects.iter_mut() {
                if !matches!(e, Effect::Purged(_)) {
                    *e = Effect::Redacted;
                }
            }
            true
        }
        Purge::Field(field) => {
            let touched = !field_values(field, effects).is_empty()
                || op.seen.contains_key(field)
                || op.args.contains_key(field);
            if !touched {
                return false;
            }
            for v in entry_values(op, effects) {
                replace_equal(v, secrets);
            }
            op.seen.remove(field);
            op.intent = "[purged]".to_string();
            for e in effects.iter_mut() {
                let hit = match e {
                    Effect::Set { field: f, .. }
                    | Effect::Added { field: f, .. }
                    | Effect::Removed { field: f, .. } => f == field,
                    Effect::BodySet { .. } => field == SEEN_BODY,
                    Effect::ConflictOpened(c) | Effect::ConflictClosed(c) => {
                        c.field.as_deref() == Some(field)
                    }
                    Effect::ConflictAmended { old, .. } => {
                        old.as_ref().is_some_and(|v| *v == Value::Redacted)
                    }
                    _ => false,
                };
                if hit {
                    *e = Effect::Redacted;
                }
                match e {
                    Effect::Created { fields, body, .. } => {
                        if field == SEEN_BODY {
                            *body = None;
                        } else if fields.contains_key(field) {
                            fields.insert(field.clone(), Value::Redacted);
                        }
                    }
                    Effect::Snapshot(o) => purge_object(o, scope),
                    _ => {}
                }
            }
            true
        }
        Purge::Blob(h) => {
            let mut changed = false;
            for v in entry_values(op, effects) {
                changed |= v.scrub_blob(h);
            }
            changed
        }
    }
}
