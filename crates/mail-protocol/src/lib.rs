//! HTTP client for `%mail` and `%calendar` (P2 Gall stores on desk `%locus`).
//!
//! The ship is the store. A host SMTP/IMAP process may ingest and send;
//! it must not keep a second mailbox database.

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
    #[error("conflict: stale expectedRevision")]
    Conflict,
    #[error("{0}")]
    Transport(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    pub folder: String,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub calendar: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Envelope {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default, rename = "msg")]
    pub msg: Option<Message>,
    #[serde(default)]
    pub messages: Option<Vec<Message>>,
    #[serde(default)]
    pub event: Option<Event>,
    #[serde(default)]
    pub events: Option<Vec<Event>>,
}

impl Envelope {
    pub fn into_ok(self) -> Result<Self> {
        if self.ok {
            return Ok(self);
        }
        if self.error.as_deref() == Some("conflict") {
            return Err(Error::Conflict);
        }
        Err(Error::Api {
            typ: self.error.unwrap_or_else(|| "error".into()),
            message: self.message.unwrap_or_default(),
        })
    }
}

#[derive(Clone)]
pub struct Client {
    agent: Agent,
    base: String,
    api_key: Option<String>,
    scry_app: &'static str,
}

impl Client {
    pub fn mail(base: impl Into<String>) -> Self {
        Self::new(base, "mail")
    }

    pub fn calendar(base: impl Into<String>) -> Self {
        Self::new(base, "calendar")
    }

    pub fn with_api_key(base: impl Into<String>, api_key: impl Into<String>, scry_app: &'static str) -> Self {
        let mut c = Self::new(base, scry_app);
        c.api_key = Some(api_key.into());
        c
    }

    pub fn mail_with_key(base: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self::with_api_key(base, api_key, "mail")
    }

    fn new(base: impl Into<String>, scry_app: &'static str) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(60))
            .build();
        Self {
            agent,
            base: trim_slash(base.into()),
            api_key: None,
            scry_app,
        }
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
        let path = format!("/~/scry/{}/v0/api-key.json", self.scry_app);
        let value: serde_json::Value = self.get_json(&path)?;
        match value {
            serde_json::Value::String(s) if !s.is_empty() => Ok(s),
            _ => Err(Error::Api {
                typ: "api-key".into(),
                message: "empty".into(),
            }),
        }
    }

    pub fn list_messages(&self, folder: Option<&str>) -> Result<Vec<Message>> {
        let path = match folder {
            Some(f) => format!("/mail/~/v1/messages?folder={f}"),
            None => "/mail/~/v1/messages".into(),
        };
        let env: Envelope = self.get_json(&path)?;
        Ok(env.into_ok()?.messages.unwrap_or_default())
    }

    pub fn create_draft(&self, subject: &str, body: &str) -> Result<Message> {
        let env: Envelope = self.post_json(
            "/mail/~/v1/messages",
            serde_json::json!({ "subject": subject, "body": body }),
        )?;
        env.into_ok()?
            .msg
            .ok_or_else(|| Error::Api {
                typ: "missing".into(),
                message: "message".into(),
            })
    }

    pub fn patch_message(&self, id: u64, expected_revision: u64, body: &str) -> Result<Message> {
        let env: Envelope = self.put_json(
            &format!("/mail/~/v1/messages/{id}"),
            serde_json::json!({
                "expectedRevision": expected_revision,
                "body": body,
            }),
        )?;
        env.into_ok()?
            .msg
            .ok_or_else(|| Error::Api {
                typ: "missing".into(),
                message: "message".into(),
            })
    }

    pub fn ingest(&self, from: &str, to: &[&str], subject: &str, body: &str) -> Result<Message> {
        let env: Envelope = self.post_json(
            "/mail/~/v1/ingest",
            serde_json::json!({
                "from": from,
                "to": to,
                "subject": subject,
                "body": body,
            }),
        )?;
        env.into_ok()?
            .msg
            .ok_or_else(|| Error::Api {
                typ: "missing".into(),
                message: "message".into(),
            })
    }

    pub fn send(&self, id: u64) -> Result<Message> {
        let env: Envelope = self.post_json(
            &format!("/mail/~/v1/messages/{id}/send"),
            serde_json::json!({}),
        )?;
        env.into_ok()?
            .msg
            .ok_or_else(|| Error::Api {
                typ: "missing".into(),
                message: "message".into(),
            })
    }

    pub fn list_events(&self) -> Result<Vec<Event>> {
        let env: Envelope = self.get_json("/calendar/~/v1/events")?;
        Ok(env.into_ok()?.events.unwrap_or_default())
    }

    pub fn create_event(&self, title: &str, start: u64, end: u64) -> Result<Event> {
        let env: Envelope = self.post_json(
            "/calendar/~/v1/events",
            serde_json::json!({ "title": title, "start": start, "end": end }),
        )?;
        env.into_ok()?.event.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "event".into(),
        })
    }

    pub fn patch_event(&self, id: u64, expected_revision: u64, title: &str) -> Result<Event> {
        let env: Envelope = self.put_json(
            &format!("/calendar/~/v1/events/{id}"),
            serde_json::json!({
                "expectedRevision": expected_revision,
                "title": title,
            }),
        )?;
        env.into_ok()?.event.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "event".into(),
        })
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
        self.send_json("POST", path, body)
    }

    fn put_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T> {
        self.send_json("PUT", path, body)
    }

    fn send_json<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .agent
            .request(method, &url)
            .set("Accept", "application/json")
            .set("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("X-Api-Key", key);
        }
        let resp = req.send_json(body).map_err(|e| http_err(path, e))?;
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
    fn conflict_envelope() {
        let env: Envelope =
            serde_json::from_str(r#"{"ok":false,"error":"conflict","message":"stale"}"#).unwrap();
        assert!(matches!(env.into_ok(), Err(Error::Conflict)));
    }
}
