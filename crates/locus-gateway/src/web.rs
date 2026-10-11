//! `GET /app/*`: static files from `--web-root`, with an SPA fallback to index.html.
//! Public: the app bundle contains no data.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use tokio_util::io::ReaderStream;

use crate::SharedState;

pub(crate) async fn serve(State(st): State<SharedState>, uri: Uri) -> Response {
    let rest = uri
        .path()
        .strip_prefix("/app")
        .unwrap_or("")
        .trim_start_matches('/');

    let mut segments = Vec::new();
    for seg in rest.split('/') {
        match seg {
            "" | "." => {}
            ".." => return StatusCode::BAD_REQUEST.into_response(),
            s if s.contains('\\') || s.contains('\0') => {
                return StatusCode::BAD_REQUEST.into_response();
            }
            s => segments.push(s),
        }
    }

    let root = st.config.web_root.as_path();
    let file = match resolve_regular(root, &segments).await {
        Some(path) => Some(path),
        None => resolve_regular(root, &["index.html"]).await,
    };
    match file {
        Some(path) => serve_file(&path).await,
        None => (StatusCode::NOT_FOUND, "app not installed").into_response(),
    }
}

/// Walk `segments` under `root`, refusing any symlink. Returns the path only
/// when it names a regular file.
async fn resolve_regular(root: &Path, segments: &[&str]) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for seg in segments {
        cur.push(seg);
        let meta = tokio::fs::symlink_metadata(&cur).await.ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
    }
    let meta = tokio::fs::metadata(&cur).await.ok()?;
    meta.is_file().then_some(cur)
}

async fn serve_file(path: &Path) -> Response {
    let file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let len = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let is_html = path.extension().is_some_and(|e| e == "html");
    let cache = if is_html {
        "no-cache"
    } else {
        "public, max-age=300"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_for(path))
        .header(header::CONTENT_LENGTH, len)
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from_stream(ReaderStream::new(file)))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
