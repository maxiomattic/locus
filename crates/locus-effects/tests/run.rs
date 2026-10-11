#![cfg(not(target_arch = "wasm32"))]

//! `run` against a loopback HTTP server and a fake platform.

use std::future::Future;
use std::sync::{Arc, Mutex};

use locus_client::Client;
use locus_client::notes::{Notebook, NotebookSummary};
use locus_core::{Effect, Msg, Section};
use locus_effects::{BlobHint, MaybeSend, PickedFile, Platform, now, run, session_ok};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

type Seen = Arc<Mutex<Vec<String>>>;

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

/// Serve requests until the test runtime shuts down. `route(method, path)`
/// returns a status line (`"200 OK"`) and a JSON body. Every raw request is
/// recorded in arrival order.
async fn serve<F>(route: F) -> (Client, Seen)
where
    F: Fn(&str, &str) -> (&'static str, String) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let raw = read_request(&mut sock).await;
            let mut parts = raw.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let (status, body) = route(&method, &path);
            log.lock().unwrap().push(raw);
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
            sock.shutdown().await.ok();
        }
    });
    let client = Client::new(Url::parse(&format!("http://{addr}")).unwrap())
        .unwrap()
        .with_api_key("secret-key");
    (client, seen)
}

/// A client that is never sent a request (for effects that only build URLs).
fn offline_client() -> Client {
    Client::new(Url::parse("http://127.0.0.1:9").unwrap()).unwrap()
}

#[derive(Default)]
struct Fake {
    picked: Vec<PickedFile>,
    sleeps: Mutex<Vec<u64>>,
    opened: Mutex<Vec<(Url, Option<BlobHint>)>>,
}

impl Platform for Fake {
    fn sleep(&self, ms: u64) -> impl Future<Output = ()> + MaybeSend {
        self.sleeps.lock().unwrap().push(ms);
        async {}
    }

    fn pick_files(&self) -> impl Future<Output = Vec<PickedFile>> + MaybeSend {
        let picked = self.picked.clone();
        async move { picked }
    }

    fn open_blob(
        &self,
        url: Url,
        hint: Option<BlobHint>,
    ) -> impl Future<Output = Result<(), String>> + MaybeSend {
        self.opened.lock().unwrap().push((url, hint));
        async { Ok(()) }
    }
}

fn nb() -> NotebookSummary {
    NotebookSummary {
        host: "~zod".into(),
        flag_name: "nb-1".into(),
        notebook: Notebook {
            id: 1,
            title: "Main".into(),
            root_folder_id: 2,
        },
        visibility: "private".into(),
    }
}

const NOTES_PATH: &str = "/notes/~/v1/notebooks/~zod/nb-1/notes";
const NOTE_PATH: &str = "/notes/~/v1/notebooks/~zod/nb-1/notes/7";

