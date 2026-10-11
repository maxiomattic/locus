//! Authentication for /blob and /thumb: the caller's credentials are checked
//! against the ship's `whoami` endpoint.

use std::time::{Duration, Instant};

use axum::http::{HeaderMap, HeaderName, StatusCode, header::COOKIE};
use sha2::{Digest, Sha256};

use crate::SharedState;

pub const CACHE_TTL: Duration = Duration::from_secs(60);
const WHOAMI_TIMEOUT: Duration = Duration::from_secs(10);
const CACHE_PRUNE_AT: usize = 4096;

const API_KEY: HeaderName = HeaderName::from_static("x-api-key");

/// Returns true when the request's Cookie or X-Api-Key is accepted by the ship.
/// Credentials are never logged.
pub(crate) async fn authorized(state: &SharedState, headers: &HeaderMap) -> bool {
    let cookies: Vec<&[u8]> = headers
        .get_all(COOKIE)
        .iter()
        .map(|v| v.as_bytes())
        .collect();
    let api_keys: Vec<&[u8]> = headers
        .get_all(API_KEY)
        .iter()
        .map(|v| v.as_bytes())
        .collect();
    if cookies.is_empty() && api_keys.is_empty() {
        return false;
    }

    let mut hasher = Sha256::new();
    for v in &cookies {
        hasher.update(b"cookie\0");
        hasher.update(v);
        hasher.update(b"\n");
    }
    for v in &api_keys {
        hasher.update(b"x-api-key\0");
        hasher.update(v);
        hasher.update(b"\n");
    }
    let key: [u8; 32] = hasher.finalize().into();

    {
        let mut cache = state.auth_cache.lock().unwrap_or_else(|e| e.into_inner());
        match cache.get(&key) {
            Some(at) if at.elapsed() < CACHE_TTL => return true,
            Some(_) => {
                cache.remove(&key);
            }
            None => {}
        }
    }

    let url = format!(
        "{}/locus/~/v1/whoami",
        state.config.eyre.trim_end_matches('/')
    );
    let mut request = state.client.get(url).timeout(WHOAMI_TIMEOUT);
    if !cookies.is_empty() {
        let mut joined = Vec::new();
        for (i, c) in cookies.iter().enumerate() {
            if i > 0 {
                joined.extend_from_slice(b"; ");
            }
            joined.extend_from_slice(c);
        }
        request = request.header(COOKIE, joined);
    }
    if let Some(first) = api_keys.first() {
        request = request.header(API_KEY, *first);
    }

    let allowed = matches!(request.send().await, Ok(r) if r.status() == StatusCode::OK);
    if allowed {
        let mut cache = state.auth_cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= CACHE_PRUNE_AT {
            cache.retain(|_, at| at.elapsed() < CACHE_TTL);
        }
        cache.insert(key, Instant::now());
    }
    allowed
}
