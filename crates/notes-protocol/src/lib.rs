//! HTTP client for `%notes` on Clay desk `%locus` (not Tlon Messenger).
//!
//! Talks to Eyre at a base URL (localhost VM: `http://127.0.0.1:8080`).
//! Login is `POST /~/login` with the ship's web code. Writes that return
//! `pending` are polled at `/notes/~/v1/request/{id}`.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::Duration;
use thiserror::Error;
use ureq::Agent;

const PENDING_POLLS: u32 = 30;

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
    #[error("{0}")]
    Api(String),
    #[error("conflict: someone else saved this note first")]
    Conflict,
    #[error("still pending after poll: {0}")]
    Pending(String),
    #[error("{0}")]
    Transport(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookSummary {
    pub host: String,
    #[serde(rename = "flagName")]
    pub flag_name: String,
    pub notebook: Notebook,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: u64,
    pub title: String,
    #[serde(rename = "rootFolderId")]
    pub root_folder_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: u64,
    pub title: String,
    #[serde(rename = "bodyMd", default)]
    pub body_md: String,
    pub revision: u64,
    #[serde(rename = "folderId", default)]
    pub folder_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct Envelope {
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    body: EnvelopeBody,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum EnvelopeBody {
    Ok {
        #[serde(default)]
        response: serde_json::Value,
    },
    #[serde(rename = "no-change")]
    NoChange,
    Notebook {
        notebook: NotebookSummary,
    },
    #[serde(rename = "api-key")]
    ApiKey {
        #[serde(rename = "apiKey")]
        api_key: Option<String>,
    },
    Error {
        #[serde(rename = "errorType")]
        error_type: String,
        #[serde(default)]
        message: Vec<String>,
    },
    Pending {
        #[serde(default)]
        status: serde_json::Value,
    },
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

    /// `POST /~/login` with the ship's web code. Cookie stays on this client.
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
        if let Ok(key) = self.fetch_api_key() {
            self.api_key = Some(key);
        }
        Ok(())
    }

    fn fetch_api_key(&self) -> Result<String> {
        // Local-only Gall scry, after a session cookie is set.
        let path = "/~/scry/notes/x/v0/api-key.json";
        let value: serde_json::Value = self.get_json(path)?;
        match value {
            serde_json::Value::String(s) if !s.is_empty() => Ok(s),
            _ => Err(Error::Api("api-key scry was empty".into())),
        }
    }

    pub fn list_notebooks(&self) -> Result<Vec<NotebookSummary>> {
        self.get_json("/notes/~/v1/notebooks")
    }

    pub fn create_notebook(&self, title: &str) -> Result<NotebookSummary> {
        let env = self.write(
            "POST",
            "/notes/~/v1/notebooks",
            Some(serde_json::json!({ "title": title })),
        )?;
        match env.body {
            EnvelopeBody::Notebook { notebook } => Ok(notebook),
            other => Err(Error::Api(format!("create notebook: {other:?}"))),
        }
    }

    pub fn list_folders(&self, host: &str, name: &str) -> Result<Vec<Folder>> {
        self.get_json(&format!("/notes/~/v1/notebooks/{host}/{name}/folders"))
    }

    pub fn list_notes(&self, host: &str, name: &str) -> Result<Vec<Note>> {
        self.get_json(&format!("/notes/~/v1/notebooks/{host}/{name}/notes"))
    }

    pub fn get_note(&self, host: &str, name: &str, id: u64) -> Result<Note> {
        self.get_json(&format!(
            "/notes/~/v1/notebooks/{host}/{name}/notes/{id}"
        ))
    }

    pub fn create_note(
        &self,
        host: &str,
        name: &str,
        folder: u64,
        title: &str,
        body: &str,
    ) -> Result<Note> {
        let path = format!("/notes/~/v1/notebooks/{host}/{name}/notes");
        self.write(
            "POST",
            &path,
            Some(serde_json::json!({
                "folder": folder,
                "title": title,
                "body": body,
            })),
        )?;
        let notes = self.list_notes(host, name)?;
        notes
            .into_iter()
            .rev()
            .find(|n| n.title == title)
            .ok_or_else(|| Error::Api("created note missing from list".into()))
    }

    /// Content edit. The ship ignores `title` when `body` is present, so
    /// this sends body only. Use [`Self::rename_note`] to change the title.
    pub fn save_note(
        &self,
        host: &str,
        name: &str,
        id: u64,
        body: &str,
        expected_revision: u64,
    ) -> Result<()> {
        let path = format!("/notes/~/v1/notebooks/{host}/{name}/notes/{id}");
        self.put_note(
            &path,
            serde_json::json!({
                "body": body,
                "expectedRevision": expected_revision,
            }),
        )
    }

    /// Metadata edit. Must not include `body`, or the ship treats it as a
    /// content edit and drops the title.
    pub fn rename_note(&self, host: &str, name: &str, id: u64, title: &str) -> Result<()> {
        let path = format!("/notes/~/v1/notebooks/{host}/{name}/notes/{id}");
        self.put_note(&path, serde_json::json!({ "title": title }))
    }

    pub fn delete_note(&self, host: &str, name: &str, id: u64) -> Result<()> {
        let path = format!("/notes/~/v1/notebooks/{host}/{name}/notes/{id}");
        let env = self.write("DELETE", &path, None)?;
        match env.body {
            EnvelopeBody::Error {
                error_type,
                message,
            } => Err(Error::Api(format!("{error_type}: {}", message.join(" ")))),
            EnvelopeBody::Pending { .. } => Err(Error::Pending(path)),
            _ => Ok(()),
        }
    }

    fn put_note(&self, path: &str, body: serde_json::Value) -> Result<()> {
        let env = self.write("PUT", path, Some(body))?;
        match env.body {
            EnvelopeBody::Error { error_type, .. } if error_type == "conflict" => {
                Err(Error::Conflict)
            }
            EnvelopeBody::Error {
                error_type,
                message,
            } => Err(Error::Api(format!("{error_type}: {}", message.join(" ")))),
            EnvelopeBody::Pending { .. } => Err(Error::Pending(path.to_string())),
            _ => Ok(()),
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base);
        let mut req = self.agent.get(&url).set("Accept", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("X-Api-Key", key);
        }
        let resp = req.call().map_err(|e| http_err(path, e))?;
        let status = resp.status();
        let text = read_body(resp);
        if status != 200 {
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

    fn write(&self, method: &str, path: &str, body: Option<serde_json::Value>) -> Result<Envelope> {
        let mut env = self.write_once(method, path, body.as_ref())?;
        if matches!(env.body, EnvelopeBody::Pending { .. }) {
            let rid = env
                .request_id
                .clone()
                .ok_or_else(|| Error::Pending("pending with no requestId".into()))?;
            env = self.poll_request(&rid)?;
        }
        match &env.body {
            EnvelopeBody::Error { error_type, .. } if error_type == "conflict" => {
                Err(Error::Conflict)
            }
            EnvelopeBody::Error {
                error_type,
                message,
            } => Err(Error::Api(format!("{error_type}: {}", message.join(" ")))),
            EnvelopeBody::Pending { .. } => Err(Error::Pending(path.into())),
            _ => Ok(env),
        }
    }

    fn write_once(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<Envelope> {
        let url = format!("{}{path}", self.base);
        let mut req = self
            .agent
            .request(method, &url)
            .set("Accept", "application/json");
        if let Some(key) = &self.api_key {
            req = req.set("X-Api-Key", key);
        }
        let resp = if let Some(body) = body {
            req.set("Content-Type", "application/json")
                .send_string(&body.to_string())
        } else {
            req.call()
        }
        .map_err(|e| http_err(path, e))?;
        let status = resp.status();
        let text = read_body(resp);
        if status != 200 {
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

    fn poll_request(&self, request_id: &str) -> Result<Envelope> {
        let path = format!("/notes/~/v1/request/{request_id}");
        for _ in 0..PENDING_POLLS {
            std::thread::sleep(Duration::from_secs(1));
            let env: Envelope = self.get_json(&path)?;
            if !matches!(env.body, EnvelopeBody::Pending { .. }) {
                return Ok(env);
            }
        }
        Err(Error::Pending(request_id.into()))
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
    fn note_json_roundtrip() {
        let raw = r#"{"id":4,"notebookId":1,"folderId":2,"title":"M3 Note","slug":null,"bodyMd":"first","createdBy":"~zod","createdAt":1,"updatedBy":"~zod","updatedAt":1,"revision":0}"#;
        let note: Note = serde_json::from_str(raw).unwrap();
        assert_eq!(note.id, 4);
        assert_eq!(note.body_md, "first");
        assert_eq!(note.revision, 0);
    }

    #[test]
    fn pending_and_conflict_envelopes() {
        let pending: Envelope = serde_json::from_str(
            r#"{"requestId":"0v1","body":{"type":"pending","status":{}}}"#,
        )
        .unwrap();
        assert!(matches!(pending.body, EnvelopeBody::Pending { .. }));

        let conflict: Envelope = serde_json::from_str(
            r#"{"requestId":"0v1","body":{"type":"error","errorType":"conflict","message":[]}}"#,
        )
        .unwrap();
        assert!(matches!(
            conflict.body,
            EnvelopeBody::Error { error_type, .. } if error_type == "conflict"
        ));
    }

    #[test]
    fn live_vm_create_save_conflict() {
        let url = match std::env::var("NOTES_LIVE") {
            Ok(u) if !u.is_empty() && u != "0" => {
                if u == "1" {
                    "http://127.0.0.1:8080".into()
                } else {
                    u
                }
            }
            _ => {
                eprintln!("skip live test (set NOTES_LIVE=1)");
                return;
            }
        };
        let code = std::env::var("NOTES_CODE").unwrap_or_default();
        if code.is_empty() {
            eprintln!("skip live test (set NOTES_CODE)");
            return;
        }
        let mut c = Client::new(url);
        c.login(&code).expect("login");
        let books = c.list_notebooks().expect("list");
        let book = if books.is_empty() {
            c.create_notebook("Notes").expect("create book")
        } else {
            books.into_iter().next().unwrap()
        };
        let title = format!("live-{}", std::process::id());
        let note = c
            .create_note(
                &book.host,
                &book.flag_name,
                book.notebook.root_folder_id,
                &title,
                "hello",
            )
            .expect("create note");
        c.save_note(
            &book.host,
            &book.flag_name,
            note.id,
            "hello2",
            note.revision,
        )
        .expect("save");
        let err = c
            .save_note(
                &book.host,
                &book.flag_name,
                note.id,
                "stale",
                note.revision,
            )
            .unwrap_err();
        assert!(matches!(err, Error::Conflict), "{err}");
    }
}
