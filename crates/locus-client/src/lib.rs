//! Async HTTP client for Locus apps (`%notes`, `%photos`, `%contacts`,
//! `%calendar`, `%files`, `%locus`) and the host gateway's byte store
//! ([`blobs`]). Compiles for native targets (rustls, cookie store)
//! and for `wasm32-unknown-unknown` (browser `fetch`, which sends same-origin
//! cookies itself).
//!
//! The crate has no runtime of its own: every request is a plain `async fn`
//! and the caller supplies the executor (tokio, wasm-bindgen-futures, ...).
//!
//! Authentication is either the session cookie set by [`Client::login`] or an
//! `X-Api-Key` header set with [`Client::with_api_key`]. Both are optional at
//! the type level; the server decides what is allowed.

pub mod blobs;
pub mod calendar;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
pub mod contacts;
pub mod files;
pub mod locus;
pub mod notes;
pub mod photos;
pub mod tasks;

use std::fmt::Write as _;

use reqwest::{Method, RequestBuilder, header};
use serde::de::DeserializeOwned;
use serde_json::Value;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The server answered with a non-2xx HTTP status.
    #[error("HTTP {status} on {path}: {body}")]
    Http {
        status: u16,
        path: String,
        body: String,
    },
    /// An `expectedRevision` did not match the server's revision (HTTP 409 or
    /// a conflict reply body). `current` is the server's revision when it
    /// could be re-read after the conflict.
    #[error("conflict: stale expectedRevision")]
    Conflict { current: Option<u64> },
    /// The server answered 404: no such entry, or no such blob.
    #[error("not found: {path}: {body}")]
    NotFound { path: String, body: String },
    /// The server reported `ok: false` (or a typed notes error) that is not a
    /// conflict.
    #[error("{error}: {message}")]
    Api { error: String, message: String },
    /// A notes write was accepted but has not finished. Poll the request id
    /// with [`notes::Client::request_status`].
    #[error("request {0} is still pending")]
    Pending(String),
    /// The response body did not match the expected wire shape.
    #[error("bad response from {path}: {detail}")]
    Decode { path: String, detail: String },
    /// Transport failure: DNS, connection, TLS, or the browser refused the request.
    #[error("network error: {0}")]
    Network(String),
    /// A path given to a photos call escapes the photo dataset.
    #[error("path must be relative with no .. (got {0})")]
    BadPath(String),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Shared client for all Locus apps. Cheap to clone (the HTTP client is
/// reference counted and, on native, holds the session cookie jar).
#[derive(Clone)]
pub struct Client {
    base: Url,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl Client {
    /// Create a client for the ship at `base`, e.g. `http://127.0.0.1:8080`.
    pub fn new(base: Url) -> Result<Self> {
        Ok(Self {
            base,
            api_key: None,
            http: build_http()?,
        })
    }

    /// Send `X-Api-Key: <key>` on every request.
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// `POST /~/login` with the ship's web code (`+code`). On native the
    /// session cookie is kept in this client's cookie store; on wasm the
    /// browser keeps it.
    pub async fn login(&self, code: &str) -> Result<()> {
        const PATH: &str = "/~/login";
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("password", code.trim())
            .finish();
        let resp = self
            .http
            .post(self.endpoint(PATH)?)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(network)?;
        let status = resp.status();
        let text = resp.text().await.map_err(network)?;
        if status.is_success() {
            Ok(())
        } else {
            Err(Error::Http {
                status: status.as_u16(),
                path: PATH.into(),
                body: text,
            })
        }
    }

    /// Build a request for `path` (starting with `/`) with `Accept` and, when
    /// set, `X-Api-Key` headers.
    pub(crate) fn request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        Ok(self
            .with_auth(self.http.request(method, self.endpoint(path)?))
            .header(header::ACCEPT, "application/json"))
    }

    /// Add the `X-Api-Key` header when a key is set. Other headers are left to
    /// the caller (blob bodies are not JSON).
    pub(crate) fn with_auth(&self, rb: RequestBuilder) -> RequestBuilder {
        match &self.api_key {
            Some(key) => rb.header("X-Api-Key", key),
            None => rb,
        }
    }

    /// Send a request whose response body is JSON of type `T`. A non-2xx
    /// status becomes [`Error::Http`] with the raw body.
    pub(crate) async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<T> {
        let mut rb = self.request(method, path)?;
        if let Some(body) = body {
            rb = rb.json(&body);
        }
        let text = send_text(rb, path).await?;
        serde_json::from_str(&text).map_err(|e| Error::Decode {
            path: path.into(),
            detail: e.to_string(),
        })
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.json(Method::GET, path, None).await
    }

    /// Absolute URL for `path` (starting with `/`) on the ship's origin.
    fn endpoint(&self, path: &str) -> Result<Url> {
        let mut s = self.base.as_str().trim_end_matches('/').to_string();
        s.push_str(path);
        Ok(Url::parse(&s)?)
    }
}

fn build_http() -> Result<reqwest::Client> {
    #[cfg(not(target_arch = "wasm32"))]
    let builder = reqwest::Client::builder().cookie_store(true);
    #[cfg(target_arch = "wasm32")]
    let builder = reqwest::Client::builder();
    builder.build().map_err(network)
}

async fn send_text(rb: RequestBuilder, path: &str) -> Result<String> {
    let resp = rb.send().await.map_err(network)?;
    let status = resp.status();
    let text = resp.text().await.map_err(network)?;
    if status.is_success() {
        Ok(text)
    } else {
        Err(status_error(status.as_u16(), path, text))
    }
}

/// Like [`send_text`] but keeps the body as raw bytes (blob downloads).
pub(crate) async fn send_bytes(rb: RequestBuilder, path: &str) -> Result<Vec<u8>> {
    let resp = rb.send().await.map_err(network)?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp.bytes().await.map_err(network)?.to_vec());
    }
    let text = resp.text().await.map_err(network)?;
    Err(status_error(status.as_u16(), path, text))
}

