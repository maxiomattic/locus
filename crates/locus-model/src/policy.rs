//! Permissions. Members, agents and grants are ordinary objects, so changing
//! one is an audited operation. This module only reads them.
//!
//! Policy answers two questions: may this actor invoke this operation on this
//! object, and which implementations may it use.

use crate::id::ObjectId;
use crate::op::{ActorKindRequired, ImplUse, Object};
use crate::types::{Approval, Registry};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The action name for reading an object.
pub const READ: &str = "read";
/// The action name for uploading blob bytes.
pub const BLOB_PUT: &str = "blob.put";
/// Must be granted by name: no wildcard reaches it.
pub const PURGE: &str = "object.purge";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    Human { admin: bool },
    Agent { autonomous: bool },
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActorInfo {
    kind: ActorKind,
    disabled: bool,
}

/// Whether a grant lets an action apply or only be proposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Propose,
    Apply,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Grant {
    subject: ObjectId,
    action: String,
    scope_type: Option<String>,
    scope_object: Option<ObjectId>,
    mode: Mode,
    models: Vec<String>,
    efforts: Vec<String>,
}

/// Who is acting: the actor, and the person it acts for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub actor: ObjectId,
    pub invoker: Option<ObjectId>,
}

impl Principal {
    pub fn acting(actor: ObjectId) -> Self {
        Principal {
            actor,
            invoker: None,
        }
    }

    pub fn for_invoker(actor: ObjectId, invoker: ObjectId) -> Self {
        Principal {
            actor,
            invoker: Some(invoker),
        }
    }

    pub const SYSTEM: Principal = Principal {
        actor: ObjectId::SYSTEM,
        invoker: None,
    };
}

/// What is being attempted.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    pub action: &'a str,
    pub type_name: &'a str,
    pub object: Option<ObjectId>,
    pub implementation: Option<&'a ImplUse>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Apply,
    /// Allowed only as a proposal someone reviews.
    Propose,
    Refuse(String),
}

/// The grants and actors in force, read from their objects.
#[derive(Clone, Debug, Default)]
pub struct Policy {
    actors: HashMap<ObjectId, ActorInfo>,
    grants: Vec<Grant>,
}

