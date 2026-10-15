//! Shared IO layer for the Locus apps.
//!
//! [`run`] performs one [`locus_core::Effect`] with a [`locus_client::Client`]
//! and returns the [`locus_core::Msg`]s the core expects back. Shells loop:
//! `update(&mut app, msg, now)` returns effects, each effect goes through
//! [`run`], and the returned messages go back into `update`.
//!
//! Effects the shell handles itself (nothing here, nothing to send):
//!
//! - [`Effect::CopyToClipboard`]: needs the window system, so [`run`] returns
//!   no messages and the shell copies the text.
//! - [`Effect::OpenSection`]: the shell opens or focuses that section's own
//!   window (desktop) or tab (web).
//!
//! Everything else is performed here. [`Effect::OpenBlob`] and
//! [`Effect::UploadFiles`] go through the [`Platform`] trait because they need
//! a shell capability (a file picker, a viewer). They return no message on
//! success (open) or a [`locus_core::Msg::UploadDone`] (upload).
//!
//! Thumbnails are not effects. Shells call [`thumb`] directly, and [`session_ok`]
//! answers "is the session still valid".
//!
//! # Send on native, not on wasm
//!
//! `reqwest` futures are `Send` on native and `!Send` on wasm (they wrap
//! `JsFuture`). The crate therefore uses marker traits that resolve per target:
//!
//! - native: [`Platform`] requires `Send + Sync`, and the futures it returns
//!   and the future of [`run`] are `Send`, so `tokio::spawn` works.
//! - wasm32: no bounds at all. Implementations may hold `Rc` and JS handles.
//!
//! Implement [`Platform`] normally; the bounds are blanket-implemented.

use std::future::Future;

use locus_client::Client;
use locus_core::{Effect, FileEntry, Msg, Now, Section};
use url::Url;

/// `Send + Sync` on native targets, nothing on wasm32. Used as a supertrait
/// of [`Platform`] and blanket-implemented, so you never implement it.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync + ?Sized> MaybeSendSync for T {}

/// See [`MaybeSendSync`].
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T: ?Sized> MaybeSendSync for T {}

/// `Send` on native targets, nothing on wasm32. Bounds the futures returned by
/// [`Platform`] methods; blanket-implemented.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

/// See [`MaybeSend`].
#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// A file the user picked for upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickedFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// What the shell may use to name or type a blob it opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobHint {
    pub name: String,
    pub mime: String,
}

/// Capabilities only the shell has. Implement once per shell.
pub trait Platform: MaybeSendSync {
    /// Sleep for debounce timers.
    fn sleep(&self, ms: u64) -> impl Future<Output = ()> + MaybeSend;

    /// Let the user pick files to upload. An empty result means cancelled.
    fn pick_files(&self) -> impl Future<Output = Vec<PickedFile>> + MaybeSend;

    /// Show or download a blob. Desktop: save to cache and open with the default
    /// app. Browser: open `url` in a new tab. `bytes_hint` names the blob;
    /// [`run`] always passes `Some`. `Err` carries a message for the user.
    fn open_blob(
        &self,
        url: Url,
        bytes_hint: Option<BlobHint>,
    ) -> impl Future<Output = Result<(), String>> + MaybeSend;
}

/// Build the core's clock from unix seconds and a local offset
/// (`local = utc + offset`).
pub fn now(unix_secs: i64, tz_offset_secs: i32) -> Now {
    Now {
        unix_secs,
        today: locus_core::date::local_date_of(unix_secs, tz_offset_secs),
        tz_offset_secs,
    }
}

/// Fetch a thumbnail. Not an effect: shells call this directly.
pub async fn thumb(client: &Client, sha256: &str, size: u32) -> Result<Vec<u8>, String> {
    client
        .download_thumb(sha256, size)
        .await
        .map_err(|e| describe(&e))
}

/// `true` when `whoami` succeeds, i.e. the session cookie or API key is accepted.
pub async fn session_ok(client: &Client) -> bool {
    client.whoami().await.is_ok()
}

