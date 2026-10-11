//! `%files` catalog: entries (`dir`, `name`, SHA-256 address, size, mime) and
//! explicit folders. Bytes stay on ZFS behind the host gateway, see
//! [`crate::blobs`].
//!
//! Errors come back as `{ok: false, error, message}` with HTTP 400 (validation),
//! 404 (missing) or 409 (stale `expectedRevision`). 409 maps to
//! [`Error::Conflict`] and 404 to [`Error::NotFound`]; 400 stays
//! [`Error::Http`] with the envelope as its body.
//!
//! `created` and `modified` are Unix seconds, like `%calendar`.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::form_urlencoded;

use crate::{Client, Error, Result, check_ok, with_revision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub id: u64,
    /// Absolute folder path, e.g. `/holidays/2026`.
    pub dir: String,
    pub name: String,
    /// Lowercase hex SHA-256 of the bytes on the gateway.
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    /// Unix seconds.
    pub created: u64,
    /// Unix seconds.
    pub modified: u64,
    pub revision: u64,
}

/// Fields to change on an existing entry. `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct FilePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    file: Option<FileEntry>,
    files: Option<Vec<FileEntry>>,
    dirs: Option<Vec<String>>,
}

impl Envelope {
    fn into_file(self, path: &str) -> Result<FileEntry> {
        check_ok(self.ok, self.error, self.message)?;
        self.file.ok_or_else(|| Error::Decode {
            path: path.into(),
            detail: "envelope has no `file`".into(),
        })
    }
}

impl Client {
    /// List entries. With `dir`, only entries whose folder is exactly that path.
    pub async fn list_files(&self, dir: Option<&str>) -> Result<Vec<FileEntry>> {
        let mut path = "/files/~/v1/files".to_string();
        if let Some(dir) = dir {
            let q = form_urlencoded::Serializer::new(String::new())
                .append_pair("dir", dir)
                .finish();
            path = format!("{path}?{q}");
        }
        let env: Envelope = self.get_json(&path).await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.files.unwrap_or_default())
    }

    pub async fn get_file(&self, id: u64) -> Result<FileEntry> {
        let p = format!("/files/~/v1/files/{id}");
        let env: Envelope = self.get_json(&p).await?;
        env.into_file(&p)
    }

    /// Add an entry for bytes already on the gateway. `sha256` and `size` must
    /// describe those bytes; see [`Client::upload_file`] to do both steps.
    pub async fn create_file(
        &self,
        dir: &str,
        name: &str,
        sha256: &str,
        size: u64,
        mime: &str,
    ) -> Result<FileEntry> {
        let p = "/files/~/v1/files";
        let env: Envelope = self
            .json(
                Method::POST,
                p,
                Some(json!({
                    "dir": dir,
                    "name": name,
                    "sha256": sha256,
                    "size": size,
                    "mime": mime,
                })),
            )
            .await?;
        env.into_file(p)
    }

    /// Rename or move an entry. Stale `expected_revision` returns [`Error::Conflict`].
    pub async fn patch_file(
        &self,
        id: u64,
        expected_revision: u64,
        patch: &FilePatch,
    ) -> Result<FileEntry> {
        let p = format!("/files/~/v1/files/{id}");
        let mut body = serde_json::to_value(patch).map_err(|e| Error::Decode {
            path: p.clone(),
            detail: e.to_string(),
        })?;
        body["expectedRevision"] = expected_revision.into();
        let env: Envelope = self.json(Method::PUT, &p, Some(body)).await?;
        env.into_file(&p)
    }

    /// Delete an entry. The revision check is optional; when given and stale,
    /// returns [`Error::Conflict`].
    pub async fn delete_file(&self, id: u64, expected_revision: Option<u64>) -> Result<()> {
        let p = with_revision(format!("/files/~/v1/files/{id}"), expected_revision);
        let env: Envelope = self.json(Method::DELETE, &p, None).await?;
        check_ok(env.ok, env.error, env.message)
    }

    /// Sorted folder paths: explicit folders plus those implied by entries.
    /// `/` is always present.
    pub async fn list_dirs(&self) -> Result<Vec<String>> {
        let env: Envelope = self.get_json("/files/~/v1/dirs").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.dirs.unwrap_or_default())
    }

    /// Create an empty folder. Creating an existing folder is not an error.
    pub async fn create_dir(&self, path: &str) -> Result<()> {
        let p = "/files/~/v1/dirs";
        let env: Envelope = self
            .json(Method::POST, p, Some(json!({ "path": path })))
            .await?;
        check_ok(env.ok, env.error, env.message)
    }

    /// Delete a folder. Returns [`Error::Conflict`] while any entry is under it.
    pub async fn delete_dir(&self, path: &str) -> Result<()> {
        let q = form_urlencoded::Serializer::new(String::new())
            .append_pair("path", path)
            .finish();
        let env: Envelope = self
            .json(Method::DELETE, &format!("/files/~/v1/dirs?{q}"), None)
            .await?;
        check_ok(env.ok, env.error, env.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn file_entry_decodes_encoder_shape() {
        // Shape of `file:enjs:files-json`: unix-second dates, flat keys.
        let raw = format!(
            r#"{{"ok":true,"file":{{"id":12,"dir":"/holidays/2026","name":"pier.jpg","sha256":"{SHA}","size":48213,"mime":"image/jpeg","created":1767225600,"modified":1767312000,"revision":2}}}}"#
        );
        let env: Envelope = serde_json::from_str(&raw).unwrap();
        let f = env.into_file("/p").unwrap();
        assert_eq!(f.id, 12);
        assert_eq!(f.dir, "/holidays/2026");
        assert_eq!(f.sha256, SHA);
        assert_eq!(f.size, 48_213);
        assert_eq!(f.created, 1_767_225_600);
        assert_eq!(f.modified - f.created, 86_400);
        assert_eq!(f.revision, 2);
    }

    #[test]
    fn list_envelope_decodes() {
        let raw = format!(
            r#"{{"ok":true,"files":[{{"id":1,"dir":"/","name":"a.txt","sha256":"{SHA}","size":0,"mime":"text/plain","created":1,"modified":1,"revision":0}}]}}"#
        );
        let env: Envelope = serde_json::from_str(&raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        let files = env.files.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "a.txt");
    }

    #[test]
    fn empty_list_envelope_decodes() {
        let env: Envelope = serde_json::from_str(r#"{"ok":true,"files":[]}"#).unwrap();
        assert!(env.files.unwrap().is_empty());
    }

    #[test]
    fn dirs_envelope_decodes_sorted_paths() {
        let raw = r#"{"ok":true,"dirs":["/","/holidays","/holidays/2026"]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        assert_eq!(env.dirs.unwrap(), ["/", "/holidays", "/holidays/2026"]);
    }

    #[test]
    fn conflict_envelope_maps_to_conflict() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"conflict","message":"stale expectedRevision"}"#,
        )
        .unwrap();
        assert!(matches!(
            env.into_file("/p"),
            Err(Error::Conflict { current: None })
        ));
    }

    #[test]
    fn bad_request_envelope_keeps_type() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"bad-request","message":"name must be non-empty and contain no /"}"#,
        )
        .unwrap();
        match env.into_file("/p") {
            Err(Error::Api { error, message }) => {
                assert_eq!(error, "bad-request");
                assert!(message.contains("no /"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn patch_serializes_only_set_fields() {
        let patch = FilePatch {
            name: Some("new.jpg".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            json!({ "name": "new.jpg" })
        );
    }
}