/// Map a non-2xx status to an error. 409 is a stale revision and 404 a
/// missing entry; everything else keeps the raw status and body.
fn status_error(status: u16, path: &str, body: String) -> Error {
    match status {
        409 => Error::Conflict { current: None },
        404 => Error::NotFound {
            path: path.into(),
            body,
        },
        _ => Error::Http {
            status,
            path: path.into(),
            body,
        },
    }
}

/// Append `?expectedRevision=N` to a DELETE path when a revision is given.
pub(crate) fn with_revision(path: String, expected_revision: Option<u64>) -> String {
    match expected_revision {
        Some(rev) => format!("{path}?expectedRevision={rev}"),
        None => path,
    }
}

pub(crate) fn network(e: reqwest::Error) -> Error {
    Error::Network(e.to_string())
}

/// Map the `{ok, error, message}` envelope shared by photos, contacts,
/// calendar and locus. `error == "conflict"` becomes [`Error::Conflict`].
pub(crate) fn check_ok(ok: bool, error: Option<String>, message: Option<String>) -> Result<()> {
    if ok {
        return Ok(());
    }
    match error.as_deref() {
        Some("conflict") => Err(Error::Conflict { current: None }),
        _ => Err(Error::Api {
            error: error.unwrap_or_else(|| "error".into()),
            message: message.unwrap_or_default(),
        }),
    }
}

