//! The shell's half of the effect contract: timers, the file picker, opening
//! blobs, and thumbnail decoding.

use std::path::PathBuf;
use std::time::Duration;

use locus_client::Client;
use locus_effects::{BlobHint, PickedFile, Platform};
use url::Url;

/// Desktop capabilities for [`locus_effects::run`].
pub struct Desk {
    pub client: Client,
    pub cache: PathBuf,
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("locus")
        .join("blobs")
}

impl Platform for Desk {
    async fn sleep(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    async fn pick_files(&self) -> Vec<PickedFile> {
        let Some(handles) = rfd::AsyncFileDialog::new()
            .set_title("Upload to Locus")
            .pick_files()
            .await
        else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(handles.len());
        for handle in handles {
            let name = handle.file_name();
            let mime = mime_guess::from_path(&name)
                .first_or_octet_stream()
                .essence_str()
                .to_string();
            let bytes = handle.read().await;
            out.push(PickedFile { name, mime, bytes });
        }
        out
    }

    /// Download the blob to `cache/<sha>/<name>` (unless already there) and
    /// hand the file to the desktop's default application.
    async fn open_blob(&self, url: Url, hint: Option<BlobHint>) -> Result<(), String> {
        let sha = url
            .path_segments()
            .and_then(|segments| segments.filter(|s| !s.is_empty()).nth(1))
            .ok_or("bad blob URL")?
            .to_string();
        let raw = hint.map(|h| h.name).unwrap_or_default();
        let name = std::path::Path::new(&raw)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .unwrap_or("blob")
            .to_string();
        let dir = self.cache.join(&sha);
        let path = dir.join(&name);
        if !path.is_file() {
            let bytes = self
                .client
                .download_blob(&sha)
                .await
                .map_err(|e| locus_effects::describe(&e))?;
            tokio::fs::create_dir_all(&dir)
                .await
                .map_err(|e| e.to_string())?;
            tokio::fs::write(&path, bytes)
                .await
                .map_err(|e| e.to_string())?;
        }
        open::that_detached(&path).map_err(|e| e.to_string())
    }
}

/// Decode a thumbnail (JPEG, PNG, WebP or GIF) into an RGBA image handle.
pub fn decode(bytes: &[u8]) -> Result<cosmic::widget::image::Handle, String> {
    let rgba = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(cosmic::widget::image::Handle::from_rgba(
        width,
        height,
        rgba.into_raw(),
    ))
}
