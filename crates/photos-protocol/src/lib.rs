//! Catalog types and HTTP client for `%photos` and `%contacts` .
//!
//! Photo **bytes** stay on ZFS (`/var/lib/locus/photos`). This crate is the
//! ship catalog (albums, tags, relative paths) plus contacts.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::Agent;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("conflict: stale expectedRevision")]
    Conflict,
    #[error("{typ}: {message}")]
    Api { typ: String, message: String },
    #[error("path must be relative with no .. (got {0})")]
    BadPath(String),
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
    Transport(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Photo {
    pub id: u64,
    pub album: String,
    pub title: String,
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub ship: String,
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
    #[serde(default)]
    pub photo: Option<Photo>,
    #[serde(default)]
    pub photos: Option<Vec<Photo>>,
    #[serde(default)]
    pub contact: Option<Contact>,
    #[serde(default)]
    pub contacts: Option<Vec<Contact>>,
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

/// Catalog paths are ZFS-relative (`holidays/pier.jpg`), never `/...` or `..`.
pub fn zfs_rel_path(path: &str) -> Result<&str> {
    let p = path.trim();
    if p.is_empty() || p.starts_with('/') || p.contains("..") {
        return Err(Error::BadPath(path.into()));
    }
    Ok(p)
}

/// Join under the photos dataset. Rejects escapes.
pub fn bytes_path(dataset: &Path, rel: &str) -> Result<PathBuf> {
    let rel = zfs_rel_path(rel)?;
    Ok(dataset.join(rel))
}

#[derive(Clone)]
pub struct Client {
    agent: Agent,
    base: String,
    api_key: Option<String>,
    scry_app: &'static str,
}

impl Client {
    pub fn photos(base: impl Into<String>) -> Self {
        Self::new(base, "photos")
    }

    pub fn contacts(base: impl Into<String>) -> Self {
        Self::new(base, "contacts")
    }

    pub fn photos_with_key(base: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut c = Self::photos(base);
        c.api_key = Some(api_key.into());
        c
    }

    pub fn contacts_with_key(base: impl Into<String>, api_key: impl Into<String>) -> Self {
        let mut c = Self::contacts(base);
        c.api_key = Some(api_key.into());
        c
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

    pub fn list_photos(&self) -> Result<Vec<Photo>> {
        let env: Envelope = self.get_json("/photos/~/v1/photos")?;
        Ok(env.into_ok()?.photos.unwrap_or_default())
    }

    pub fn create_photo(
        &self,
        album: &str,
        title: &str,
        path: &str,
        tags: &[&str],
    ) -> Result<Photo> {
        let rel = zfs_rel_path(path)?;
        let env: Envelope = self.post_json(
            "/photos/~/v1/photos",
            serde_json::json!({
                "album": album,
                "title": title,
                "path": rel,
                "tags": tags,
            }),
        )?;
        env.into_ok()?.photo.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "photo".into(),
        })
    }

    pub fn patch_photo(
        &self,
        id: u64,
        expected_revision: u64,
        title: &str,
    ) -> Result<Photo> {
        let env: Envelope = self.put_json(
            &format!("/photos/~/v1/photos/{id}"),
            serde_json::json!({
                "expectedRevision": expected_revision,
                "title": title,
            }),
        )?;
        env.into_ok()?.photo.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "photo".into(),
        })
    }

    pub fn list_contacts(&self) -> Result<Vec<Contact>> {
        let env: Envelope = self.get_json("/contacts/~/v1/contacts")?;
        Ok(env.into_ok()?.contacts.unwrap_or_default())
    }

    pub fn create_contact(&self, name: &str, email: &str, ship: &str) -> Result<Contact> {
        let env: Envelope = self.post_json(
            "/contacts/~/v1/contacts",
            serde_json::json!({
                "name": name,
                "email": email,
                "ship": ship,
            }),
        )?;
        env.into_ok()?.contact.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "contact".into(),
        })
    }

    pub fn patch_contact(
        &self,
        id: u64,
        expected_revision: u64,
        name: &str,
    ) -> Result<Contact> {
        let env: Envelope = self.put_json(
            &format!("/contacts/~/v1/contacts/{id}"),
            serde_json::json!({
                "expectedRevision": expected_revision,
                "name": name,
            }),
        )?;
        env.into_ok()?.contact.ok_or_else(|| Error::Api {
            typ: "missing".into(),
            message: "contact".into(),
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

    #[test]
    fn ok_photo_roundtrip() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":true,"photo":{"id":1,"album":"a","title":"t","path":"a/t.jpg","tags":["x"],"revision":0}}"#,
        )
        .unwrap();
        let p = env.into_ok().unwrap().photo.unwrap();
        assert_eq!(p.path, "a/t.jpg");
        assert_eq!(p.tags, ["x"]);
    }

    #[test]
    fn reject_dotdot_and_absolute() {
        assert!(zfs_rel_path("../etc/passwd").is_err());
        assert!(zfs_rel_path("/etc/passwd").is_err());
        assert!(zfs_rel_path("holidays/pier.jpg").is_ok());
    }

    #[test]
    fn bytes_stay_under_dataset() {
        let root = Path::new("/var/lib/locus/photos");
        let p = bytes_path(root, "holidays/pier.jpg").unwrap();
        assert_eq!(p, root.join("holidays/pier.jpg"));
        assert!(bytes_path(root, "../x").is_err());
    }

    #[test]
    fn create_photo_rejects_escape_before_http() {
        let c = Client::photos("http://127.0.0.1");
        assert!(matches!(
            c.create_photo("a", "t", "../etc/passwd", &[]),
            Err(Error::BadPath(_))
        ));
    }

    #[test]
    fn expected_revision_field_is_camel_case() {
        let v = serde_json::json!({ "expectedRevision": 3, "title": "ok" });
        assert_eq!(v["expectedRevision"], 3);
    }
}
