//! Everything not handled locally is forwarded to Eyre. Bodies stream both ways
//! and nothing is buffered, so SSE channels work.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures_util::TryStreamExt;

use crate::SharedState;

const HOP_BY_HOP: [&str; 9] = [
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

pub(crate) async fn forward(State(st): State<SharedState>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let url = format!("{}{}", st.config.eyre.trim_end_matches('/'), path_and_query);

    let mut headers = HeaderMap::new();
    let listed = connection_tokens(&parts.headers);
    for (name, value) in parts.headers.iter() {
        let n = name.as_str();
        if n == "host" || HOP_BY_HOP.contains(&n) || listed.iter().any(|t| t == n) {
            continue;
        }
        headers.append(name.clone(), value.clone());
    }
    // Eyre sees its own authority; the public host travels in X-Forwarded-Host.
    if let Some(original) = parts.headers.get(header::HOST) {
        headers.insert(
            HeaderName::from_static("x-forwarded-host"),
            original.clone(),
        );
    }
    if let Ok(v) = HeaderValue::from_str(&st.upstream_host) {
        headers.insert(header::HOST, v);
    }
    if !parts.headers.contains_key("x-forwarded-proto") {
        headers.insert(
            HeaderName::from_static("x-forwarded-proto"),
            HeaderValue::from_static("http"),
        );
    }

    let upstream = st
        .client
        .request(parts.method.clone(), url)
        .headers(headers)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await;
    let resp = match upstream {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "eyre unreachable");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let status = resp.status();
    let mut out = HeaderMap::new();
    let listed = connection_tokens(resp.headers());
    for (name, value) in resp.headers() {
        let n = name.as_str();
        if HOP_BY_HOP.contains(&n) || listed.iter().any(|t| t == n) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }

    let mut response = Response::new(Body::from_stream(
        resp.bytes_stream().map_err(std::io::Error::other),
    ));
    *response.status_mut() = status;
    *response.headers_mut() = out;
    response
}

/// Header names listed in the `Connection` header, lowercased.
fn connection_tokens(headers: &HeaderMap) -> Vec<String> {
    headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect()
}
