//! `locus photos`: the photo catalog and photo bytes.

use locus_client::blobs::photo_blob;
use locus_client::photos::{Photo, PhotoPatch};
use serde_json::json;

use super::Ctx;
use super::args::{Parsed, parse_id};
use super::fmt::{self, client_error, emit, one_line, table};

/// Help for `locus photos --help`.
pub const HELP: &str = "locus photos <command>

  ls [--json]                          list photos
  upload <file>... [--album <album>]   upload image files; prints the new ids
  get <id> [-o <path>]                 download the original (-o - for stdout)
  edit <id> [--title <t>] [--album <a>] change a photo's title or album
  rm <id>                              delete a photo
";

/// Run one `locus photos` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "upload" => upload(ctx, args).await,
        "get" => get(ctx, args).await,
        "edit" => edit(ctx, args).await,
        "rm" => rm(ctx, args).await,
        other => Err(format!(
            "unknown photos command {other}. try: locus photos --help"
        )),
    }
}

/// The file extension (with dot) for common image bytes, from their header.
pub fn sniff_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(".png"),
        [0xFF, 0xD8, 0xFF, ..] => Some(".jpg"),
        [b'G', b'I', b'F', b'8', ..] => Some(".gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(".webp"),
        _ => None,
    }
}

/// Where `get` writes by default: the photo's title as a bare file name, with
/// an extension sniffed from the bytes when the title has none.
pub fn download_name(photo: &Photo, bytes: &[u8]) -> String {
    let base = photo
        .title
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('.');
    let mut name = if base.is_empty() {
        format!("photo-{}", photo.id)
    } else {
        base.to_string()
    };
    if !name.contains('.')
        && let Some(ext) = sniff_ext(bytes)
    {
        name.push_str(ext);
    }
    name
}

async fn find(client: &locus_client::Client, id: u64) -> Result<Photo, String> {
    client
        .list_photos()
        .await
        .map_err(client_error)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no photo {id}"))
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    p.positionals(0, 0, "locus photos ls [--json]")?;
    let photos = ctx
        .client()
        .await?
        .list_photos()
        .await
        .map_err(client_error)?;
    if p.has("--json") {
        let out: Vec<_> = photos
            .iter()
            .map(|p| {
                json!({"id": p.id, "album": p.album, "title": p.title, "path": p.path,
                       "tags": p.tags, "revision": p.revision})
            })
            .collect();
        fmt::json(&out);
    } else {
        let rows: Vec<Vec<String>> = photos
            .iter()
            .map(|p| vec![p.id.to_string(), p.album.clone(), one_line(&p.title)])
            .collect();
        emit(&table(&["ID", "ALBUM", "TITLE"], &rows));
    }
    Ok(())
}

async fn upload(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["--album"], &[])?;
    let files = p.positionals(
        1,
        usize::MAX,
        "locus photos upload <file>... [--album <album>]",
    )?;
    // Read everything first so a typo does not leave a partial upload.
    let mut loaded = Vec::new();
    for path in files {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        let title = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{path}: not a file"))?;
        loaded.push((title, bytes));
    }
    let client = ctx.client().await?;
    let album = p.value("--album").unwrap_or("").trim_matches('/');
    for (title, bytes) in loaded {
        let photo = client
            .upload_photo(album, &title, bytes)
            .await
            .map_err(client_error)?;
        emit(&format!("{}\t{title}\n", photo.id));
    }
    Ok(())
}

async fn get(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["-o", "--output"], &[])?;
    let id = parse_id(
        &p.positionals(1, 1, "locus photos get <id> [-o <path>]")?[0],
        "photo",
    )?;
    let client = ctx.client().await?;
    let photo = find(&client, id).await?;
    let sha = photo_blob(&photo)
        .ok_or_else(|| format!("photo {id} is stored at {}, not on the gateway", photo.path))?;
    let bytes = client.download_blob(sha).await.map_err(client_error)?;
    let out = p.value("-o").or(p.value("--output"));
    if out == Some("-") {
        use std::io::Write;
        let _ = std::io::stdout().write_all(&bytes);
        return Ok(());
    }
    let path = out.map_or_else(|| download_name(&photo, &bytes), str::to_string);
    std::fs::write(&path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    emit(&format!("{path}\n"));
    Ok(())
}

async fn edit(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let usage = "locus photos edit <id> [--title <t>] [--album <a>]";
    let p = Parsed::parse(&args, &["--title", "--album"], &[])?;
    let id = parse_id(&p.positionals(1, 1, usage)?[0], "photo")?;
    let patch = PhotoPatch {
        title: p.value("--title").map(str::to_string),
        album: p.value("--album").map(|a| a.trim_matches('/').to_string()),
        ..Default::default()
    };
    if patch == PhotoPatch::default() {
        return Err(format!("nothing to change\nusage: {usage}"));
    }
    let client = ctx.client().await?;
    let photo = find(&client, id).await?;
    client
        .patch_photo(id, photo.revision, &patch)
        .await
        .map_err(client_error)?;
    emit(&format!("updated {id}\n"));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let id = parse_id(&p.positionals(1, 1, "locus photos rm <id>")?[0], "photo")?;
    ctx.client()
        .await?
        .delete_photo(id, None)
        .await
        .map_err(client_error)?;
    emit(&format!("deleted {id}\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(title: &str) -> Photo {
        Photo {
            id: 7,
            album: String::new(),
            title: title.into(),
            path: "sha256:ab".into(),
            tags: vec![],
            revision: 1,
        }
    }

    #[test]
    fn sniffs_common_formats() {
        assert_eq!(sniff_ext(&[0x89, b'P', b'N', b'G', 0]), Some(".png"));
        assert_eq!(sniff_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(".jpg"));
        assert_eq!(sniff_ext(b"RIFF\0\0\0\0WEBPVP8"), Some(".webp"));
        assert_eq!(sniff_ext(b"hello"), None);
    }

    #[test]
    fn download_name_is_a_safe_basename() {
        assert_eq!(download_name(&photo("pier.jpg"), b""), "pier.jpg");
        assert_eq!(download_name(&photo("../../etc/pier.jpg"), b""), "pier.jpg");
        assert_eq!(download_name(&photo(""), b""), "photo-7");
        assert_eq!(
            download_name(&photo("sunset"), &[0xFF, 0xD8, 0xFF, 0]),
            "sunset.jpg"
        );
        assert_eq!(download_name(&photo("sunset"), b"zz"), "sunset");
    }
}
