//! `%locus` command plane: clan members, audit log, approvals and named
//! function calls. Destructive functions are gated server-side behind human
//! approval; this client only forwards the call.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Client, Result, check_ok};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub ship: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
}

/// A call waiting for approval, or one that has run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub actor: String,
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub args: Value,
    pub status: String,
    #[serde(rename = "approvedBy", default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub result: String,
}

/// One append-only audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    pub actor: String,
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub result: String,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    members: Option<Vec<Member>>,
    pending: Option<Vec<Job>>,
    audit: Option<Vec<AuditEntry>>,
    ship: Option<String>,
    name: Option<String>,
    role: Option<String>,
}

impl Client {
    /// The authenticated principal (session cookie or `X-Api-Key`).
    pub async fn whoami(&self) -> Result<Member> {
        let env: Envelope = self.get_json("/locus/~/v1/whoami").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(Member {
            ship: env.ship.unwrap_or_default(),
            name: env.name.unwrap_or_default(),
            role: env.role.unwrap_or_default(),
        })
    }

    pub async fn members(&self) -> Result<Vec<Member>> {
        let env: Envelope = self.get_json("/locus/~/v1/members").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.members.unwrap_or_default())
    }

    pub async fn audit(&self) -> Result<Vec<AuditEntry>> {
        let env: Envelope = self.get_json("/locus/~/v1/audit").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.audit.unwrap_or_default())
    }

    /// OS commands waiting for human approval.
    pub async fn pending(&self) -> Result<Vec<Job>> {
        let env: Envelope = self.get_json("/locus/~/v1/pending").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.pending.unwrap_or_default())
    }

    /// Invoke a named function. Returns the whole reply object on success
    /// (its shape depends on the function); `ok: false` becomes [`Error::Api`]
    /// or [`Error::Conflict`].
    pub async fn call(&self, function: &str, args: Value) -> Result<Value> {
        let path = "/locus/~/v1/call";
        let reply: Value = self
            .json(
                Method::POST,
                path,
                Some(json!({ "fn": function, "args": args })),
            )
            .await?;
        let ok = reply.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let text = |key: &str| reply.get(key).and_then(Value::as_str).map(str::to_owned);
        check_ok(ok, text("error"), text("message"))?;
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    #[test]
    fn member_decodes() {
        let m: Member =
            serde_json::from_str(r#"{"ship":"~zod","name":"owner","role":"owner"}"#).unwrap();
        assert_eq!(m.ship, "~zod");
        assert_eq!(m.role, "owner");
    }

    #[test]
    fn members_envelope_decodes() {
        let raw = r#"{"ok":true,"members":[{"ship":"~zod","name":"owner","role":"owner"},{"ship":"~nec","name":"kid","role":"viewer"}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        let list = env.members.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].role, "viewer");
    }

    #[test]
    fn whoami_envelope_is_flat() {
        let raw = r#"{"ok":true,"ship":"~zod","name":"owner","role":"owner"}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        assert!(env.ok);
        assert_eq!(env.ship.as_deref(), Some("~zod"));
        assert_eq!(env.role.as_deref(), Some("owner"));
    }

    #[test]
    fn pending_envelope_decodes_jobs() {
        let raw = r#"{"ok":true,"pending":[{"id":3,"actor":"~nec","fn":"pkg.install","args":{"name":"htop"},"status":"pending","approvedBy":null,"result":""}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        let jobs = env.pending.unwrap();
        assert_eq!(jobs[0].function, "pkg.install");
        assert_eq!(jobs[0].args["name"], "htop");
        assert_eq!(jobs[0].approved_by, None);
    }

    #[test]
    fn audit_envelope_decodes() {
        let raw =
            r#"{"ok":true,"audit":[{"id":1,"actor":"~zod","fn":"notes.create","result":"ok"}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        let entries = env.audit.unwrap();
        assert_eq!(entries[0].function, "notes.create");
        assert_eq!(entries[0].result, "ok");
    }

    #[test]
    fn denied_call_envelope_is_error() {
        let raw = r#"{"ok":false,"error":"denied","message":"this function cannot empty history"}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        match check_ok(env.ok, env.error, env.message) {
            Err(Error::Api { error, message }) => {
                assert_eq!(error, "denied");
                assert!(message.contains("cannot"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
