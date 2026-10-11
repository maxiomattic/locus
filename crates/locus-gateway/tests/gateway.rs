//! End-to-end tests: a real gateway on an ephemeral port, a fake Eyre, and a temp data-dir.

use std::net::SocketAddr;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use locus_gateway::{Config, router};
use sha2::{Digest, Sha256};

const GOOD_COOKIE: &str = "urbauth-~zod=good";

/// Fake Eyre: whoami accepts one cookie; everything else echoes the request.
async fn start_fake_eyre() -> SocketAddr {
    let app = Router::new()
        .route(
            "/locus/~/v1/whoami",
            get(|headers: HeaderMap| async move {
                let ok = headers
                    .get(header::COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|c| c.split(';').any(|p| p.trim() == GOOD_COOKIE));
                if ok {
                    StatusCode::OK.into_response()
                } else {
                    StatusCode::UNAUTHORIZED.into_response()
                }
            }),
        )
        .fallback(echo);
    serve(app).await
}

async fn echo(req: Request) -> Response {
    let method = req.method().to_string();
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.to_string())
        .unwrap_or_default();
    let cookie = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let mut resp = Response::new(Body::from(body));
    let h = resp.headers_mut();
    h.insert("x-echo-method", method.parse().unwrap());
    h.insert("x-echo-path", path.parse().unwrap());
    h.insert(
        "x-echo-cookie",
        cookie.parse().unwrap_or_else(|_| "".parse().unwrap()),
    );
    h.insert(
        header::SET_COOKIE,
        "sid=abc; Path=/; HttpOnly".parse().unwrap(),
    );
    resp
}

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

struct Fixture {
    gateway: SocketAddr,
    data: tempfile::TempDir,
    _web: tempfile::TempDir,
    _eyre: SocketAddr,
    client: reqwest::Client,
}

async fn fixture_with_limit(max_upload_bytes: u64) -> Fixture {
    let eyre = start_fake_eyre().await;
    let data = tempfile::tempdir().unwrap();
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<html>app</html>").unwrap();
    std::fs::write(web.path().join("app.js"), "console.log(1)").unwrap();
    std::fs::write(web.path().join("app.wasm"), b"\0asm\x01\0\0\0").unwrap();

    let app = router(Config {
        eyre: format!("http://{eyre}"),
        data_dir: data.path().to_path_buf(),
        web_root: web.path().to_path_buf(),
        max_upload_bytes,
    })
    .unwrap();
    let gateway = serve(app).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    Fixture {
        gateway,
        data,
        _web: web,
        _eyre: eyre,
        client,
    }
}

async fn fixture() -> Fixture {
    fixture_with_limit(64 * 1024 * 1024).await
}

impl Fixture {
    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.gateway, path)
    }

    async fn upload(&self, bytes: Vec<u8>) -> reqwest::Response {
        self.client
            .put(self.url("/blob"))
            .header(header::COOKIE, GOOD_COOKIE)
            .body(bytes)
            .send()
            .await
            .unwrap()
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sample_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 % 251) as u8).collect()
}

fn count_files(dir: &Path) -> usize {
    let mut n = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path().is_dir() {
                n += count_files(&e.path());
            } else {
                n += 1;
            }
        }
    }
    n
}

#[tokio::test]
async fn upload_returns_sha_and_get_returns_same_bytes() {
    let fx = fixture().await;
    let bytes = sample_bytes(100_000);
    let resp = fx.upload(bytes.clone()).await;
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = serde_json::from_str(&resp.text().await.unwrap()).unwrap();
    let sha = sha_hex(&bytes);
    assert_eq!(json["sha256"], sha.as_str());
    assert_eq!(json["size"], bytes.len());

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["content-length"],
        bytes.len().to_string().as_str()
    );
    assert_eq!(resp.headers()["etag"], format!("\"{sha}\"").as_str());
    assert_eq!(
        resp.headers()["cache-control"],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(resp.bytes().await.unwrap().as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn duplicate_upload_dedupes() {
    let fx = fixture().await;
    let bytes = sample_bytes(5_000);
    let a: serde_json::Value =
        serde_json::from_str(&fx.upload(bytes.clone()).await.text().await.unwrap()).unwrap();
    let b: serde_json::Value =
        serde_json::from_str(&fx.upload(bytes.clone()).await.text().await.unwrap()).unwrap();
    assert_eq!(a["sha256"], b["sha256"]);
    assert_eq!(count_files(&fx.data.path().join("blobs")), 1);
    assert_eq!(count_files(&fx.data.path().join("tmp")), 0);
    let sha = sha_hex(&bytes);
    assert!(
        fx.data
            .path()
            .join("blobs")
            .join(&sha[..2])
            .join(&sha)
            .is_file()
    );
}

#[tokio::test]
async fn range_request_returns_slice() {
    let fx = fixture().await;
    let bytes = sample_bytes(1000);
    fx.upload(bytes.clone()).await;
    let sha = sha_hex(&bytes);
    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .header(header::COOKIE, GOOD_COOKIE)
        .header(header::RANGE, "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.headers()["content-range"], "bytes 10-19/1000");
    assert_eq!(resp.headers()["content-length"], "10");
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &bytes[10..20]);

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .header(header::COOKIE, GOOD_COOKIE)
        .header(header::RANGE, "bytes=-5")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 206);
    assert_eq!(resp.bytes().await.unwrap().as_ref(), &bytes[995..]);

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .header(header::COOKIE, GOOD_COOKIE)
        .header(header::RANGE, "bytes=5000-")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 416);
}

