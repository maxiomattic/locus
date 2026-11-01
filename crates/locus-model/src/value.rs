//! Field values and references.

use crate::id::{BlobHash, ObjectId, ServerId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A pointer to an object's ID, which may pin a version. The graph is an index
/// built from these, never stored on its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Reference {
    pub id: ObjectId,
    /// Pin to this revision of the target.
    pub pin: Option<u64>,
    /// Where the target lives, if that is another clan's server.
    pub home: Option<ServerId>,
}

impl Reference {
    pub fn to(id: ObjectId) -> Self {
        Reference {
            id,
            pin: None,
            home: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    /// Milliseconds since the Unix epoch.
    Time(i64),
    Ref(Reference),
    Blob(BlobHash),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// What a purge leaves behind.
    Redacted,
}

impl Value {
    pub fn text(s: impl Into<String>) -> Self {
        Value::Text(s.into())
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_ref_id(&self) -> Option<ObjectId> {
        match self {
            Value::Ref(r) => Some(r.id),
            _ => None,
        }
    }

    /// Every reference inside this value.
    pub fn references(&self, out: &mut Vec<Reference>) {
        match self {
            Value::Ref(r) => out.push(r.clone()),
            Value::List(l) => l.iter().for_each(|v| v.references(out)),
            Value::Map(m) => m.values().for_each(|v| v.references(out)),
            _ => {}
        }
    }

    /// Every blob inside this value.
    pub fn blobs(&self, out: &mut Vec<BlobHash>) {
        match self {
            Value::Blob(b) => out.push(*b),
            Value::List(l) => l.iter().for_each(|v| v.blobs(out)),
            Value::Map(m) => m.values().for_each(|v| v.blobs(out)),
            _ => {}
        }
    }

    /// Replace a blob everywhere inside this value. True if anything changed.
    pub fn scrub_blob(&mut self, hash: &BlobHash) -> bool {
        match self {
            Value::Blob(b) if b == hash => {
                *self = Value::Redacted;
                true
            }
            Value::List(l) => l.iter_mut().map(|v| v.scrub_blob(hash)).any_changed(),
            Value::Map(m) => m.values_mut().map(|v| v.scrub_blob(hash)).any_changed(),
            _ => false,
        }
    }
}

/// Consumes a whole iterator, unlike `any`, which would stop at the first hit.
trait AnyChanged: Iterator<Item = bool> + Sized {
    fn any_changed(self) -> bool {
        self.filter(|c| *c).count() > 0
    }
}

impl<I: Iterator<Item = bool>> AnyChanged for I {}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::text(s)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<ObjectId> for Value {
    fn from(id: ObjectId) -> Self {
        Value::Ref(Reference::to(id))
    }
}

/// A map of fields or arguments.
pub type Fields = BTreeMap<String, Value>;

/// Build a [`Fields`] map from pairs.
pub fn fields<const N: usize>(pairs: [(&str, Value); N]) -> Fields {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}
