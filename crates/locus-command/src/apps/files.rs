//! `locus files`: folders and files in the `%files` catalog.
//!
//! Folders are absolute (`/holidays/2026`). A file is named by its path
//! (`/holidays/pier.jpg`) or by its numeric id.

use locus_client::Client;
use locus_client::files::{FileEntry, FilePatch};
use locus_core::view_model::size_label;
use serde_json::json;

use super::Ctx;
use super::args::Parsed;
use super::fmt::{self, client_error, emit, one_line, stamp, table};

/// Help for `locus files --help`.
pub const HELP: &str = "locus files <command>

  ls [<dir>] [--json]               list a folder (default /)
  put <local file> [<dir>]          upload a file (default folder /)
  get <path-or-id> [-o <path>]      download (default: its name in the current folder; -o - for stdout)
  mkdir <dir>                       create a folder
  mv <path-or-id> <dest dir>        move a file to another folder
  rename <path-or-id> <new name>    rename a file
  rm <path-or-id>                   delete a file
  rmdir <dir>                       delete an empty folder

A file is given as /folder/name.ext or as its numeric id (shown by ls).
";

/// Run one `locus files` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "put" => put(ctx, args).await,
        "get" => get(ctx, args).await,
        "mkdir" => mkdir(ctx, args).await,
        "mv" => mv(ctx, args).await,
        "rename" => rename(ctx, args).await,
        "rm" => rm(ctx, args).await,
        "rmdir" => rmdir(ctx, args).await,
        other => Err(format!(
            "unknown files command {other}. try: locus files --help"
        )),
    }
}

/// A file as typed: by id, or by folder and name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Id(u64),
    Path { dir: String, name: String },
}

/// Normalise a folder: absolute (a missing leading `/` is added), no `.`,
/// `..` or empty segments.
pub fn normalize_dir(raw: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for seg in raw.trim().split('/').filter(|s| !s.is_empty()) {
        if seg == "." || seg == ".." {
            return Err(format!("folder cannot contain . or ..: {raw}"));
        }
        parts.push(seg);
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Read `s` as a numeric id, or else as a path to a file.
pub fn parse_target(s: &str) -> Result<Target, String> {
    let t = s.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
        return t
            .parse()
            .map(Target::Id)
            .map_err(|_| format!("bad file id: {s}"));
    }
    let (dir, name) = t.rsplit_once('/').unwrap_or(("", t));
    if name.is_empty() || name == "." || name == ".." {
        return Err(format!("not a file path: {s}"));
    }
    Ok(Target::Path {
        dir: normalize_dir(dir)?,
        name: name.to_string(),
    })
}

/// The immediate subfolders of `dir` among `all` folder paths.
pub fn child_dirs(all: &[String], dir: &str) -> Vec<String> {
    let prefix = if dir == "/" {
        "/".to_string()
    } else {
        format!("{dir}/")
    };
    let mut out: Vec<String> = all
        .iter()
        .filter_map(|p| p.strip_prefix(&prefix))
        .filter(|rest| !rest.is_empty() && !rest.contains('/'))
        .map(|rest| format!("{prefix}{rest}"))
        .collect();
    out.sort();
    out
}

/// The final segment of a folder path.
fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

async fn resolve(client: &Client, target: &str) -> Result<FileEntry, String> {
    match parse_target(target)? {
        Target::Id(id) => client.get_file(id).await.map_err(client_error),
        Target::Path { dir, name } => client
            .list_files(Some(&dir))
            .await
            .map_err(client_error)?
            .into_iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("no file {target}")),
    }
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let pos = p.positionals(0, 1, "locus files ls [<dir>] [--json]")?;
    let dir = normalize_dir(pos.first().map_or("/", String::as_str))?;
    let client = ctx.client().await?;
    let all = client.list_dirs().await.map_err(client_error)?;
    let files = client.list_files(Some(&dir)).await.map_err(client_error)?;
    let dirs = child_dirs(&all, &dir);
    if dir != "/" && files.is_empty() && dirs.is_empty() && !all.contains(&dir) {
        return Err(format!("no folder {dir}"));
    }
    if p.has("--json") {
        let files: Vec<_> = files
            .iter()
            .map(|f| {
                json!({"id": f.id, "path": path_of(f), "dir": f.dir, "name": f.name, "size": f.size,
                       "mime": f.mime, "created": f.created, "modified": f.modified,
                       "revision": f.revision})
            })
            .collect();
        fmt::json(&json!({"dir": dir, "dirs": dirs, "files": files}));
    } else {
        let mut rows: Vec<Vec<String>> = dirs
            .iter()
            .map(|d| {
                vec![
                    "-".into(),
                    String::new(),
                    String::new(),
                    format!("{}/", base(d)),
                ]
            })
            .collect();
        rows.extend(files.iter().map(|f| {
            vec![
                f.id.to_string(),
                size_label(f.size),
                stamp(f.modified, ctx.tz),
                one_line(&f.name),
            ]
        }));
        emit(&table(&["ID", "SIZE", "MODIFIED", "NAME"], &rows));
    }
    Ok(())
}

