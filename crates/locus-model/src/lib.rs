//! The Locus data primitives, as pure data and pure functions: object, type,
//! operation, reference, blob and view, plus the policy that governs them.
//! Nothing here touches a disk, a network, a UI or Urbit.

pub mod id;
pub mod op;
pub mod policy;
pub mod proposal;
pub mod types;
pub mod value;
pub mod view;

pub use id::{BlobHash, Id, ObjectId, OpId, ServerId};
pub use op::{Conflict, Effect, ImplUse, Object, Op, PlanError, Purge, apply, plan};
pub use policy::{ActorKind, Decision, Policy, Principal, Request};
pub use types::{Approval, FieldDef, Implementation, Kind, Merge, OpDef, Registry, TypeDef};
pub use value::{Fields, Reference, Value, fields};
pub use view::{Filter, ViewSpec, evaluate};

#[cfg(test)]
mod tests;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The objects a body references inline, as `locus:<id>` links.
pub fn body_refs(html: &str) -> Vec<ObjectId> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("locus:") {
        rest = &rest[i + 6..];
        if let Some(id) = rest.get(..26).and_then(|s| s.parse().ok()) {
            out.push(id);
        }
    }
    out
}

/// The blobs a body embeds, as `blob:<hash>` sources.
pub fn body_blobs(html: &str) -> Vec<BlobHash> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("blob:") {
        rest = &rest[i + 5..];
        if let Some(h) = rest.get(..64).and_then(|s| s.parse().ok()) {
            out.push(h);
        }
    }
    out
}
