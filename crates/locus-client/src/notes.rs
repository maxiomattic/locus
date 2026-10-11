//! `%notes` API: notebooks, folders, notes, history and search.
//!
//! Writes answer with a `{requestId, body}` envelope whose `body.type` is one
//! of `ok`, `no-change`, `notebook`, `api-key`, `error`, `pending`. Errors with
//! `errorType == "conflict"` map to [`Error::Conflict`] whether they arrive as
//! HTTP 200 (the usual case) or as an HTTP error status.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Client, Error, Result, seg};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookSummary {
    pub host: String,
    pub flag_name: String,
    pub notebook: Notebook,
    #[serde(default)]
    pub visibility: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub id: u64,
    pub title: String,
    pub root_folder_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: u64,
    #[serde(default)]
    pub notebook_id: u64,
    pub name: String,
    #[serde(default)]
    pub parent_folder_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: u64,
    #[serde(default)]
    pub notebook_id: u64,
    #[serde(default)]
    pub folder_id: u64,
    pub title: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub body_md: String,
    pub revision: u64,
    #[serde(default)]
    pub updated_at: u64,
}

/// One archived revision from `GET .../notes/{id}/history` (newest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRevision {
    pub rev: u64,
    /// Unix seconds.
    pub at: u64,
    pub author: String,
    pub title: String,
    pub body_md: String,
}

/// One page of `search/bounded/text`. Pass `last` back as `from` until it is 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub last: u64,
    pub notes: Vec<Note>,
}

/// Write-side response body. `Other` absorbs `api-key` and any future type.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Reply {
    Ok {},
    NoChange {},
    Notebook {
        notebook: NotebookSummary,
    },
    Error {
        #[serde(rename = "errorType")]
        error_type: String,
        #[serde(default)]
        message: Vec<String>,
    },
    Pending {},
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
struct Envelope {
    #[serde(rename = "requestId", default)]
    request_id: Option<String>,
    body: Reply,
}

/// Turn a write envelope into a result. Pending writes return the request id
/// so the caller can poll.
fn check_reply(path: &str, env: Envelope) -> Result<Reply> {
    match env.body {
        Reply::Error { error_type, .. } if error_type == "conflict" => {
            Err(Error::Conflict { current: None })
        }
        Reply::Error {
            error_type,
            message,
        } => Err(Error::Api {
            error: error_type,
            message: message.join(" "),
        }),
        Reply::Pending {} => Err(Error::Pending(
            env.request_id.unwrap_or_else(|| path.to_string()),
        )),
        other => Ok(other),
    }
}

fn nb_path(host: &str, name: &str) -> String {
    format!("/notes/~/v1/notebooks/{}/{}", seg(host), seg(name))
}

fn note_path(host: &str, name: &str, id: u64) -> String {
    format!("{}/notes/{id}", nb_path(host, name))
}

impl Client {
    async fn write(&self, method: Method, path: &str, body: Option<Value>) -> Result<Reply> {
        let env: Envelope = self.json(method, path, body).await?;
        check_reply(path, env)
    }

    pub async fn list_notebooks(&self) -> Result<Vec<NotebookSummary>> {
        self.get_json("/notes/~/v1/notebooks").await
    }

    pub async fn create_notebook(&self, title: &str) -> Result<NotebookSummary> {
        match self
            .write(
                Method::POST,
                "/notes/~/v1/notebooks",
                Some(json!({ "title": title })),
            )
            .await?
        {
            Reply::Notebook { notebook } => Ok(notebook),
            other => Err(unexpected("create-notebook", other)),
        }
    }

    pub async fn list_folders(&self, host: &str, name: &str) -> Result<Vec<Folder>> {
        self.get_json(&format!("{}/folders", nb_path(host, name)))
            .await
    }

    pub async fn list_notes(&self, host: &str, name: &str) -> Result<Vec<Note>> {
        self.get_json(&format!("{}/notes", nb_path(host, name)))
            .await
    }

    pub async fn get_note(&self, host: &str, name: &str, id: u64) -> Result<Note> {
        self.get_json(&note_path(host, name, id)).await
    }

