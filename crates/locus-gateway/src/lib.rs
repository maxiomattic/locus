//! Host-side gateway that runs next to the Urbit ship on a Locus server.
//!
//! Eyre holds all structured data. This gateway serves what Urbit cannot:
//! content-addressed bytes on ZFS, thumbnails, and the compiled browser app.
//! It gives the browser a single origin by reverse-proxying everything else to Eyre.

mod auth;
mod blob;
mod proxy;
mod thumb;
mod web;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::Router;
use axum::response::Redirect;
use axum::routing::{get, put};

/// Runtime configuration for the gateway.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the ship's Eyre, e.g. `http://127.0.0.1:80`.
    pub eyre: String,
    /// Root of blob, temp and thumbnail storage.
    pub data_dir: PathBuf,
    /// Directory holding the compiled browser app.
    pub web_root: PathBuf,
    /// Maximum accepted upload size in bytes.
    pub max_upload_bytes: u64,
}

/// State shared by every handler.
pub(crate) struct AppState {
    pub config: Config,
    /// Client for Eyre. Never follows redirects, never decodes bodies.
    pub client: reqwest::Client,
    /// Host header value to send upstream, e.g. `127.0.0.1:80`.
    pub upstream_host: String,
    /// Positive auth results keyed by SHA-256 of the credential headers.
    pub auth_cache: Mutex<HashMap<[u8; 32], Instant>>,
}

pub(crate) type SharedState = Arc<AppState>;

/// Build the gateway router. Creates the data-dir subdirectories.
pub fn router(config: Config) -> io::Result<Router> {
    for sub in ["blobs", "tmp", "thumbs"] {
        std::fs::create_dir_all(config.data_dir.join(sub))?;
    }
    let upstream = reqwest::Url::parse(&config.eyre)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("bad --eyre: {e}")))?;
    let host = upstream
        .host_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "--eyre has no host"))?;
    let upstream_host = match upstream.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(io::Error::other)?;
    let state: SharedState = Arc::new(AppState {
        config,
        client,
        upstream_host,
        auth_cache: Mutex::new(HashMap::new()),
    });

    Ok(Router::new()
        .route("/", get(|| async { Redirect::temporary("/app/") }))
        .route("/app", get(web::serve))
        .route("/app/", get(web::serve))
        .route("/app/{*path}", get(web::serve))
        .route("/blob", put(blob::upload))
        .route("/blob/{sha}", get(blob::download))
        .route("/thumb/{sha}", get(thumb::thumbnail))
        .fallback(proxy::forward)
        .with_state(state))
}

/// A process-unique name for temp files.
pub(crate) fn unique_name(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{prefix}-{}-{}-{nanos}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
