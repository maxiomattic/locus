//! Types: what fields an object has, which operations it offers and how
//! concurrent edits merge. Types implement shared traits such as `timed`, which
//! is what lets one view show meetings, flights and photos together.

use crate::op::ActorKindRequired;
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Any,
    Bool,
    Int,
    Float,
    Text,
    Time,
    Ref,
    Blob,
    List,
    Map,
}

impl Kind {
    pub fn accepts(self, v: &Value) -> bool {
        matches!(
            (self, v),
            (Kind::Any, _)
                | (_, Value::Redacted)
                | (Kind::Bool, Value::Bool(_))
                | (Kind::Int, Value::Int(_))
                | (Kind::Float, Value::Float(_) | Value::Int(_))
                | (Kind::Text, Value::Text(_))
                | (Kind::Time, Value::Time(_))
                | (Kind::Ref, Value::Ref(_))
                | (Kind::Blob, Value::Blob(_))
                | (Kind::List, Value::List(_))
                | (Kind::Map, Value::Map(_))
        )
    }
}

/// How concurrent edits to a field merge. There is deliberately no
/// last-writer-wins: a field either merges without losing anything, or it
/// keeps both versions as a conflict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Merge {
    /// Two different values keep both, for a person or agent to resolve.
    #[default]
    KeepBoth,
    /// The field is a set: additions and removals commute.
    Set,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub kind: Kind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub merge: Merge,
}

/// How an operation is carried out. The signature stays the same when an
/// operation that began as agent work later becomes code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Implementation {
    #[default]
    Code,
    Agent,
    CodeWithAgentFallback,
}

/// When an operation may apply without review.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// Whatever the actor's grant says.
    #[default]
    Policy,
    /// Agents and reactions can only propose this. Sending a message and
    /// changing the operating system are like this.
    Human,
}

/// An operation a type declares. `sets` maps fields to the argument that fills
/// them, which is all a code implementation needs for field-level operations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OpDef {
    #[serde(default)]
    pub params: BTreeMap<String, Kind>,
    #[serde(default)]
    pub sets: BTreeMap<String, String>,
    #[serde(default)]
    pub implementation: Implementation,
    #[serde(default)]
    pub approval: Approval,
    /// Who may invoke it at all, beyond grants.
    #[serde(default)]
    pub only: ActorKindRequired,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, FieldDef>,
    /// Keyed by the part after the dot: `rename` for `note.rename`.
    #[serde(default)]
    pub ops: BTreeMap<String, OpDef>,
    /// Objects of this type have a text body.
    #[serde(default)]
    pub body: bool,
    /// Fields not listed above are allowed.
    #[serde(default)]
    pub open: bool,
    /// Identity and policy: only an administrator may change these.
    #[serde(default)]
    pub protected: bool,
    /// Generic `object.*` writes are refused; only declared operations apply.
    #[serde(default)]
    pub sealed: bool,
}

/// The fields a trait promises.
pub fn trait_fields(name: &str) -> &'static [&'static str] {
    match name {
        "titled" => &["title"],
        "timed" => &["starts_at"],
        "located" => &["location"],
        _ => &[],
    }
}

impl TypeDef {
    /// A type must define the fields its traits promise.
    pub fn check(&self) -> Result<(), String> {
        for t in &self.traits {
            for f in trait_fields(t) {
                if !self.fields.contains_key(*f) {
                    return Err(format!(
                        "type {} claims trait {t} but has no field {f}",
                        self.name
                    ));
                }
            }
        }
        for (name, op) in &self.ops {
            for (field, param) in &op.sets {
                if !self.fields.contains_key(field) {
                    return Err(format!("{}.{name} sets unknown field {field}", self.name));
                }
                if !op.params.contains_key(param) {
                    return Err(format!("{}.{name} uses unknown param {param}", self.name));
                }
            }
        }
        Ok(())
    }
}

/// A set of known types: the built-in ones plus those defined by objects.
#[derive(Clone, Debug, PartialEq)]
pub struct Registry {
    types: BTreeMap<String, TypeDef>,
}

fn field(kind: Kind) -> FieldDef {
    FieldDef {
        kind,
        required: false,
        merge: Merge::KeepBoth,
    }
}