    /// Create a note and return it. The server's create reply does not carry
    /// a full note in a stable shape, so this lists the notebook afterwards
    /// and returns the newest note with that title.
    pub async fn create_note(
        &self,
        host: &str,
        name: &str,
        folder: u64,
        title: &str,
        body: &str,
    ) -> Result<Note> {
        self.write(
            Method::POST,
            &format!("{}/notes", nb_path(host, name)),
            Some(json!({ "folder": folder, "title": title, "body": body })),
        )
        .await?;
        self.list_notes(host, name)
            .await?
            .into_iter()
            .rev()
            .find(|n| n.title == title)
            .ok_or_else(|| Error::Api {
                error: "not-found".into(),
                message: "created note missing from list".into(),
            })
    }

    /// Content edit with optimistic concurrency. On a stale write this
    /// re-reads the note (best effort) and returns
    /// `Conflict { current: Some(rev) }` when that succeeds.
    ///
    /// The server does not return the new revision here; call
    /// [`Client::get_note`] if you need it.
    pub async fn save_note(
        &self,
        host: &str,
        name: &str,
        id: u64,
        body: &str,
        expected_revision: u64,
    ) -> Result<()> {
        let path = note_path(host, name, id);
        let res = self
            .write(
                Method::PUT,
                &path,
                Some(json!({ "body": body, "expectedRevision": expected_revision })),
            )
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(Error::Conflict { .. }) => {
                let current = self.get_note(host, name, id).await.ok().map(|n| n.revision);
                Err(Error::Conflict { current })
            }
            Err(e) => Err(e),
        }
    }

    /// Metadata edit. Sends `title` only: the server treats a request with
    /// `body` as a content edit and ignores the title.
    pub async fn rename_note(&self, host: &str, name: &str, id: u64, title: &str) -> Result<()> {
        self.write(
            Method::PUT,
            &note_path(host, name, id),
            Some(json!({ "title": title })),
        )
        .await
        .map(|_| ())
    }

    pub async fn delete_note(&self, host: &str, name: &str, id: u64) -> Result<()> {
        self.write(Method::DELETE, &note_path(host, name, id), None)
            .await
            .map(|_| ())
    }

    pub async fn history(&self, host: &str, name: &str, id: u64) -> Result<Vec<NoteRevision>> {
        self.get_json(&format!("{}/history", note_path(host, name, id)))
            .await
    }

    /// Bounded, resumable text search. `tries` is the number of notes examined
    /// per call (server default 100), not the number of hits.
    pub async fn search(
        &self,
        host: &str,
        name: &str,
        needle: &str,
        from: Option<u64>,
        tries: Option<u32>,
    ) -> Result<SearchPage> {
        let mut q = url::form_urlencoded::Serializer::new(String::new());
        q.append_pair("needle", needle);
        if let Some(from) = from {
            q.append_pair("from", &from.to_string());
        }
        if let Some(tries) = tries {
            q.append_pair("tries", &tries.to_string());
        }
        let path = format!("{}/search/bounded/text?{}", nb_path(host, name), q.finish());
        self.get_json(&path).await
    }

    /// Raw JSON of a request's current state (`GET .../request/{id}`), for
    /// polling writes that answered `pending`.
    pub async fn request_status(&self, request_id: &str) -> Result<Value> {
        self.get_json(&format!("/notes/~/v1/request/{}", seg(request_id)))
            .await
    }
}

