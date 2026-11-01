//! Reviewing a proposal: a branch of unapplied operations shown as a diff.

use crate::id::{ObjectId, OpId};
use crate::op::{BodyCheck, Effect, Object, Op, apply, plan};
use crate::types::Registry;
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;

/// The operations a proposal holds.
pub fn proposal_ops(doc: &Object) -> Result<Vec<Op>, String> {
    let Some(Value::List(l)) = doc.fields.get("ops") else {
        return Err("the proposal has no operations".to_string());
    };
    l.iter()
        .map(|v| {
            let json = v.as_text().ok_or("a proposal op must be text")?;
            serde_json::from_str(json).map_err(|e| e.to_string())
        })
        .collect()
}

/// Encode operations for `proposal.create`.
pub fn encode_ops(ops: &[Op]) -> Value {
    Value::List(
        ops.iter()
            .map(|o| Value::Text(serde_json::to_string(o).expect("an Op always serializes")))
            .collect(),
    )
}

/// What one proposed operation would do if applied now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffEntry {
    pub index: usize,
    pub op: OpId,
    pub object: ObjectId,
    pub name: String,
    pub intent: String,
    pub title: Option<String>,
    pub effects: Vec<Effect>,
    /// Why it would not apply, if it would not.
    pub error: Option<String>,
}

/// Dry-run operations in order, against current state, changing nothing.
pub fn diff(
    ops: &[Op],
    mut load: impl FnMut(ObjectId) -> Option<Object>,
    reg: &Registry,
    check: BodyCheck,
) -> Vec<DiffEntry> {
    let mut docs: BTreeMap<ObjectId, Option<Object>> = BTreeMap::new();
    let mut out = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        let doc = docs.entry(op.object).or_insert_with(|| load(op.object));
        let planned = plan(doc.as_ref(), op, reg, check);
        let (effects, error) = match planned {
            Ok(e) => {
                apply(doc, &e, op.object, op.at);
                (e, None)
            }
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        out.push(DiffEntry {
            index,
            op: op.id,
            object: op.object,
            name: op.name.clone(),
            intent: op.intent.clone(),
            title: doc.as_ref().and_then(|d| d.title().map(str::to_string)),
            effects,
            error,
        });
    }
    out
}

fn show(v: &Option<Value>) -> String {
    match v {
        None => "(nothing)".to_string(),
        Some(Value::Text(s)) => format!("{s:?}"),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Render a diff for reading.
pub fn render(entries: &[DiffEntry]) -> String {
    let mut s = String::new();
    for e in entries {
        let name = e.title.clone().unwrap_or_else(|| e.object.to_string());
        let _ = writeln!(s, "#{} {} on {name}: {}", e.index, e.name, e.intent);
        if let Some(err) = &e.error {
            let _ = writeln!(s, "  ! would fail: {err}");
        }
        for x in &e.effects {
            let _ = match x {
                Effect::Created { type_name, .. } => writeln!(s, "  + new {type_name}"),
                Effect::Set { field, old, new } => {
                    writeln!(s, "  ~ {field}: {} -> {}", show(old), show(new))
                }
                Effect::Added { field, value } => {
                    writeln!(s, "  + {field}: {}", show(&Some(value.clone())))
                }
                Effect::Removed { field, value } => {
                    writeln!(s, "  - {field}: {}", show(&Some(value.clone())))
                }
                Effect::BodySet { .. } => writeln!(s, "  ~ body"),
                Effect::Deleted => writeln!(s, "  - deleted"),
                Effect::Restored => writeln!(s, "  + restored"),
                Effect::ConflictOpened(c) => writeln!(
                    s,
                    "  ! conflict on {}: keeps both, proposed {}",
                    c.field.as_deref().unwrap_or("delete"),
                    show(&c.value)
                ),
                other => writeln!(s, "  {other:?}"),
            };
        }
    }
    s
}

/// The operations that undo one log entry, as (name, args) pairs in the order
/// to run them. Fails if a purge took what is needed.
pub fn inverse(op: &Op, effects: &[Effect]) -> Result<Vec<(String, crate::Fields)>, String> {
    use crate::value::fields;
    let mut out = Vec::new();
    for e in effects.iter().rev() {
        match e {
            Effect::Created { .. } => out.push(("object.delete".to_string(), crate::Fields::new())),
            Effect::Deleted => out.push(("object.restore".to_string(), crate::Fields::new())),
            Effect::Restored => out.push(("object.delete".to_string(), crate::Fields::new())),
            Effect::Set { field, old, .. } => {
                if old == &Some(Value::Redacted) {
                    return Err(format!("the old value of {field} was purged"));
                }
                let mut args = crate::Fields::new();
                match old {
                    Some(v) => {
                        args.insert(
                            "fields".to_string(),
                            Value::Map(fields([(field.as_str(), v.clone())])),
                        );
                    }
                    None => {
                        args.insert(
                            "unset".to_string(),
                            Value::List(vec![Value::text(field.clone())]),
                        );
                    }
                }
                args.insert("reverts".to_string(), Value::text(op.id.to_string()));
                out.push(("object.revert".to_string(), args));
            }
            Effect::Added { field, value } => out.push((
                "object.remove".to_string(),
                fields([("field", field.as_str().into()), ("value", value.clone())]),
            )),
            Effect::Removed { field, value } => out.push((
                "object.add".to_string(),
                fields([("field", field.as_str().into()), ("value", value.clone())]),
            )),
            Effect::BodySet { old, .. } => out.push((
                "body.set".to_string(),
                fields([("html", old.clone().unwrap_or_default().into())]),
            )),
            Effect::Redacted | Effect::Purged(_) => {
                return Err("a purge removed what this would restore".to_string());
            }
            Effect::ConflictOpened(_)
            | Effect::ConflictAmended { .. }
            | Effect::ConflictClosed(_)
            | Effect::Snapshot(_) => {}
        }
    }
    Ok(out)
}
