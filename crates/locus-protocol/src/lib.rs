//! HTTP client for `%locus` (clan members, named functions, audit).

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::Duration;
use thiserror::Error;
use ureq::Agent;

#[derive(Debug, Error)]
pub enum Error {
    #[error("login failed (HTTP {0}); check the web code and that the ship is up")]
    Login(u16),
    #[error("HTTP {status} on {path}: {body}")]
    Http {
        status: u16,
        path: String,
        body: String,
    },
    #[error("not JSON from {path}: {detail}")]
    Json { path: String, detail: String },
    #[error("{typ}: {message}")]
    Api { typ: String, message: String },
    #[error("{0}")]
    Transport(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub ship: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub actor: String,
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub args: serde_json::Value,
    pub status: String,
    #[serde(rename = "approvedBy", default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    pub actor: String,
    #[serde(rename = "fn")]
    pub function: String,
    #[serde(default)]
    pub result: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Envelope {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<Member>>,
    #[serde(default)]
    pub member: Option<Member>,
    #[serde(default)]
    pub pending: Option<Vec<Job>>,
    #[serde(default)]
    pub jobs: Option<Vec<Job>>,
    #[serde(default)]
    pub job: Option<Job>,
    #[serde(default)]
    pub audit: Option<Vec<AuditEntry>>,
    #[serde(default, rename = "apiKey")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub ship: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

impl Envelope {
    pub fn into_ok(self) -> Result<Self> {
        if self.ok {
            Ok(self)
        } else {
            Err(Error::Api {
                typ: self.error.unwrap_or_else(|| "error".into()),
                message: self.message.unwrap_or_default(),
            })
        }
    }
}

#[derive(Clone)]
pub struct Client {
    agent: Agent,
    base: String,
    api_key: Option<String>,
}

impl Client {
    pub fn new(base: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        Self {
            agent,
            base: trim_slash(base.into()),
            api_key: None,
        }
    }

    pub fn with_api_key(base: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut c = Self::new(base);
        c.api_key = Some(api_key.into());
        c
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn login(&mut self, code: &str) -> Result<()> {
        let url = format!("{}/~/login", self.base);
        let resp = self
            .agent
            .post(&url)
            .send_form(&[("password", code.trim())])
            .map_err(|e| Error::Transport(e.to_string()))?;
        let status = resp.status();
        if status != 204 && status != 200 {
            return Err(Error::Login(status));
        }
        self.api_key = Some(self.fetch_api_key()?);
        Ok(())
    }

    fn fetch_api_key(&self) -> Result<String> {
        // Gall on-peek is [%x %v0 %api-key ~]; Eyre's /~/scry omits the care.
        let path = "/~/scry/locus/v0/api-key.json";
        let value: serde_json::Value = self.get_json(path)?;
        match value {
            serde_json::Value::String(s) if !s.is_empty() => Ok(s),
            _ => Err(Error::Api {
                typ: "api-key".into(),
                message: "empty".into(),
            }),
        }
    }

    pub fn members(&self) -> Result<Vec<Member>> {
        let env: Envelope = self.get_json("/locus/~/v1/members")?;
        Ok(env.into_ok()?.members.unwrap_or_default())
    }

    pub fn audit(&self) -> Result<Vec<AuditEntry>> {
        let env: Envelope = self.get_json("/locus/~/v1/audit")?;
        Ok(env.into_ok()?.audit.unwrap_or_default())
    }

    pub fn pending(&self) -> Result<Vec<Job>> {
        let env: Envelope = self.get_json("/locus/~/v1/pending")?;
        Ok(env.into_ok()?.pending.unwrap_or_default())
    }

    pub fn jobs(&self) -> Result<Vec<Job>> {
        let env: Envelope = self.get_json("/locus/~/v1/jobs")?;
        Ok(env.into_ok()?.jobs.unwrap_or_default())
    }

    pub fn whoami(&self) -> Result<Member> {
        let env: Envelope = self.get_json("/locus/~/v1/whoami")?;
        let env = env.into_ok()?;
        Ok(Member {
            ship: env.ship.unwrap_or_default(),
            name: env.name.unwrap_or_default(),
            role: env.role.unwrap_or_default(),
        })
    }

    pub fn call(
        &self,
        fn_name: &str,
        as_ship: Option<&str>,
        args: serde_json::Value,
    ) -> Result<Envelope> {
        let mut body = serde_json::json!({ "fn": fn_name, "args": args });
        if let Some(ship) = as_ship {
            body["as"] = serde_json::Value::String(ship.to_string());
        }
        let env: Envelope = self.post_json("/locus/~/v1/call", body)?;
        env.into_ok()
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base);
        let mut req = self.agent.get(&url).set("Accept", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("X-Api-Key", key);
        }
        let resp = req.call().map_err(|e| http_err(path, e))?;
        read_json(path, resp)
    }

    fn post_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .agent
            .post(&url)
            .set("Accept", "application/json")
            .set("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("X-Api-Key", key);
        }
        let resp = req
            .send_json(body)
            .map_err(|e| http_err(path, e))?;
        read_json(path, resp)
    }
}

fn trim_slash(mut s: String) -> String {
    while s.ends_with('/') {
        s.pop();
    }
    s
}

fn read_body(resp: ureq::Response) -> String {
    let mut buf = String::new();
    let _ = resp.into_reader().read_to_string(&mut buf);
    buf
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &str, resp: ureq::Response) -> Result<T> {
    let status = resp.status();
    let text = read_body(resp);
    if !(200..300).contains(&status) {
        return Err(Error::Http {
            status,
            path: path.to_string(),
            body: text,
        });
    }
    serde_json::from_str(&text).map_err(|e| Error::Json {
        path: path.to_string(),
        detail: e.to_string(),
    })
}

fn http_err(path: &str, err: ureq::Error) -> Error {
    match err {
        ureq::Error::Status(status, resp) => Error::Http {
            status,
            path: path.to_string(),
            body: read_body(resp),
        },
        other => Error::Transport(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_error() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"denied","message":"this function cannot empty history"}"#,
        )
        .unwrap();
        let err = env.into_ok().unwrap_err();
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn member_json() {
        let m: Member =
            serde_json::from_str(r#"{"ship":"~zod","name":"owner","role":"owner"}"#).unwrap();
        assert_eq!(m.role, "owner");
    }
}