#[tokio::test]
async fn blob_content_type_and_name_follow_query() {
    let fx = fixture().await;
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&[0; 8]);
    fx.upload(png.clone()).await;
    let sha = sha_hex(&png);
    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}?name=photo%20one.png")))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.headers()["content-type"], "image/png");
    assert_eq!(
        resp.headers()["content-disposition"],
        "inline; filename=\"photo one.png\""
    );

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}?type=text/html")))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.headers()["content-type"], "image/png");
}

#[tokio::test]
async fn bad_hash_is_400() {
    let fx = fixture().await;
    for bad in [
        "abc",
        &"A".repeat(64),
        &"g".repeat(64),
        "..%2F..%2Fetc%2Fpasswd",
    ] {
        let resp = fx
            .client
            .get(fx.url(&format!("/blob/{bad}")))
            .header(header::COOKIE, GOOD_COOKIE)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "hash {bad}");
    }
}

#[tokio::test]
async fn missing_or_bad_auth_is_401() {
    let fx = fixture().await;
    let bytes = sample_bytes(10);
    let sha = sha_hex(&bytes);

    let resp = fx
        .client
        .put(fx.url("/blob"))
        .body(bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{sha}")))
        .header(header::COOKIE, "urbauth-~zod=nope")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    let resp = fx
        .client
        .get(fx.url(&format!("/thumb/{sha}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn api_key_alone_is_refused_by_fake_ship() {
    let fx = fixture().await;
    let resp = fx
        .client
        .get(fx.url(&format!("/blob/{}", "0".repeat(64))))
        .header("x-api-key", "whatever")
        .send()
        .await
        .unwrap();
    // The fake Eyre only checks cookies, so an API key alone is refused.
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn upload_over_limit_is_413() {
    let fx = fixture_with_limit(1000).await;
    let resp = fx.upload(vec![7u8; 2000]).await;
    assert_eq!(resp.status(), 413);
    assert_eq!(count_files(&fx.data.path().join("tmp")), 0);
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[tokio::test]
async fn thumbnail_is_jpeg_with_longest_side() {
    let fx = fixture().await;
    let png = png_bytes(800, 400);
    fx.upload(png.clone()).await;
    let sha = sha_hex(&png);

    let resp = fx
        .client
        .get(fx.url(&format!("/thumb/{sha}?size=256")))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["content-type"], "image/jpeg");
    assert_eq!(
        resp.headers()["cache-control"],
        "private, max-age=31536000, immutable"
    );
    let body = resp.bytes().await.unwrap();
    let decoded = image::load_from_memory_with_format(&body, image::ImageFormat::Jpeg).unwrap();
    assert_eq!(decoded.width().max(decoded.height()), 256);
    assert!(
        fx.data
            .path()
            .join("thumbs/256")
            .join(format!("{sha}.jpg"))
            .is_file()
    );

    // Second request is served from the cache.
    let resp = fx
        .client
        .get(fx.url(&format!("/thumb/{sha}?size=256")))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn thumbnail_of_non_image_is_415() {
    let fx = fixture().await;
    let text = b"just some text, not an image".to_vec();
    fx.upload(text.clone()).await;
    let resp = fx
        .client
        .get(fx.url(&format!("/thumb/{}?size=256", sha_hex(&text))))
        .header(header::COOKIE, GOOD_COOKIE)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 415);
}

#[tokio::test]
async fn proxy_passes_method_path_body_cookie_and_set_cookie() {
    let fx = fixture().await;
    let resp = fx
        .client
        .post(fx.url("/~/login?redirect=/apps"))
        .header(header::COOKIE, GOOD_COOKIE)
        .header("x-api-key", "k")
        .body("password=secret")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers()["x-echo-method"], "POST");
    assert_eq!(resp.headers()["x-echo-path"], "/~/login?redirect=/apps");
    assert_eq!(resp.headers()["x-echo-cookie"], GOOD_COOKIE);
    assert!(
        resp.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .starts_with("sid=abc")
    );
    assert_eq!(resp.text().await.unwrap(), "password=secret");
}

#[tokio::test]
async fn app_serves_files_and_spa_fallback() {
    let fx = fixture().await;

    let resp = fx.client.get(fx.url("/")).send().await.unwrap();
    assert_eq!(resp.status(), 307);
    assert_eq!(resp.headers()["location"], "/app/");

    let resp = fx.client.get(fx.url("/app/")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "<html>app</html>");

    let resp = fx
        .client
        .get(fx.url("/app/some/deep/route"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "<html>app</html>");

    let resp = fx.client.get(fx.url("/app/app.wasm")).send().await.unwrap();
    assert_eq!(resp.headers()["content-type"], "application/wasm");

    let resp = fx.client.get(fx.url("/app/app.js")).send().await.unwrap();
    assert_eq!(
        resp.headers()["content-type"],
        "text/javascript; charset=utf-8"
    );

    // Traversal never leaves web-root: the response is the SPA shell or a 400.
    let resp = fx
        .client
        .get(fx.url("/app/..%2F..%2Fetc%2Fpasswd"))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap();
    assert!(
        status == 400 || body == "<html>app</html>",
        "status {status}"
    );
}
