//! Content-addressed blob storage: `<data-dir>/blobs/<first2>/<sha256hex>`.

use std::io::{self, SeekFrom};
use std::path::{Path, PathBuf};

use axum::Json;
use axum::body::Body;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

use crate::{SharedState, auth, unique_name};

pub(crate) const IMMUTABLE: &str = "private, max-age=31536000, immutable";
const SNIFF_LEN: usize = 32;

/// True when `s` is exactly 64 lowercase hex characters.
pub(crate) fn valid_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(crate) fn blob_path(data_dir: &Path, sha: &str) -> PathBuf {
    data_dir.join("blobs").join(&sha[..2]).join(sha)
}

/// True when `path` is a regular file and not a symlink.
pub(crate) async fn is_regular(path: &Path) -> bool {
    match tokio::fs::symlink_metadata(path).await {
        Ok(m) => m.file_type().is_file(),
        Err(_) => false,
    }
}

#[derive(Serialize)]
pub struct Stored {
    sha256: String,
    size: u64,
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    name: Option<String>,
    #[serde(rename = "type")]
    content_type: Option<String>,
}

pub(crate) fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

fn too_large() -> Response {
    (StatusCode::PAYLOAD_TOO_LARGE, "upload exceeds limit").into_response()
}

/// `PUT /blob`: stream the body to a temp file while hashing, then move it into place.
pub(crate) async fn upload(
    State(st): State<SharedState>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    if !auth::authorized(&st, &headers).await {
        return unauthorized();
    }
    let max = st.config.max_upload_bytes;
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    if declared.is_some_and(|len| len > max) {
        return too_large();
    }

    match store(&st, body).await {
        Ok((sha256, size)) => Json(Stored { sha256, size }).into_response(),
        Err(StoreError::TooLarge) => too_large(),
        Err(StoreError::Client) => (StatusCode::BAD_REQUEST, "upload aborted").into_response(),
        Err(StoreError::Io(e)) => {
            tracing::error!(error = %e, "blob upload failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

enum StoreError {
    TooLarge,
    Client,
    Io(io::Error),
}

impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> Self {
        StoreError::Io(e)
    }
}

async fn store(st: &SharedState, body: Body) -> Result<(String, u64), StoreError> {
    let tmp = st.config.data_dir.join("tmp").join(unique_name("upload"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .await?;

    let written = write_and_hash(&mut file, body, st.config.max_upload_bytes).await;
    drop(file);
    let (sha, size) = match written {
        Ok(v) => v,
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e);
        }
    };

    let dest = blob_path(&st.config.data_dir, &sha);
    if let Err(e) = commit(&tmp, &dest).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(StoreError::Io(e));
    }
    Ok((sha, size))
}

async fn write_and_hash(
    file: &mut File,
    body: Body,
    max: u64,
) -> Result<(String, u64), StoreError> {
    let mut hasher = Sha256::new();
    let mut size: u64 = 0;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StoreError::Client)?;
        size += chunk.len() as u64;
        if size > max {
            return Err(StoreError::TooLarge);
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    file.sync_all().await?;
    Ok((hex::encode(hasher.finalize()), size))
}

/// Move `tmp` to `dest`, or drop it when `dest` already exists (dedupe).
async fn commit(tmp: &Path, dest: &Path) -> io::Result<()> {
    if is_regular(dest).await {
        return tokio::fs::remove_file(tmp).await;
    }
    let parent = dest
        .parent()
        .ok_or_else(|| io::Error::other("blob path has no parent"))?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent).await?;
    tokio::fs::rename(tmp, dest).await?;
    let dir = parent.clone();
    tokio::task::spawn_blocking(move || std::fs::File::open(dir)?.sync_all())
        .await
        .map_err(io::Error::other)??;
    Ok(())
}

/// `GET /blob/{sha}`: stream the blob with Range support.
pub(crate) async fn download(
    State(st): State<SharedState>,
    headers: HeaderMap,
    AxPath(sha): AxPath<String>,
    Query(q): Query<DownloadQuery>,
) -> Response {
    if !auth::authorized(&st, &headers).await {
        return unauthorized();
    }
    if !valid_sha(&sha) {
        return (StatusCode::BAD_REQUEST, "invalid sha256").into_response();
    }
    let path = blob_path(&st.config.data_dir, &sha);
    if !is_regular(&path).await {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let etag = format!("\"{sha}\"");

    if if_none_match(&headers, &etag) {
        return with_cache_headers(
            Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, &etag),
        )
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    let mut file = match File::open(&path).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    let size = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let range = match headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
        Some(raw) => match parse_range(raw, size) {
            Ok(r) => r,
            Err(()) => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
        },
        None => None,
    };

    let mut head = [0u8; SNIFF_LEN];
    let n = file.read(&mut head).await.unwrap_or(0);
    let content_type = q
        .content_type
        .as_deref()
        .and_then(safe_content_type)
        .unwrap_or_else(|| sniff(&head[..n]).to_string());
    if file.seek(SeekFrom::Start(0)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let (status, start, len) = match range {
        Some((s, e)) => (StatusCode::PARTIAL_CONTENT, s, e - s + 1),
        None => (StatusCode::OK, 0, size),
    };
    if start > 0 && file.seek(SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, len)
        .header(header::ETAG, &etag)
        .header(header::ACCEPT_RANGES, "bytes")
        .header("x-content-type-options", "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        );
    builder = with_cache_headers(builder);
    if let Some((s, e)) = range {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {s}-{e}/{size}"));
    }
    if let Some(disp) = q.name.as_deref().and_then(content_disposition) {
        builder = builder.header(header::CONTENT_DISPOSITION, disp);
    }

    let body = Body::from_stream(ReaderStream::new(file.take(len)));
    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(crate) fn with_cache_headers(
    builder: axum::http::response::Builder,
) -> axum::http::response::Builder {
    builder.header(header::CACHE_CONTROL, IMMUTABLE)
}

fn if_none_match(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|raw| raw.split(',').map(str::trim).any(|t| t == "*" || t == etag))
}

/// Parse a single `bytes=` range into inclusive `(start, end)`.
/// `Ok(None)` means serve the whole file (malformed or multi-range input).
/// `Err(())` means the range is not satisfiable.
pub(crate) fn parse_range(raw: &str, size: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(spec) = raw.trim().strip_prefix("bytes=") else {
        return Ok(None);
    };
    if spec.contains(',') {
        return Ok(None);
    }
    let Some((first, last)) = spec.split_once('-') else {
        return Ok(None);
    };
    if first.is_empty() {
        // Suffix range: last N bytes.
        let Ok(n) = last.parse::<u64>() else {
            return Ok(None);
        };
        if n == 0 || size == 0 {
            return Err(());
        }
        let start = size.saturating_sub(n);
        return Ok(Some((start, size - 1)));
    }
    let Ok(start) = first.parse::<u64>() else {
        return Ok(None);
    };
    if start >= size {
        return Err(());
    }
    let end = if last.is_empty() {
        size - 1
    } else {
        match last.parse::<u64>() {
            Ok(e) => e.min(size - 1),
            Err(_) => return Ok(None),
        }
    };
    if end < start {
        return Err(());
    }
    Ok(Some((start, end)))
}

/// Accept only `type/subtype` with token characters, and never an active
/// document type that would run script on the gateway origin.
pub(crate) fn safe_content_type(raw: &str) -> Option<String> {
    let raw = raw.trim().to_ascii_lowercase();
    let (ty, sub) = raw.split_once('/')?;
    let token = |s: &str| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    if !token(ty) || !token(sub) {
        return None;
    }
    const ACTIVE: [&str; 9] = [
        "text/html",
        "application/xhtml+xml",
        "image/svg+xml",
        "text/xml",
        "application/xml",
        "application/javascript",
        "text/javascript",
        "application/ecmascript",
        "text/ecmascript",
    ];
    if ACTIVE.contains(&raw.as_str()) {
        return None;
    }
    Some(raw)
}

/// Sniff a few common types from magic bytes.
pub(crate) fn sniff(head: &[u8]) -> &'static str {
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        "image/gif"
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        "image/webp"
    } else if head.starts_with(b"%PDF-") {
        "application/pdf"
    } else if head.len() >= 8 && &head[4..8] == b"ftyp" {
        "video/mp4"
    } else {
        "application/octet-stream"
    }
}

/// Build a `Content-Disposition` value from an untrusted display name.
pub(crate) fn content_disposition(name: &str) -> Option<String> {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        .take(120)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        None
    } else {
        Some(format!("inline; filename=\"{clean}\""))
    }
}