/// Percent-encode one path segment (host, notebook flag, request id, query
/// value). `~` and `.` are left alone, so `~zod` stays readable.
pub(crate) fn seg(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seg_encodes_reserved_characters() {
        assert_eq!(seg("~zod"), "~zod");
        assert_eq!(seg("my-notebook-1"), "my-notebook-1");
        assert_eq!(seg("a b/c?d"), "a%20b%2Fc%3Fd");
    }

    #[test]
    fn endpoint_joins_without_double_slash() {
        let c = Client::new(Url::parse("http://127.0.0.1:8080/").unwrap()).unwrap();
        assert_eq!(
            c.endpoint("/~/login").unwrap().as_str(),
            "http://127.0.0.1:8080/~/login"
        );
    }

    #[test]
    fn check_ok_maps_conflict_and_errors() {
        assert!(check_ok(true, None, None).is_ok());
        assert!(matches!(
            check_ok(false, Some("conflict".into()), Some("stale".into())),
            Err(Error::Conflict { current: None })
        ));
        match check_ok(false, Some("bad-request".into()), Some("need id".into())) {
            Err(Error::Api { error, message }) => {
                assert_eq!(error, "bad-request");
                assert_eq!(message, "need id");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn http_statuses_map_to_typed_errors() {
        assert!(matches!(
            status_error(409, "/p", "{}".into()),
            Error::Conflict { current: None }
        ));
        match status_error(404, "/p", "gone".into()) {
            Error::NotFound { path, body } => {
                assert_eq!(path, "/p");
                assert_eq!(body, "gone");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            status_error(400, "/p", "bad".into()),
            Error::Http { status: 400, .. }
        ));
    }

    #[test]
    fn revision_query_is_optional() {
        assert_eq!(with_revision("/x/1".into(), None), "/x/1");
        assert_eq!(
            with_revision("/x/1".into(), Some(0)),
            "/x/1?expectedRevision=0"
        );
    }
}

#[cfg(test)]
mod loopback_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    /// Read one HTTP/1.1 request (head plus Content-Length body) off the socket.
    async fn read_request(sock: &mut TcpStream) -> String {
        let mut buf = Vec::new();
        loop {
            let mut tmp = [0u8; 4096];
            let n = sock.read(&mut tmp).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            let Some(head_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let len = head
                .lines()
                .find_map(|l| {
                    let l = l.to_ascii_lowercase();
                    l.strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if buf.len() >= head_end + 4 + len {
                break;
            }
        }
        String::from_utf8_lossy(&buf).to_string()
    }

    async fn respond(sock: &mut TcpStream, status: &str, body: &str) {
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        sock.write_all(resp.as_bytes()).await.unwrap();
        sock.shutdown().await.ok();
    }

    #[tokio::test]
    async fn stale_save_sends_api_key_and_maps_conflict() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Serves two requests: the stale PUT, then the best-effort re-read.
        let server = tokio::spawn(async move {
            let mut saw = Vec::new();
            let (mut sock, _) = listener.accept().await.unwrap();
            let put = read_request(&mut sock).await;
            saw.push(put.clone());
            respond(
                &mut sock,
                "200 OK",
                r#"{"requestId":"0v1","body":{"type":"error","errorType":"conflict","message":[]}}"#,
            )
            .await;

            let (mut sock, _) = listener.accept().await.unwrap();
            let get = read_request(&mut sock).await;
            saw.push(get);
            respond(&mut sock, "404 Not Found", "{}").await;
            saw
        });

        let client = Client::new(Url::parse(&format!("http://{addr}")).unwrap())
            .unwrap()
            .with_api_key("secret-key");
        let err = client
            .save_note("~zod", "nb-1", 7, "body", 3)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Conflict { current: None }), "{err}");

        let saw = server.await.unwrap();
        let put_head = saw[0].to_ascii_lowercase();
        assert!(put_head.starts_with("put /notes/~/v1/notebooks/~zod/nb-1/notes/7 "));
        assert!(put_head.contains("x-api-key: secret-key"));
        assert!(
            put_head.contains(r#""expectedrevision":3"#)
                || saw[0].contains("\"expectedRevision\":3")
        );
        assert!(saw[1].starts_with("GET /notes/~/v1/notebooks/~zod/nb-1/notes/7 "));
    }

    #[tokio::test]
    async fn http_error_status_keeps_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let _ = read_request(&mut sock).await;
            respond(&mut sock, "401 Unauthorized", "no session").await;
        });
        let client = Client::new(Url::parse(&format!("http://{addr}")).unwrap()).unwrap();
        match client.list_notebooks().await {
            Err(Error::Http { status, path, body }) => {
                assert_eq!(status, 401);
                assert_eq!(path, "/notes/~/v1/notebooks");
                assert_eq!(body, "no session");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Serve one canned reply and hand back the request the server saw.
    async fn one_shot(status: &str, body: &str) -> (Client, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (status, body) = (status.to_string(), body.to_string());
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let req = read_request(&mut sock).await;
            respond(&mut sock, &status, &body).await;
            req
        });
        let client = Client::new(Url::parse(&format!("http://{addr}")).unwrap())
            .unwrap()
            .with_api_key("secret-key");
        (client, server)
    }

    #[tokio::test]
    async fn deletes_send_optional_expected_revision() {
        let ok = r#"{"ok":true,"id":7}"#;

        let (c, srv) = one_shot("200 OK", ok).await;
        c.delete_photo(7, Some(3)).await.unwrap();
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("DELETE /photos/~/v1/photos/7?expectedRevision=3 "),
            "{req}"
        );

        let (c, srv) = one_shot("200 OK", ok).await;
        c.delete_contact(7, None).await.unwrap();
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("DELETE /contacts/~/v1/contacts/7 "),
            "{req}"
        );

        let (c, srv) = one_shot("200 OK", ok).await;
        c.delete_event(7, Some(0)).await.unwrap();
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("DELETE /calendar/~/v1/events/7?expectedRevision=0 "),
            "{req}"
        );

        let (c, srv) = one_shot("200 OK", ok).await;
        c.delete_file(7, Some(2)).await.unwrap();
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("DELETE /files/~/v1/files/7?expectedRevision=2 "),
            "{req}"
        );
    }

    #[tokio::test]
    async fn stale_revision_http_409_maps_to_conflict() {
        let body = r#"{"ok":false,"error":"conflict","message":"stale expectedRevision"}"#;
        let (c, srv) = one_shot("409 Conflict", body).await;
        let err = c
            .patch_file(
                3,
                1,
                &crate::files::FilePatch {
                    name: Some("x".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Conflict { current: None }), "{err}");
        let req = srv.await.unwrap();
        assert!(req.starts_with("PUT /files/~/v1/files/3 "), "{req}");
        assert!(req.contains(r#""expectedRevision":1"#), "{req}");

        let (c, _srv) = one_shot("409 Conflict", body).await;
        let err = c.delete_event(3, Some(9)).await.unwrap_err();
        assert!(matches!(err, Error::Conflict { current: None }), "{err}");
    }

    #[tokio::test]
    async fn missing_entry_http_404_maps_to_not_found() {
        let body = r#"{"ok":false,"error":"not-found","message":"no such photo"}"#;
        let (c, _srv) = one_shot("404 Not Found", body).await;
        match c.delete_photo(99, None).await {
            Err(Error::NotFound { path, body }) => {
                assert_eq!(path, "/photos/~/v1/photos/99");
                assert!(body.contains("no such photo"));
            }
            other => panic!("unexpected {other:?}"),
        }

        let (c, _srv) = one_shot("404 Not Found", body).await;
        assert!(matches!(c.get_file(99).await, Err(Error::NotFound { .. })));
    }

    #[tokio::test]
    async fn list_files_encodes_dir_filter() {
        let (c, srv) = one_shot("200 OK", r#"{"ok":true,"files":[]}"#).await;
        assert!(c.list_files(Some("/a b")).await.unwrap().is_empty());
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("GET /files/~/v1/files?dir=%2Fa+b "),
            "{req}"
        );
    }

    #[tokio::test]
    async fn delete_dir_sends_path_query() {
        let (c, srv) = one_shot("200 OK", r#"{"ok":true,"path":"/a/b"}"#).await;
        c.delete_dir("/a/b").await.unwrap();
        let req = srv.await.unwrap();
        assert!(
            req.starts_with("DELETE /files/~/v1/dirs?path=%2Fa%2Fb "),
            "{req}"
        );
    }

    #[tokio::test]
    async fn upload_blob_accepts_matching_hash() {
        let bytes = b"abc".to_vec();
        let sha = crate::blobs::sha256_hex(&bytes);
        let reply = format!(r#"{{"sha256":"{sha}","size":3}}"#);
        let (c, srv) = one_shot("200 OK", &reply).await;
        let got = c.upload_blob(bytes).await.unwrap();
        assert_eq!(got.sha256, sha);
        assert_eq!(got.size, 3);
        let req = srv.await.unwrap();
        assert!(req.starts_with("PUT /blob "), "{req}");
        assert!(
            req.to_ascii_lowercase()
                .contains("content-type: application/octet-stream")
        );
        assert!(req.ends_with("abc"), "{req}");
    }

    #[tokio::test]
    async fn upload_blob_rejects_sha_mismatch() {
        let wrong = "0".repeat(64);
        let reply = format!(r#"{{"sha256":"{wrong}","size":3}}"#);
        let (c, _srv) = one_shot("200 OK", &reply).await;
        match c.upload_blob(b"abc".to_vec()).await {
            Err(Error::Decode { path, detail }) => {
                assert_eq!(path, "/blob");
                assert!(detail.contains(&wrong), "{detail}");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn download_blob_returns_bytes_with_api_key() {
        let (c, srv) = one_shot("200 OK", "raw-bytes").await;
        let sha = "1".repeat(64);
        assert_eq!(c.download_blob(&sha).await.unwrap(), b"raw-bytes");
        let req = srv.await.unwrap();
        assert!(req.starts_with(&format!("GET /blob/{sha} ")), "{req}");
        assert!(req.to_ascii_lowercase().contains("x-api-key: secret-key"));

        let (c, _srv) = one_shot("404 Not Found", "not found").await;
        assert!(matches!(
            c.download_thumb(&sha, 128).await,
            Err(Error::NotFound { .. })
        ));
    }
}

#[cfg(test)]
mod live_tests {
    //! Runs against a live bring-up ship. Skipped unless `LOCUS_LIVE=1`:
    //!
    //! ```text
    //! LOCUS_LIVE=1 LOCUS_CODE=<+code> cargo test -p locus-client live -- --ignored --nocapture
    //! ```
    //!
    //! `LOCUS_BASE` overrides the default `http://127.0.0.1:8080`. Each step
    //! deletes what it creates; a failed run can leave `/locus-live-test` behind.
    use super::*;
    use crate::files::FilePatch;

    #[tokio::test]
    #[ignore = "needs a running ship: LOCUS_LIVE=1 LOCUS_CODE=<+code>"]
    async fn live_files_photos_contacts_events() {
        if std::env::var("LOCUS_LIVE").as_deref() != Ok("1") {
            eprintln!("LOCUS_LIVE is not 1; skipping");
            return;
        }
        let code = std::env::var("LOCUS_CODE").expect("LOCUS_CODE must hold the ship's web code");
        let base = std::env::var("LOCUS_BASE").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
        let c = Client::new(Url::parse(&base).unwrap()).unwrap();
        c.login(&code).await.expect("login");

        files_steps(&c).await;
        photo_steps(&c).await;
        contact_steps(&c).await;
        event_steps(&c).await;
    }

    async fn files_steps(c: &Client) {
        let dir = "/locus-live-test";
        c.create_dir(dir).await.unwrap();
        let sha = "a".repeat(64);
        let f = c
            .create_file(dir, "one.txt", &sha, 5, "text/plain")
            .await
            .unwrap();
        assert_eq!(f.dir, dir);
        assert_eq!(f.revision, 0);

        assert!(
            c.list_files(Some(dir))
                .await
                .unwrap()
                .iter()
                .any(|e| e.id == f.id)
        );
        assert!(c.list_dirs().await.unwrap().iter().any(|d| d == dir));
        assert_eq!(c.get_file(f.id).await.unwrap().name, "one.txt");

        let moved = c
            .patch_file(
                f.id,
                0,
                &FilePatch {
                    name: Some("two.txt".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!((moved.name.as_str(), moved.revision), ("two.txt", 1));
        let stale = c
            .patch_file(
                f.id,
                0,
                &FilePatch {
                    name: Some("three.txt".into()),
                    ..Default::default()
                },
            )
            .await;
        assert!(matches!(stale, Err(Error::Conflict { .. })), "{stale:?}");

        assert!(matches!(
            c.get_file(999_999).await,
            Err(Error::NotFound { .. })
        ));
        assert!(matches!(
            c.delete_file(f.id, Some(0)).await,
            Err(Error::Conflict { .. })
        ));
        c.delete_file(f.id, Some(1)).await.unwrap();
        assert!(matches!(
            c.get_file(f.id).await,
            Err(Error::NotFound { .. })
        ));
        c.delete_dir(dir).await.unwrap();
        eprintln!("files: ok");
    }

    async fn photo_steps(c: &Client) {
        let p = c
            .create_photo("locus-live", "probe", "locus-live/probe.jpg", &[])
            .await
            .unwrap();
        assert!(c.list_photos().await.unwrap().iter().any(|x| x.id == p.id));
        let stale = c.delete_photo(p.id, Some(p.revision + 1)).await;
        assert!(matches!(stale, Err(Error::Conflict { .. })), "{stale:?}");
        c.delete_photo(p.id, Some(p.revision)).await.unwrap();
        assert!(matches!(
            c.delete_photo(p.id, None).await,
            Err(Error::NotFound { .. })
        ));
        eprintln!("photos: ok");
    }

    async fn contact_steps(c: &Client) {
        let ct = c.create_contact("Locus Live", "", "").await.unwrap();
        let stale = c.delete_contact(ct.id, Some(ct.revision + 1)).await;
        assert!(matches!(stale, Err(Error::Conflict { .. })), "{stale:?}");
        c.delete_contact(ct.id, Some(ct.revision)).await.unwrap();
        assert!(matches!(
            c.delete_contact(ct.id, None).await,
            Err(Error::NotFound { .. })
        ));
        eprintln!("contacts: ok");
    }

    async fn event_steps(c: &Client) {
        let ev = c
            .create_event("Locus live", 1_767_225_600, 1_767_229_200, "")
            .await
            .unwrap();
        let stale = c.delete_event(ev.id, Some(ev.revision + 1)).await;
        assert!(matches!(stale, Err(Error::Conflict { .. })), "{stale:?}");
        c.delete_event(ev.id, Some(ev.revision)).await.unwrap();
        assert!(matches!(
            c.delete_event(ev.id, None).await,
            Err(Error::NotFound { .. })
        ));
        eprintln!("events: ok");
    }
}
