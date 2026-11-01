//! Views: live queries over the rest. A view is derived and can always be
//! rebuilt. Exports, the timeline and reactions are all views.

use crate::id::ObjectId;
use crate::op::Object;
use crate::types::Registry;
use crate::value::Value;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    All,
    Type(String),
    /// Objects whose type implements a trait, such as `timed`.
    Trait(String),
    Eq(String, Value),
    /// Case-insensitive search in a text field.
    Contains(String, String),
    /// Case-insensitive search in the body.
    BodyContains(String),
    /// Objects that reference this one.
    Refers(ObjectId),
    IncludeDeleted,
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewSpec {
    pub filter: Filter,
    /// Field to sort by.
    pub sort: Option<String>,
    #[serde(default)]
    pub descending: bool,
    pub limit: Option<usize>,
}

impl ViewSpec {
    pub fn of(filter: Filter) -> Self {
        ViewSpec {
            filter,
            sort: None,
            descending: false,
            limit: None,
        }
    }
}

fn has_ref(o: &Object, target: ObjectId) -> bool {
    let mut refs = Vec::new();
    o.fields.values().for_each(|v| v.references(&mut refs));
    refs.iter().any(|r| r.id == target)
        || o.body
            .as_deref()
            .is_some_and(|b| crate::body_refs(b).contains(&target))
}

impl Filter {
    fn wants_deleted(&self) -> bool {
        match self {
            Filter::IncludeDeleted => true,
            Filter::And(l) | Filter::Or(l) => l.iter().any(Filter::wants_deleted),
            _ => false,
        }
    }

    pub fn matches(&self, o: &Object, reg: &Registry) -> bool {
        match self {
            Filter::All | Filter::IncludeDeleted => true,
            Filter::Type(t) => &o.type_name == t,
            Filter::Trait(t) => reg
                .get(&o.type_name)
                .is_some_and(|d| d.traits.iter().any(|x| x == t)),
            Filter::Eq(f, v) => o.fields.get(f) == Some(v),
            Filter::Contains(f, q) => o
                .fields
                .get(f)
                .and_then(Value::as_text)
                .is_some_and(|t| t.to_lowercase().contains(&q.to_lowercase())),
            Filter::BodyContains(q) => o
                .body
                .as_deref()
                .is_some_and(|t| t.to_lowercase().contains(&q.to_lowercase())),
            Filter::Refers(id) => has_ref(o, *id),
            Filter::And(l) => l.iter().all(|f| f.matches(o, reg)),
            Filter::Or(l) => l.iter().any(|f| f.matches(o, reg)),
            Filter::Not(f) => !f.matches(o, reg),
        }
    }
}

fn sort_key(o: &Object, field: &str) -> (u8, i64, String) {
    match o.fields.get(field) {
        Some(Value::Time(t) | Value::Int(t)) => (0, *t, String::new()),
        Some(Value::Text(s)) => (1, 0, s.to_lowercase()),
        Some(_) => (2, 0, String::new()),
        None => (3, 0, String::new()),
    }
}

/// Evaluate a view over a set of objects.
pub fn evaluate(spec: &ViewSpec, objects: Vec<Object>, reg: &Registry) -> Vec<Object> {
    let deleted = spec.filter.wants_deleted();
    let mut out: Vec<Object> = objects
        .into_iter()
        .filter(|o| !o.purged && (deleted || !o.deleted) && spec.filter.matches(o, reg))
        .collect();
    if let Some(f) = &spec.sort {
        out.sort_by_key(|o| sort_key(o, f));
    } else {
        out.sort_by_key(|o| o.id);
    }
    if spec.descending {
        out.reverse();
    }
    if let Some(n) = spec.limit {
        out.truncate(n);
    }
    out
}