fn required(kind: Kind) -> FieldDef {
    FieldDef {
        required: true,
        ..field(kind)
    }
}

fn set() -> FieldDef {
    FieldDef {
        merge: Merge::Set,
        ..field(Kind::List)
    }
}

fn rename_op() -> OpDef {
    OpDef {
        params: [("title".to_string(), Kind::Text)].into(),
        sets: [("title".to_string(), "title".to_string())].into(),
        ..OpDef::default()
    }
}

fn def<const F: usize>(
    name: &str,
    traits: &[&str],
    body: bool,
    fields: [(&str, FieldDef); F],
) -> TypeDef {
    TypeDef {
        name: name.to_string(),
        traits: traits.iter().map(|t| t.to_string()).collect(),
        fields: fields
            .into_iter()
            .map(|(k, f)| (k.to_string(), f))
            .collect(),
        body,
        ..TypeDef::default()
    }
}

/// The types every store starts with. These are data shapes only: the actual
/// list of types can grow with `locus.type` objects, without a release.
pub fn builtin_types() -> Vec<TypeDef> {
    let mut note = def(
        "note",
        &["titled"],
        true,
        [
            ("title", field(Kind::Text)),
            ("parent", field(Kind::Ref)),
            ("notebook", field(Kind::Text)),
            ("path", field(Kind::Text)),
            ("tags", set()),
        ],
    );
    note.ops.insert("rename".to_string(), rename_op());
    note.open = true;

    let mut message = def(
        "message",
        &["timed"],
        true,
        [
            ("starts_at", field(Kind::Time)),
            ("to", set()),
            ("status", field(Kind::Text)),
        ],
    );
    message.ops.insert(
        "send".to_string(),
        OpDef {
            params: [("status".to_string(), Kind::Text)].into(),
            sets: [("status".to_string(), "status".to_string())].into(),
            approval: Approval::Human,
            ..OpDef::default()
        },
    );

    let mut host = def(
        "host",
        &["titled"],
        false,
        [
            ("title", field(Kind::Text)),
            ("generation", field(Kind::Text)),
        ],
    );
    host.ops.insert(
        "rebuild".to_string(),
        OpDef {
            params: [("generation".to_string(), Kind::Text)].into(),
            sets: [("generation".to_string(), "generation".to_string())].into(),
            approval: Approval::Human,
            ..OpDef::default()
        },
    );

    let mut meeting = def(
        "meeting",
        &["titled", "timed", "located"],
        true,
        [
            ("title", required(Kind::Text)),
            ("starts_at", field(Kind::Time)),
            ("ends_at", field(Kind::Time)),
            ("location", field(Kind::Text)),
            ("attendees", set()),
        ],
    );
    meeting.ops.insert("rename".to_string(), rename_op());
    meeting.ops.insert(
        "reschedule".to_string(),
        OpDef {
            params: [
                ("starts_at".to_string(), Kind::Time),
                ("ends_at".to_string(), Kind::Time),
            ]
            .into(),
            sets: [
                ("starts_at".to_string(), "starts_at".to_string()),
                ("ends_at".to_string(), "ends_at".to_string()),
            ]
            .into(),
            ..OpDef::default()
        },
    );

    let task = def(
        "task",
        &["titled"],
        true,
        [
            ("title", required(Kind::Text)),
            ("done", field(Kind::Bool)),
            ("due", field(Kind::Time)),
        ],
    );

    let mut file = def(
        "file",
        &["titled", "located"],
        false,
        [
            ("title", required(Kind::Text)),
            ("location", field(Kind::Text)),
            ("blob", field(Kind::Blob)),
            ("size", field(Kind::Int)),
            ("mime", field(Kind::Text)),
            ("parent", field(Kind::Ref)),
        ],
    );
    file.open = true;

    let mut photo = def(
        "photo",
        &["titled", "timed", "located"],
        false,
        [
            ("title", field(Kind::Text)),
            ("starts_at", field(Kind::Time)),
            ("location", field(Kind::Text)),
            ("blob", field(Kind::Blob)),
            ("rotation", field(Kind::Int)),
            ("crop", field(Kind::Map)),
            ("albums", set()),
        ],
    );
    photo.open = true;

    let mut person = def(
        "person",
        &["titled"],
        true,
        [
            ("title", required(Kind::Text)),
            ("emails", set()),
            ("phones", set()),
        ],
    );
    person.open = true;

    let collection = def(
        "collection",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("parent", field(Kind::Ref)),
            ("kind", field(Kind::Text)),
            ("members", set()),
        ],
    );

    let page = def(
        "page",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("blob", required(Kind::Blob)),
        ],
    );

    let view = def(
        "locus.view",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("spec", required(Kind::Text)),
        ],
    );

    let reaction = def(
        "locus.reaction",
        &[],
        false,
        [
            ("view", required(Kind::Ref)),
            ("op", required(Kind::Text)),
            ("args", field(Kind::Map)),
            ("agent", required(Kind::Ref)),
            ("auto", field(Kind::Bool)),
        ],
    );

    let mut member = def(
        "locus.member",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("admin", field(Kind::Bool)),
            ("disabled", field(Kind::Bool)),
            ("keys", set()),
        ],
    );
    member.protected = true;

    let mut agent = def(
        "locus.agent",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("autonomous", field(Kind::Bool)),
            ("disabled", field(Kind::Bool)),
            ("keys", set()),
        ],
    );
    agent.protected = true;

    let mut grant = def(
        "locus.grant",
        &[],
        false,
        [
            ("subject", required(Kind::Ref)),
            ("action", required(Kind::Text)),
            ("scope_type", field(Kind::Text)),
            ("scope_object", field(Kind::Ref)),
            ("mode", field(Kind::Text)),
            ("models", field(Kind::List)),
            ("efforts", field(Kind::List)),
        ],
    );
    grant.protected = true;

    let mut type_type = def(
        "locus.type",
        &[],
        false,
        [
            ("name", required(Kind::Text)),
            ("def", required(Kind::Text)),
        ],
    );
    type_type.protected = true;

    let mut proposal = def(
        "locus.proposal",
        &["titled"],
        false,
        [
            ("title", required(Kind::Text)),
            ("status", required(Kind::Text)),
            ("ops", required(Kind::List)),
            ("decided_by", field(Kind::Ref)),
            ("reason", field(Kind::Text)),
            ("applied", field(Kind::List)),
        ],
    );
    proposal.sealed = true;

    vec![
        note, message, host, meeting, task, file, photo, person, collection, page, view, reaction,
        member, agent, grant, type_type, proposal,
    ]
}