fn note_json(revision: u64, body: &str) -> String {
    format!(r#"{{"id":7,"title":"Todo","bodyMd":"{body}","revision":{revision}}}"#)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn load_notes_returns_notes() {
    let (c, _seen) = serve(|m, p| match (m, p) {
        ("GET", NOTES_PATH) => ("200 OK", format!("[{}]", note_json(3, ""))),
        _ => ("404 Not Found", "{}".into()),
    })
    .await;
    let msgs = run(&c, &Fake::default(), Effect::LoadNotes { nb: nb() }).await;
    match msgs.as_slice() {
        [Msg::NotesLoaded { nb: got, notes }] => {
            assert_eq!(got, &nb());
            assert_eq!(notes.len(), 1);
            assert_eq!((notes[0].id, notes[0].revision), (7, 3));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn save_note_conflict_maps_to_save_conflict() {
    let conflict =
        r#"{"requestId":"0v1","body":{"type":"error","errorType":"conflict","message":[]}}"#;
    let (c, _seen) = serve(move |m, p| match (m, p) {
        ("PUT", NOTE_PATH) => ("200 OK", conflict.into()),
        ("GET", NOTE_PATH) => ("200 OK", note_json(5, "server")),
        _ => ("404 Not Found", "{}".into()),
    })
    .await;
    let msgs = run(
        &c,
        &Fake::default(),
        Effect::SaveNote {
            nb: nb(),
            id: 7,
            body: "mine".into(),
            expected_revision: 3,
        },
    )
    .await;
    assert_eq!(
        msgs,
        vec![Msg::SaveConflict {
            nb: nb(),
            id: 7,
            body: "mine".into(),
            current: Some(5),
        }]
    );
}

#[tokio::test]
async fn save_note_success_rereads_the_note() {
    let (c, seen) = serve(|m, p| match (m, p) {
        ("PUT", NOTE_PATH) => (
            "200 OK",
            r#"{"requestId":"0v1","body":{"type":"ok"}}"#.into(),
        ),
        ("GET", NOTE_PATH) => ("200 OK", note_json(4, "new body")),
        _ => ("404 Not Found", "{}".into()),
    })
    .await;
    let msgs = run(
        &c,
        &Fake::default(),
        Effect::SaveNote {
            nb: nb(),
            id: 7,
            body: "new body".into(),
            expected_revision: 3,
        },
    )
    .await;
    match msgs.as_slice() {
        [Msg::NoteSaved { nb: got, note }] => {
            assert_eq!(got, &nb());
            assert_eq!(note.revision, 4);
            assert_eq!(note.body_md, "new body");
        }
        other => panic!("unexpected {other:?}"),
    }
    let seen = seen.lock().unwrap();
    assert!(seen[0].starts_with("PUT /notes/~/v1/notebooks/~zod/nb-1/notes/7 "));
    assert!(seen[1].starts_with("GET /notes/~/v1/notebooks/~zod/nb-1/notes/7 "));
}

#[tokio::test]
async fn schedule_save_returns_timer_with_same_token() {
    let fake = Fake::default();
    let msgs = run(
        &offline_client(),
        &fake,
        Effect::ScheduleSave {
            after_ms: 250,
            token: 42,
        },
    )
    .await;
    assert_eq!(msgs, vec![Msg::SaveTimer(42)]);
    assert_eq!(*fake.sleeps.lock().unwrap(), vec![250]);
}

#[tokio::test]
async fn upload_files_stores_blob_then_adds_entry() {
    let bytes = b"hello".to_vec();
    let sha = sha256_hex(&bytes);
    let blob_reply = format!(r#"{{"sha256":"{sha}","size":5}}"#);
    let entry = format!(
        r#"{{"ok":true,"file":{{"id":1,"dir":"/docs","name":"a.txt","sha256":"{sha}","size":5,"mime":"text/plain","created":0,"modified":0,"revision":0}}}}"#
    );
    let (c, seen) = serve(move |m, p| match (m, p) {
        ("PUT", "/blob") => ("200 OK", blob_reply.clone()),
        ("POST", "/files/~/v1/files") => ("200 OK", entry.clone()),
        _ => ("404 Not Found", "{}".into()),
    })
    .await;
    let fake = Fake {
        picked: vec![PickedFile {
            name: "a.txt".into(),
            mime: "text/plain".into(),
            bytes,
        }],
        ..Default::default()
    };
    let msgs = run(
        &c,
        &fake,
        Effect::UploadFiles {
            dir: "/docs".into(),
        },
    )
    .await;
    assert_eq!(
        msgs,
        vec![Msg::UploadDone {
            dir: "/docs".into(),
            count: 1
        }]
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(seen[0].starts_with("PUT /blob "), "{}", seen[0]);
    assert!(
        seen[1].starts_with("POST /files/~/v1/files "),
        "{}",
        seen[1]
    );
    assert!(seen[1].contains(r#""name":"a.txt""#), "{}", seen[1]);
}

#[tokio::test]
async fn upload_under_photos_uses_photo_catalog() {
    let bytes = b"pixels".to_vec();
    let sha = sha256_hex(&bytes);
    let blob_reply = format!(r#"{{"sha256":"{sha}","size":6}}"#);
    let photo = format!(
        r#"{{"ok":true,"photo":{{"id":3,"album":"2026","title":"p.jpg","path":"sha256:{sha}","tags":[],"revision":0}}}}"#
    );
    let (c, seen) = serve(move |m, p| match (m, p) {
        ("PUT", "/blob") => ("200 OK", blob_reply.clone()),
        ("POST", "/photos/~/v1/photos") => ("200 OK", photo.clone()),
        _ => ("404 Not Found", "{}".into()),
    })
    .await;
    let fake = Fake {
        picked: vec![PickedFile {
            name: "p.jpg".into(),
            mime: "image/jpeg".into(),
            bytes,
        }],
        ..Default::default()
    };
    let msgs = run(
        &c,
        &fake,
        Effect::UploadFiles {
            dir: "/Photos/2026".into(),
        },
    )
    .await;
    assert_eq!(
        msgs,
        vec![Msg::UploadDone {
            dir: "/Photos/2026".into(),
            count: 1
        }]
    );
    let seen = seen.lock().unwrap();
    assert!(
        seen[1].starts_with("POST /photos/~/v1/photos "),
        "{}",
        seen[1]
    );
    assert!(seen[1].contains(r#""album":"2026""#), "{}", seen[1]);
}

#[tokio::test]
async fn server_error_maps_to_load_failed() {
    let (c, _seen) = serve(|_, _| ("500 Internal Server Error", "boom".into())).await;
    let msgs = run(&c, &Fake::default(), Effect::LoadNotebooks).await;
    assert_eq!(
        msgs,
        vec![Msg::LoadFailed {
            section: Section::Notes,
            error: "server returned HTTP 500".into(),
        }]
    );
}

#[tokio::test]
async fn copy_to_clipboard_is_shell_handled() {
    let msgs = run(
        &offline_client(),
        &Fake::default(),
        Effect::CopyToClipboard("/x".into()),
    )
    .await;
    assert!(msgs.is_empty());
}

#[tokio::test]
async fn open_blob_builds_url_and_returns_no_msg() {
    let fake = Fake::default();
    let sha = "a".repeat(64);
    let msgs = run(
        &offline_client(),
        &fake,
        Effect::OpenBlob {
            sha256: sha.clone(),
            name: "a b.pdf".into(),
            mime: "application/pdf".into(),
        },
    )
    .await;
    assert!(msgs.is_empty());
    let opened = fake.opened.lock().unwrap();
    let (url, hint) = &opened[0];
    assert_eq!(url.path(), format!("/blob/{sha}"));
    assert_eq!(url.query(), Some("name=a+b.pdf"));
    assert_eq!(
        hint,
        &Some(BlobHint {
            name: "a b.pdf".into(),
            mime: "application/pdf".into(),
        })
    );
}

#[tokio::test]
async fn session_ok_reflects_whoami() {
    let (c, _seen) = serve(|_, _| ("200 OK", r#"{"ok":true,"ship":"~zod"}"#.into())).await;
    assert!(session_ok(&c).await);
    let (c, _seen) = serve(|_, _| ("401 Unauthorized", "no session".into())).await;
    assert!(!session_ok(&c).await);
}

#[test]
fn clock_helper_uses_local_offset() {
    // 2026-01-01T00:00:00Z is 1767225600.
    assert_eq!(now(1_767_225_600, 0).today.iso(), "2026-01-01");
    assert_eq!(now(1_767_225_600 - 1, 0).today.iso(), "2025-12-31");
    assert_eq!(now(1_767_225_600 - 1, 3600).today.iso(), "2026-01-01");
}

/// Compile-time check: the future of `run` is `Send` on native targets, so
/// shells can `tokio::spawn` it.
#[test]
fn run_future_is_send_on_native() {
    fn assert_send<T: Send>(_: &T) {}
    let client = offline_client();
    let fake = Fake::default();
    let fut = run(&client, &fake, Effect::LoadPhotos);
    assert_send(&fut);
}