fn matches(pattern: &str, action: &str) -> bool {
    if pattern == action {
        return true;
    }
    if action == PURGE {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    pattern
        .strip_suffix(".*")
        .is_some_and(|p| action.strip_prefix(p).is_some_and(|r| r.starts_with('.')))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::List(l)) => l
            .iter()
            .filter_map(Value::as_text)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

impl Policy {
    /// Build from the `locus.member`, `locus.agent` and `locus.grant` objects.
    pub fn from_objects<'a>(objects: impl IntoIterator<Item = &'a Object>) -> Self {
        let mut p = Policy::default();
        for o in objects {
            if o.deleted {
                continue;
            }
            let flag = |n: &str| o.fields.get(n).and_then(Value::as_bool).unwrap_or(false);
            match o.type_name.as_str() {
                "locus.member" => {
                    p.actors.insert(
                        o.id,
                        ActorInfo {
                            kind: ActorKind::Human {
                                admin: flag("admin"),
                            },
                            disabled: flag("disabled"),
                        },
                    );
                }
                "locus.agent" => {
                    p.actors.insert(
                        o.id,
                        ActorInfo {
                            kind: ActorKind::Agent {
                                autonomous: flag("autonomous"),
                            },
                            disabled: flag("disabled"),
                        },
                    );
                }
                "locus.grant" => {
                    let Some(subject) = o.fields.get("subject").and_then(Value::as_ref_id) else {
                        continue;
                    };
                    let Some(action) = o.fields.get("action").and_then(Value::as_text) else {
                        continue;
                    };
                    p.grants.push(Grant {
                        subject,
                        action: action.to_string(),
                        scope_type: o
                            .fields
                            .get("scope_type")
                            .and_then(Value::as_text)
                            .map(str::to_string),
                        scope_object: o.fields.get("scope_object").and_then(Value::as_ref_id),
                        mode: match o.fields.get("mode").and_then(Value::as_text) {
                            Some("propose") => Mode::Propose,
                            _ => Mode::Apply,
                        },
                        models: strings(o.fields.get("models")),
                        efforts: strings(o.fields.get("efforts")),
                    });
                }
                _ => {}
            }
        }
        p
    }

    pub fn kind(&self, actor: ObjectId) -> Option<ActorKind> {
        if actor == ObjectId::SYSTEM {
            return Some(ActorKind::System);
        }
        self.actors.get(&actor).map(|a| a.kind)
    }

    fn single(&self, actor: ObjectId, req: &Request, reg: &Registry) -> Result<Mode, String> {
        if actor == ObjectId::SYSTEM {
            return Ok(Mode::Apply);
        }
        let Some(info) = self.actors.get(&actor) else {
            return Err(format!("{actor} is not a member or agent of this clan"));
        };
        if info.disabled {
            return Err(format!("{actor} is disabled"));
        }
        let is_agent = matches!(info.kind, ActorKind::Agent { .. });
        let protected = reg.get(req.type_name).is_some_and(|t| t.protected);
        if is_agent {
            if req.action == PURGE {
                return Err("agents can never purge".to_string());
            }
            if protected && req.action != READ {
                return Err("agents can never change identity or policy".to_string());
            }
        }
        if matches!(info.kind, ActorKind::Human { admin: true }) {
            return Ok(Mode::Apply);
        }
        if protected && req.action != READ {
            return Err("only an administrator can change identity or policy".to_string());
        }
        let mut best = None;
        let mut blocked_impl = None;
        for g in self.grants.iter().filter(|g| g.subject == actor) {
            if !matches(&g.action, req.action) {
                continue;
            }
            if g.scope_type.as_deref().is_some_and(|t| t != req.type_name) {
                continue;
            }
            if g.scope_object.is_some() && g.scope_object != req.object {
                continue;
            }
            if let Some(i) = req.implementation
                && ((!g.models.is_empty() && !g.models.contains(&i.model))
                    || (!g.efforts.is_empty() && !g.efforts.contains(&i.effort)))
            {
                blocked_impl = Some(format!("model {} at effort {}", i.model, i.effort));
                continue;
            }
            best = best.max(Some(g.mode));
        }
        best.ok_or_else(|| match blocked_impl {
            Some(i) => format!("{actor} may not use {i} for {}", req.action),
            None => format!(
                "{actor} has no grant for {} on {}",
                req.action, req.type_name
            ),
        })
    }

    /// May this principal do this? An agent gets the intersection of its own
    /// grants and its invoker's. With no invoker it must be a named autonomous
    /// agent, running on its own grant.
    pub fn decide(&self, who: &Principal, req: &Request, reg: &Registry) -> Decision {
        let mut mode = match self.single(who.actor, req, reg) {
            Ok(m) => m,
            Err(e) => return Decision::Refuse(e),
        };
        let kind = self.kind(who.actor);
        let def = reg.op(req.action).map(|(_, d)| d);
        if let Some(ActorKind::Agent { autonomous }) = kind {
            if matches!(req.action, "proposal.approve" | "proposal.reject") {
                return Decision::Refuse("only a person can decide a proposal".to_string());
            }
            match who.invoker {
                Some(i) => {
                    if !matches!(self.kind(i), Some(ActorKind::Human { .. })) {
                        return Decision::Refuse(format!("{i} is not a person"));
                    }
                    match self.single(i, req, reg) {
                        Ok(m) => mode = mode.min(m),
                        Err(e) => return Decision::Refuse(format!("invoker: {e}")),
                    }
                }
                None if autonomous => {}
                None => {
                    return Decision::Refuse(format!(
                        "agent {} has no invoker and is not autonomous",
                        who.actor
                    ));
                }
            }
            if def.is_some_and(|d| d.approval == Approval::Human) {
                mode = Mode::Propose;
            }
        } else if who.invoker.is_some() {
            return Decision::Refuse("only an agent acts for an invoker".to_string());
        }
        if def.is_some_and(|d| d.only == ActorKindRequired::Human)
            && matches!(kind, Some(ActorKind::Agent { .. }))
        {
            return Decision::Refuse(format!("{} is for people only", req.action));
        }
        match mode {
            Mode::Apply => Decision::Apply,
            Mode::Propose => Decision::Propose,
        }
    }

    /// Shorthand for the read check.
    pub fn may_read(&self, who: &Principal, object: &Object, reg: &Registry) -> bool {
        let req = Request {
            action: READ,
            type_name: &object.type_name,
            object: Some(object.id),
            implementation: None,
        };
        !matches!(self.decide(who, &req, reg), Decision::Refuse(_))
    }
}