fn unexpected(what: &str, reply: Reply) -> Error {
    Error::Api {
        error: "unexpected-reply".into(),
        message: format!("{what}: {reply:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(raw: &str) -> Envelope {
        serde_json::from_str(raw).expect("envelope decodes")
    }

    #[test]
    fn notebook_summary_decodes() {
        let raw = r#"{"host":"~zod","flagName":"my-notebook-1","visibility":"private","notebook":{"id":1,"title":"Notes","rootFolderId":2,"createdBy":"~zod","createdAt":1700000000,"updatedAt":1700000000,"updatedBy":"~zod"}}"#;
        let nb: NotebookSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(nb.host, "~zod");
        assert_eq!(nb.flag_name, "my-notebook-1");
        assert_eq!(nb.visibility, "private");
        assert_eq!(nb.notebook.root_folder_id, 2);
    }

    #[test]
    fn folder_decodes_with_null_parent() {
        let raw = r#"{"id":2,"notebookId":1,"name":"Inbox","parentFolderId":null,"createdBy":"~zod","createdAt":1,"updatedAt":1,"updatedBy":"~zod"}"#;
        let f: Folder = serde_json::from_str(raw).unwrap();
        assert_eq!(f.name, "Inbox");
        assert_eq!(f.parent_folder_id, None);
        let child = raw.replace("null", "2");
        let f: Folder = serde_json::from_str(&child).unwrap();
        assert_eq!(f.parent_folder_id, Some(2));
    }

    #[test]
    fn note_decodes_with_null_slug() {
        let raw = r#"{"id":4,"notebookId":1,"folderId":2,"title":"M3 Note","slug":null,"bodyMd":"first","createdBy":"~zod","createdAt":1,"updatedBy":"~zod","updatedAt":5,"revision":3}"#;
        let note: Note = serde_json::from_str(raw).unwrap();
        assert_eq!(note.id, 4);
        assert_eq!(note.folder_id, 2);
        assert_eq!(note.slug, None);
        assert_eq!(note.body_md, "first");
        assert_eq!(note.revision, 3);
    }

    #[test]
    fn history_decodes_newest_first_list() {
        let raw = r#"[{"rev":2,"at":1700000002,"author":"~zod","title":"T","bodyMd":"two"},{"rev":1,"at":1700000001,"author":"~zod","title":"T","bodyMd":"one"}]"#;
        let revs: Vec<NoteRevision> = serde_json::from_str(raw).unwrap();
        assert_eq!(revs[0].rev, 2);
        assert_eq!(revs[1].body_md, "one");
    }

    #[test]
    fn search_page_decodes_with_cursor() {
        let raw = r#"{"last":17,"notes":[{"id":20,"notebookId":1,"folderId":2,"title":"hit","slug":null,"bodyMd":"needle here","createdBy":"~zod","createdAt":1,"updatedBy":"~zod","updatedAt":1,"revision":0}]}"#;
        let page: SearchPage = serde_json::from_str(raw).unwrap();
        assert_eq!(page.last, 17);
        assert_eq!(page.notes.len(), 1);
        assert_eq!(page.notes[0].title, "hit");
    }

    #[test]
    fn ok_and_no_change_replies() {
        let ok = env(r#"{"requestId":"0v1","body":{"type":"ok","response":{"type":"update"}}}"#);
        assert!(matches!(check_reply("/p", ok), Ok(Reply::Ok {})));
        let nc = env(r#"{"requestId":"0v1","body":{"type":"no-change"}}"#);
        assert!(matches!(check_reply("/p", nc), Ok(Reply::NoChange {})));
    }

    #[test]
    fn notebook_reply_carries_summary() {
        let raw = r#"{"requestId":"0v2","body":{"type":"notebook","notebook":{"host":"~zod","flagName":"notes-1","visibility":"private","notebook":{"id":1,"title":"Notes","rootFolderId":2,"createdBy":"~zod","createdAt":1,"updatedAt":1,"updatedBy":"~zod"}}}}"#;
        match check_reply("/p", env(raw)) {
            Ok(Reply::Notebook { notebook }) => assert_eq!(notebook.flag_name, "notes-1"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn conflict_error_in_body_maps_to_conflict() {
        let raw =
            r#"{"requestId":"0v1","body":{"type":"error","errorType":"conflict","message":[]}}"#;
        assert!(matches!(
            check_reply("/p", env(raw)),
            Err(Error::Conflict { current: None })
        ));
    }

    #[test]
    fn other_errors_keep_type_and_message() {
        let raw = r#"{"requestId":"0v1","body":{"type":"error","errorType":"not-authorized","message":["no","write"]}}"#;
        match check_reply("/p", env(raw)) {
            Err(Error::Api { error, message }) => {
                assert_eq!(error, "not-authorized");
                assert_eq!(message, "no write");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn pending_reply_carries_request_id() {
        let raw = r#"{"requestId":"0v9","body":{"type":"pending","status":"sending"}}"#;
        match check_reply("/p", env(raw)) {
            Err(Error::Pending(id)) => assert_eq!(id, "0v9"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn api_key_reply_is_tolerated() {
        let raw = r#"{"requestId":"0v3","body":{"type":"api-key","apiKey":null}}"#;
        assert!(matches!(check_reply("/p", env(raw)), Ok(Reply::Other)));
    }

    #[test]
    fn paths_encode_host_and_name() {
        assert_eq!(
            note_path("~zod", "my-notebook-1", 7),
            "/notes/~/v1/notebooks/~zod/my-notebook-1/notes/7"
        );
    }
}