/// Perform one effect and return the messages the core expects back, in order.
pub async fn run<P: Platform>(client: &Client, platform: &P, effect: Effect) -> Vec<Msg> {
    match effect {
        // Shell-handled: nothing to do here.
        Effect::CopyToClipboard(_) | Effect::OpenSection(..) => Vec::new(),

        Effect::LoadNotebooks => match client.list_notebooks().await {
            Ok(notebooks) => vec![Msg::NotebooksLoaded { notebooks }],
            Err(e) => vec![load_failed(Section::Notes, &e)],
        },
        Effect::CreateNotebook { title } => match client.create_notebook(&title).await {
            Ok(notebook) => vec![Msg::NotebookCreated { notebook }],
            Err(e) => vec![write_failed(Section::Notes, &e)],
        },
        Effect::LoadNotes { nb } => match client.list_notes(&nb.host, &nb.flag_name).await {
            Ok(notes) => vec![Msg::NotesLoaded { nb, notes }],
            Err(e) => vec![load_failed(Section::Notes, &e)],
        },
        Effect::LoadFolders { nb } => match client.list_folders(&nb.host, &nb.flag_name).await {
            Ok(folders) => vec![Msg::FoldersLoaded { nb, folders }],
            Err(e) => vec![load_failed(Section::Notes, &e)],
        },
        Effect::LoadNote { nb, id } => match client.get_note(&nb.host, &nb.flag_name, id).await {
            Ok(note) => vec![Msg::NoteLoaded { nb, note }],
            Err(e) => vec![load_failed(Section::Notes, &e)],
        },
        Effect::SaveNote {
            nb,
            id,
            body,
            expected_revision,
        } => {
            match client
                .save_note(&nb.host, &nb.flag_name, id, &body, expected_revision)
                .await
            {
                // save_note does not return the new revision; re-read so the
                // core gets the server's revision and body.
                Ok(()) => match client.get_note(&nb.host, &nb.flag_name, id).await {
                    Ok(note) => vec![Msg::NoteSaved { nb, note }],
                    Err(e) => vec![Msg::SaveFailed {
                        nb,
                        id,
                        error: format!("saved, but re-reading the note failed: {}", describe(&e)),
                    }],
                },
                // The client already re-read the note to fill `current`.
                Err(locus_client::Error::Conflict { current }) => {
                    vec![Msg::SaveConflict {
                        nb,
                        id,
                        body,
                        current,
                    }]
                }
                Err(e) => vec![Msg::SaveFailed {
                    nb,
                    id,
                    error: describe(&e),
                }],
            }
        }
        Effect::CreateNote {
            nb,
            folder,
            title,
            body,
        } => match client
            .create_note(&nb.host, &nb.flag_name, folder, &title, &body)
            .await
        {
            Ok(note) => vec![Msg::NoteCreated { nb, note }],
            Err(e) => vec![write_failed(Section::Notes, &e)],
        },
        Effect::RenameNote { nb, id, title } => {
            match client
                .rename_note(&nb.host, &nb.flag_name, id, &title)
                .await
            {
                Ok(()) => vec![Msg::NoteRenamed { nb, id, title }],
                Err(e) => vec![write_failed(Section::Notes, &e)],
            }
        }
        Effect::DeleteNote { nb, id } => {
            match client.delete_note(&nb.host, &nb.flag_name, id).await {
                Ok(()) => vec![Msg::NoteDeleted { nb, id }],
                Err(e) => vec![write_failed(Section::Notes, &e)],
            }
        }

        Effect::LoadEvents => match client.list_events().await {
            Ok(events) => vec![Msg::EventsLoaded { events }],
            Err(e) => vec![load_failed(Section::Calendar, &e)],
        },
        Effect::CreateEvent {
            title,
            start,
            end,
            body,
        } => match client.create_event(&title, start, end, &body).await {
            Ok(event) => vec![Msg::EventSaved { event }],
            Err(e) => vec![write_failed(Section::Calendar, &e)],
        },
        Effect::PatchEvent {
            id,
            expected_revision,
            patch,
        } => match client.patch_event(id, expected_revision, &patch).await {
            Ok(event) => vec![Msg::EventSaved { event }],
            Err(e) => vec![write_failed(Section::Calendar, &e)],
        },
        // The core does not carry a revision for deletes, so none is sent.
        Effect::DeleteEvent { id } => match client.delete_event(id, None).await {
            Ok(()) => vec![Msg::EventDeleted { id }],
            Err(e) => vec![write_failed(Section::Calendar, &e)],
        },

        Effect::LoadPhotos => match client.list_photos().await {
            Ok(photos) => vec![Msg::PhotosLoaded { photos }],
            Err(e) => vec![load_failed(Section::Photos, &e)],
        },
        Effect::UploadFiles { dir } => upload(client, platform, dir).await,

        Effect::LoadContacts => match client.list_contacts().await {
            Ok(contacts) => vec![Msg::ContactsLoaded { contacts }],
            Err(e) => vec![load_failed(Section::Contacts, &e)],
        },
        Effect::CreateContact { name, email, ship } => {
            match client.create_contact(&name, &email, &ship).await {
                Ok(contact) => vec![Msg::ContactSaved { contact }],
                Err(e) => vec![write_failed(Section::Contacts, &e)],
            }
        }
        Effect::PatchContact {
            id,
            expected_revision,
            patch,
        } => match client.patch_contact(id, expected_revision, &patch).await {
            Ok(contact) => vec![Msg::ContactSaved { contact }],
            Err(e) => vec![write_failed(Section::Contacts, &e)],
        },
        Effect::DeletePhoto { id } => match client.delete_photo(id, None).await {
            Ok(()) => vec![Msg::PhotoDeleted { id }],
            Err(e) => vec![write_failed(Section::Photos, &e)],
        },
        Effect::CreateNoteFolder { nb, parent, name } => {
            match client.create_folder(&nb.host, &nb.flag_name, parent, &name).await {
                Ok(_) => match client.list_folders(&nb.host, &nb.flag_name).await {
                    Ok(folders) => vec![Msg::FoldersLoaded { nb, folders }],
                    Err(e) => vec![load_failed(Section::Notes, &e)],
                },
                Err(e) => vec![write_failed(Section::Notes, &e)],
            }
        }
        Effect::DeleteContact { id } => match client.delete_contact(id, None).await {
            Ok(()) => vec![Msg::ContactDeleted { id }],
            Err(e) => vec![write_failed(Section::Contacts, &e)],
        },

        Effect::LoadFiles => match client.list_files(None).await {
            Ok(files) => vec![Msg::FilesLoaded {
                files: files.into_iter().map(to_core_file).collect(),
            }],
            Err(e) => vec![load_failed(Section::Files, &e)],
        },
        Effect::LoadDirs => match client.list_dirs().await {
            Ok(dirs) => vec![Msg::DirsLoaded { dirs }],
            Err(e) => vec![load_failed(Section::Files, &e)],
        },
        Effect::CreateDir { path } => match client.create_dir(&path).await {
            Ok(()) => vec![Msg::DirCreated { path }],
            Err(e) => vec![write_failed(Section::Files, &e)],
        },
        Effect::PatchFile {
            id,
            expected_revision,
            patch,
        } => {
            let patch = locus_client::files::FilePatch {
                name: patch.name,
                dir: patch.dir,
            };
            match client.patch_file(id, expected_revision, &patch).await {
                Ok(file) => vec![Msg::FilePatched {
                    file: to_core_file(file),
                }],
                Err(e) => vec![write_failed(Section::Files, &e)],
            }
        }
        Effect::DeleteFile { id } => match client.delete_file(id, None).await {
            Ok(()) => vec![Msg::FileDeleted { id }],
            Err(e) => vec![write_failed(Section::Files, &e)],
        },
        Effect::OpenBlob { sha256, name, mime } => {
            let url = match client.blob_url(&sha256, Some(&name)) {
                Ok(url) => url,
                Err(e) => return vec![write_failed(Section::Files, &e)],
            };
            match platform.open_blob(url, Some(BlobHint { name, mime })).await {
                Ok(()) => Vec::new(),
                Err(error) => vec![Msg::WriteFailed {
                    section: Section::Files,
                    error,
                }],
            }
        }

        Effect::ScheduleSave { after_ms, token } => {
            platform.sleep(u64::from(after_ms)).await;
            vec![Msg::SaveTimer(token)]
        }
        Effect::Login { code } => match client.login(&code).await {
            Ok(()) => vec![Msg::LoggedIn],
            Err(e) => vec![Msg::LoginFailed {
                error: describe(&e),
            }],
        },
    }
}