fn path_of(f: &FileEntry) -> String {
    if f.dir == "/" {
        format!("/{}", f.name)
    } else {
        format!("{}/{}", f.dir.trim_end_matches('/'), f.name)
    }
}

async fn put(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 2, "locus files put <local file> [<dir>]")?;
    let local = &pos[0];
    let dir = normalize_dir(pos.get(1).map_or("/", String::as_str))?;
    let name = std::path::Path::new(local)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{local}: not a file"))?;
    let bytes = std::fs::read(local).map_err(|e| format!("{local}: {e}"))?;
    let mime = mime_guess::from_path(&name)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let client = ctx.client().await?;
    if dir != "/" {
        client.create_dir(&dir).await.map_err(client_error)?;
    }
    let file = client
        .upload_file(&dir, &name, &mime, bytes)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\t{}\n", file.id, path_of(&file)));
    Ok(())
}

async fn get(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["-o", "--output"], &[])?;
    let target = &p.positionals(1, 1, "locus files get <path-or-id> [-o <path>]")?[0];
    let client = ctx.client().await?;
    let file = resolve(&client, target).await?;
    let bytes = client
        .download_blob(&file.sha256)
        .await
        .map_err(client_error)?;
    let out = p.value("-o").or(p.value("--output"));
    if out == Some("-") {
        use std::io::Write;
        let _ = std::io::stdout().write_all(&bytes);
        return Ok(());
    }
    let path = out.map_or_else(|| file.name.clone(), str::to_string);
    std::fs::write(&path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    emit(&format!("{path}\n"));
    Ok(())
}

async fn mkdir(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let dir = normalize_dir(&p.positionals(1, 1, "locus files mkdir <dir>")?[0])?;
    if dir == "/" {
        return Err("/ always exists".into());
    }
    ctx.client()
        .await?
        .create_dir(&dir)
        .await
        .map_err(client_error)?;
    emit(&format!("{dir}\n"));
    Ok(())
}

async fn mv(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(2, 2, "locus files mv <path-or-id> <dest dir>")?;
    let dest = normalize_dir(&pos[1])?;
    let client = ctx.client().await?;
    let file = resolve(&client, &pos[0]).await?;
    if dest != "/" {
        client.create_dir(&dest).await.map_err(client_error)?;
    }
    let patch = FilePatch {
        dir: Some(dest),
        name: None,
    };
    let moved = client
        .patch_file(file.id, file.revision, &patch)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", path_of(&moved)));
    Ok(())
}

async fn rename(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(2, 2, "locus files rename <path-or-id> <new name>")?;
    let new = pos[1].trim();
    if new.is_empty() || new.contains('/') || new == "." || new == ".." {
        return Err("a name cannot be empty or contain /".into());
    }
    let client = ctx.client().await?;
    let file = resolve(&client, &pos[0]).await?;
    let patch = FilePatch {
        dir: None,
        name: Some(new.to_string()),
    };
    let renamed = client
        .patch_file(file.id, file.revision, &patch)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", path_of(&renamed)));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let target = &p.positionals(1, 1, "locus files rm <path-or-id>")?[0];
    let client = ctx.client().await?;
    let file = resolve(&client, target).await?;
    client
        .delete_file(file.id, Some(file.revision))
        .await
        .map_err(client_error)?;
    emit(&format!("deleted {}\n", path_of(&file)));
    Ok(())
}

async fn rmdir(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let dir = normalize_dir(&p.positionals(1, 1, "locus files rmdir <dir>")?[0])?;
    if dir == "/" {
        return Err("cannot delete /".into());
    }
    ctx.client()
        .await?
        .delete_dir(&dir)
        .await
        .map_err(|e| match e {
            locus_client::Error::Conflict { .. } => format!("{dir} is not empty"),
            other => client_error(other),
        })?;
    emit(&format!("deleted {dir}\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_are_normalized() {
        assert_eq!(normalize_dir("/"), Ok("/".into()));
        assert_eq!(normalize_dir(""), Ok("/".into()));
        assert_eq!(normalize_dir("a//b/"), Ok("/a/b".into()));
        assert!(normalize_dir("/a/../b").is_err());
    }

    #[test]
    fn targets_are_ids_or_paths() {
        assert_eq!(parse_target("42"), Ok(Target::Id(42)));
        assert_eq!(
            parse_target("/a/b.txt"),
            Ok(Target::Path {
                dir: "/a".into(),
                name: "b.txt".into()
            })
        );
        assert_eq!(
            parse_target("b.txt"),
            Ok(Target::Path {
                dir: "/".into(),
                name: "b.txt".into()
            })
        );
        assert_eq!(
            parse_target("/42"),
            Ok(Target::Path {
                dir: "/".into(),
                name: "42".into()
            })
        );
        assert!(parse_target("/a/").is_err());
        assert!(parse_target("").is_err());
    }

    #[test]
    fn child_dirs_are_one_level_deep() {
        let all: Vec<String> = ["/", "/a", "/a/b", "/a/b/c", "/ab", "/z"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(child_dirs(&all, "/"), ["/a", "/ab", "/z"]);
        assert_eq!(child_dirs(&all, "/a"), ["/a/b"]);
        assert!(child_dirs(&all, "/z").is_empty());
        assert_eq!(base("/a/b"), "b");
    }
}
