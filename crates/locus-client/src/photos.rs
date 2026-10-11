//! `%photos` catalog: albums, titles, tags and ZFS-relative paths. Photo bytes
//! never travel over this API.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Client, Error, Result, check_ok, with_revision};

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

/// Fields to change on an existing photo. `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    photo: Option<Photo>,
    photos: Option<Vec<Photo>>,
}

impl Envelope {
    fn into_photo(self, path: &str) -> Result<Photo> {
        check_ok(self.ok, self.error, self.message)?;
        self.photo.ok_or_else(|| missing(path, "photo"))
    }
}

fn missing(path: &str, what: &str) -> Error {
    Error::Decode {
        path: path.into(),
        detail: format!("envelope has no `{what}`"),
    }
}

/// Catalog paths are ZFS-relative (`holidays/pier.jpg`): no leading `/`,
/// no `..`, not empty. Returns the trimmed path.
pub fn zfs_rel_path(path: &str) -> Result<&str> {
    let p = path.trim();
    if p.is_empty() || p.starts_with('/') || p.contains("..") {
        return Err(Error::BadPath(path.into()));
    }
    Ok(p)
}

impl Client {
    pub async fn list_photos(&self) -> Result<Vec<Photo>> {
        let env: Envelope = self.get_json("/photos/~/v1/photos").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.photos.unwrap_or_default())
    }

    /// Add a catalog entry. `path` is checked locally before any request.
    pub async fn create_photo(
        &self,
        album: &str,
        title: &str,
        path: &str,
        tags: &[&str],
    ) -> Result<Photo> {
        let rel = zfs_rel_path(path)?;
        let p = "/photos/~/v1/photos";
        let env: Envelope = self
            .json(
                Method::POST,
                p,
                Some(json!({ "album": album, "title": title, "path": rel, "tags": tags })),
            )
            .await?;
        env.into_photo(p)
    }

    /// Patch a photo. Stale `expected_revision` returns [`Error::Conflict`].
    pub async fn patch_photo(
        &self,
        id: u64,
        expected_revision: u64,
        patch: &PhotoPatch,
    ) -> Result<Photo> {
        if let Some(path) = &patch.path {
            zfs_rel_path(path)?;
        }
        let p = format!("/photos/~/v1/photos/{id}");
        let mut body = serde_json::to_value(patch).map_err(|e| Error::Decode {
            path: p.clone(),
            detail: e.to_string(),
        })?;
        body["expectedRevision"] = expected_revision.into();
        let env: Envelope = self.json(Method::PUT, &p, Some(body)).await?;
        env.into_photo(&p)
    }

    /// Delete a photo. `expected_revision` is optional; when set and stale the
    /// server answers 409 ([`Error::Conflict`]). A missing photo is
    /// [`Error::NotFound`].
    pub async fn delete_photo(&self, id: u64, expected_revision: Option<u64>) -> Result<()> {
        let p = with_revision(format!("/photos/~/v1/photos/{id}"), expected_revision);
        let env: Envelope = self.json(Method::DELETE, &p, None).await?;
        check_ok(env.ok, env.error, env.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_envelope_decodes() {
        let raw = r#"{"ok":true,"photo":{"id":1,"album":"holidays","title":"pier","path":"holidays/pier.jpg","tags":["sea","2026"],"revision":2}}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        let p = env.into_photo("/p").unwrap();
        assert_eq!(p.path, "holidays/pier.jpg");
        assert_eq!(p.tags, ["sea", "2026"]);
        assert_eq!(p.revision, 2);
    }

    #[test]
    fn list_envelope_decodes_without_tags_or_revision() {
        let raw = r#"{"ok":true,"photos":[{"id":3,"album":"a","title":"t","path":"a/t.png"}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        let photos = env.photos.unwrap();
        assert_eq!(photos.len(), 1);
        assert!(photos[0].tags.is_empty());
        assert_eq!(photos[0].revision, 0);
    }

    #[test]
    fn conflict_envelope_maps_to_conflict() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"conflict","message":"stale expectedRevision"}"#,
        )
        .unwrap();
        assert!(matches!(
            env.into_photo("/p"),
            Err(Error::Conflict { current: None })
        ));
    }

    #[test]
    fn api_error_envelope_keeps_type() {
        let env: Envelope =
            serde_json::from_str(r#"{"ok":false,"error":"not-found","message":"no such photo"}"#)
                .unwrap();
        match env.into_photo("/p") {
            Err(Error::Api { error, message }) => {
                assert_eq!(error, "not-found");
                assert_eq!(message, "no such photo");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn zfs_paths_stay_under_dataset() {
        assert_eq!(
            zfs_rel_path(" holidays/pier.jpg ").unwrap(),
            "holidays/pier.jpg"
        );
        assert!(matches!(
            zfs_rel_path("../etc/passwd"),
            Err(Error::BadPath(_))
        ));
        assert!(matches!(
            zfs_rel_path("/etc/passwd"),
            Err(Error::BadPath(_))
        ));
        assert!(matches!(zfs_rel_path("  "), Err(Error::BadPath(_))));
    }

    #[test]
    fn patch_serializes_only_set_fields() {
        let patch = PhotoPatch {
            title: Some("new".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            json!({ "title": "new" })
        );
    }
}