/// `UploadFiles`: ask the platform for files, then upload each one. Paths
/// under `/Photos` go to the photo catalog (album = the path below `/Photos`),
/// everything else to `%files` in `dir`.
///
/// On a failure the upload stops: a `WriteFailed` reports it, and `UploadDone`
/// is sent only when some files were stored, so the core reloads the section.
async fn upload<P: Platform>(client: &Client, platform: &P, dir: String) -> Vec<Msg> {
    let section = if is_photos_dir(&dir) {
        Section::Photos
    } else {
        Section::Files
    };
    let picked = platform.pick_files().await;
    let total = picked.len();
    let mut count = 0;
    for file in picked {
        let stored = if section == Section::Photos {
            let album = dir.strip_prefix("/Photos").unwrap_or("").trim_matches('/');
            client
                .upload_photo(album, &file.name, file.bytes)
                .await
                .map(|_| ())
        } else {
            client
                .upload_file(&dir, &file.name, &file.mime, file.bytes)
                .await
                .map(|_| ())
        };
        if let Err(e) = stored {
            let mut out = vec![Msg::WriteFailed {
                section,
                error: format!("{} (stored {count} of {total} file(s))", describe(&e)),
            }];
            if count > 0 {
                out.push(Msg::UploadDone { dir, count });
            }
            return out;
        }
        count += 1;
    }
    vec![Msg::UploadDone { dir, count }]
}