impl Default for Registry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl Registry {
    pub fn builtin() -> Self {
        Registry {
            types: builtin_types()
                .into_iter()
                .map(|t| (t.name.clone(), t))
                .collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&TypeDef> {
        self.types.get(name)
    }

    /// Add a type defined at run time. Built-in types cannot be replaced, so a
    /// `locus.type` object cannot quietly unprotect `locus.grant`.
    pub fn define(&mut self, def: TypeDef) -> Result<(), String> {
        def.check()?;
        if def.name.starts_with("locus.") || Registry::builtin().get(&def.name).is_some() {
            return Err(format!("type {} is built in", def.name));
        }
        self.types.insert(def.name.clone(), def);
        Ok(())
    }

    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.types.keys()
    }

    /// The declaration of an operation such as `note.rename`.
    pub fn op(&self, name: &str) -> Option<(&TypeDef, &OpDef)> {
        let (ty, op) = name.rsplit_once('.')?;
        let t = self.types.get(ty)?;
        Some((t, t.ops.get(op)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_types_keep_their_trait_promises() {
        for t in builtin_types() {
            t.check().unwrap();
        }
    }

    #[test]
    fn builtin_types_cannot_be_replaced() {
        let mut r = Registry::builtin();
        let bad = TypeDef {
            name: "locus.grant".to_string(),
            ..TypeDef::default()
        };
        assert!(r.define(bad).is_err());
        let ok = TypeDef {
            name: "recipe".to_string(),
            ..TypeDef::default()
        };
        r.define(ok).unwrap();
        assert!(r.get("recipe").is_some());
    }
}
