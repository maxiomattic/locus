//! Byte store of the host gateway (`locus-gateway`), on the same origin as the
//! ship API. Bytes are content-addressed by SHA-256; the catalogs
//! ([`crate::files`], [`crate::photos`]) store only the hash.
//!
//! Requests carry the same credentials as the ship API (session cookie or
//! `X-Api-Key`). An upload is checked locally: the SHA-256 and size the gateway
//! reports must match the bytes that were sent.

use reqwest::{Method, header};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::files::FileEntry;
use crate::photos::Photo;
use crate::{Client, Error, Result, seg, send_bytes, send_text};

/// The gateway's answer to `PUT /blob`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    pub sha256: String,
    pub size: u64,
}

/// Photo paths that name a gateway blob look like `sha256:<hex>`.
const PHOTO_BLOB_PREFIX: &str = "sha256:";

/// The hash of a photo's bytes when its catalog path names a gateway blob.
/// `None` for ZFS-relative paths.
pub fn photo_blob(photo: &Photo) -> Option<&str> {
    photo.path.strip_prefix(PHOTO_BLOB_PREFIX)
}

/// Lowercase hex SHA-256 of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

impl Client {
    /// Store `bytes` on the gateway. The reply's SHA-256 and size must equal the
    /// local hash and length, otherwise this returns [`Error::Decode`].
    pub async fn upload_blob(&self, bytes: Vec<u8>) -> Result<BlobRef> {
        const PATH: &str = "/blob";
        let expected = BlobRef {
            sha256: sha256_hex(&bytes),
            size: bytes.len() as u64,
        };
        let rb = self
            .request(Method::PUT, PATH)?
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes);
        let text = send_text(rb, PATH).await?;
        let got: BlobRef = serde_json::from_str(&text).map_err(|e| Error::Decode {
            path: PATH.into(),
            detail: e.to_string(),
        })?;
        if got != expected {
            return Err(Error::Decode {
                path: PATH.into(),
                detail: format!(
                    "gateway stored sha256 {} size {}, but sent sha256 {} size {}",
                    got.sha256, got.size, expected.sha256, expected.size
                ),
            });
        }
        Ok(got)
    }

    /// `GET /blob/{sha256}`, with `name` as the download file name when given.
    pub fn blob_url(&self, sha256: &str, name: Option<&str>) -> Result<Url> {
        let mut url = self.endpoint(&format!("/blob/{}", seg(sha256)))?;
        if let Some(name) = name {
            url.query_pairs_mut().append_pair("name", name);
        }
        Ok(url)
    }

    /// `GET /thumb/{sha256}?size=N`. The gateway clamps `size`.
    pub fn thumb_url(&self, sha256: &str, size: u32) -> Result<Url> {
        let mut url = self.endpoint(&format!("/thumb/{}", seg(sha256)))?;
        url.query_pairs_mut().append_pair("size", &size.to_string());
        Ok(url)
    }

    pub async fn download_blob(&self, sha256: &str) -> Result<Vec<u8>> {
        let url = self.blob_url(sha256, None)?;
        self.get_bytes(url).await
    }

    pub async fn download_thumb(&self, sha256: &str, size: u32) -> Result<Vec<u8>> {
        let url = self.thumb_url(sha256, size)?;
        self.get_bytes(url).await
    }

    async fn get_bytes(&self, url: Url) -> Result<Vec<u8>> {
        let path = url.path().to_string();
        send_bytes(self.with_auth(self.http.get(url)), &path).await
    }

    /// Upload `bytes`, then add a `%files` entry for them.
    pub async fn upload_file(
        &self,
        dir: &str,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> Result<FileEntry> {
        let blob = self.upload_blob(bytes).await?;
        self.create_file(dir, name, &blob.sha256, blob.size, mime)
            .await
    }

    /// Upload `bytes`, then add a `%photos` entry whose path is `sha256:<hex>`.
    pub async fn upload_photo(&self, album: &str, title: &str, bytes: Vec<u8>) -> Result<Photo> {
        let blob = self.upload_blob(bytes).await?;
        let path = format!("{PHOTO_BLOB_PREFIX}{}", blob.sha256);
        self.create_photo(album, title, &path, &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(path: &str) -> Photo {
        Photo {
            id: 1,
            album: "a".into(),
            title: "t".into(),
            path: path.into(),
            tags: vec![],
            revision: 0,
        }
    }

    #[test]
    fn photo_blob_reads_hash_prefix() {
        let sha = sha256_hex(b"pixels");
        assert_eq!(
            photo_blob(&photo(&format!("sha256:{sha}"))),
            Some(sha.as_str())
        );
        assert_eq!(photo_blob(&photo("holidays/pier.jpg")), None);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn blob_ref_decodes_gateway_reply() {
        let raw = r#"{"sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","size":3}"#;
        let b: BlobRef = serde_json::from_str(raw).unwrap();
        assert_eq!(b.size, 3);
        assert_eq!(b.sha256, sha256_hex(b"abc"));
    }

    #[test]
    fn urls_carry_name_and_size_queries() {
        let c = Client::new(Url::parse("http://127.0.0.1:8080").unwrap()).unwrap();
        let sha = sha256_hex(b"abc");
        assert_eq!(
            c.blob_url(&sha, None).unwrap().as_str(),
            format!("http://127.0.0.1:8080/blob/{sha}")
        );
        assert_eq!(
            c.blob_url(&sha, Some("a b.jpg")).unwrap().as_str(),
            format!("http://127.0.0.1:8080/blob/{sha}?name=a+b.jpg")
        );
        assert_eq!(
            c.thumb_url(&sha, 256).unwrap().as_str(),
            format!("http://127.0.0.1:8080/thumb/{sha}?size=256")
        );
    }
}