/// `/Photos` and `/Photos/...`, but not `/PhotosBackup`.
fn is_photos_dir(dir: &str) -> bool {
    dir == "/Photos" || dir.starts_with("/Photos/")
}

fn to_core_file(f: locus_client::files::FileEntry) -> FileEntry {
    FileEntry {
        id: f.id,
        dir: f.dir,
        name: f.name,
        sha256: f.sha256,
        size: f.size,
        mime: f.mime,
        modified: f.modified,
        revision: f.revision,
    }
}

fn load_failed(section: Section, e: &locus_client::Error) -> Msg {
    Msg::LoadFailed {
        section,
        error: describe(e),
    }
}

fn write_failed(section: Section, e: &locus_client::Error) -> Msg {
    Msg::WriteFailed {
        section,
        error: describe(e),
    }
}

/// A short message for the user. Raw bodies and transport details stay out.
pub fn describe(e: &locus_client::Error) -> String {
    use locus_client::Error;
    match e {
        Error::Http { status, .. } => format!("server returned HTTP {status}"),
        Error::Conflict { .. } => "changed elsewhere; reload and try again".into(),
        Error::NotFound { .. } => "not found".into(),
        Error::Api { error, message } if message.is_empty() => error.clone(),
        Error::Api { message, .. } => message.clone(),
        Error::Pending(_) => "still pending on the server".into(),
        Error::Decode { .. } => "unexpected response from the server".into(),
        Error::Network(_) => "could not reach the server".into(),
        Error::BadPath(_) => "invalid path".into(),
        Error::Url(_) => "invalid URL".into(),
    }
}
