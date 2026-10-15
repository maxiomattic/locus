//! `locus notes`: notebooks, folders and notes.
//!
//! A note is addressed as `<notebook>:<id>` where `<notebook>` is the
//! notebook's flag name. A notebook argument may be its flag name or its
//! title (case-insensitive).

use locus_client::notes::{Folder, Note, NotebookSummary};
use locus_client::{Client, Error};
use serde_json::json;

use super::args::{Parsed, parse_id};
use super::fmt::{self, client_error, emit, one_line, stamp, table};
use super::{Ctx, edit_in_editor, piped_stdin};

/// Help for `locus notes --help`.
pub const HELP: &str = "locus notes <command>

  notebooks [--json]                       list notebooks
  notebook-new <title>                     create a notebook
  ls [<notebook>[/<folder path>]] [--json] list notes (all notebooks by default)
  show <ref> [--json]                      print a note's markdown body
  new <notebook> [--folder <path>] <title> create a note; body from stdin if piped
  edit <ref>                               replace the body from stdin, or in $EDITOR
  rename <ref> <title>                     change a note's title
  rm <ref>                                 delete a note
  history <ref> [--json]                   list a note's saved revisions
  search <query> [--json]                  search note text in all notebooks
  mkdir <notebook> <folder path>           create folders (like mkdir -p)

A <ref> is <notebook>:<id>, as printed by ls and search.
";

/// Run one `locus notes` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "notebooks" => notebooks(ctx, args).await,
        "notebook-new" => notebook_new(ctx, args).await,
        "ls" => ls(ctx, args).await,
        "show" => show(ctx, args).await,
        "new" => new(ctx, args).await,
        "edit" => edit(ctx, args).await,
        "rename" => rename(ctx, args).await,
        "rm" => rm(ctx, args).await,
        "history" => history(ctx, args).await,
        "search" => search(ctx, args).await,
        "mkdir" => mkdir(ctx, args).await,
        other => Err(format!(
            "unknown notes command {other}. try: locus notes --help"
        )),
    }
}

/// The notebook named by `spec`: an exact flag name, else a flag name or
/// title ignoring case. More than one match is an error.
pub fn find_notebook<'a>(
    books: &'a [NotebookSummary],
    spec: &str,
) -> Result<&'a NotebookSummary, String> {
    if let Some(nb) = books.iter().find(|b| b.flag_name == spec) {
        return Ok(nb);
    }
    let want = spec.to_lowercase();
    let hits: Vec<&NotebookSummary> = books
        .iter()
        .filter(|b| b.flag_name.to_lowercase() == want || b.notebook.title.to_lowercase() == want)
        .collect();
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("no notebook {spec}")),
        many => {
            let names: Vec<&str> = many.iter().map(|b| b.flag_name.as_str()).collect();
            Err(format!(
                "notebook {spec} is ambiguous; use one of: {}",
                names.join(", ")
            ))
        }
    }
}

/// Split `<notebook>/<folder path>` into the notebook and the folder names.
/// The whole text is tried as a notebook first, so titles may contain `/`.
pub fn locate<'a>(
    books: &'a [NotebookSummary],
    spec: &str,
) -> Result<(&'a NotebookSummary, Vec<String>), String> {
    match find_notebook(books, spec) {
        Ok(nb) => return Ok((nb, Vec::new())),
        Err(e) if !spec.contains('/') || e.contains("ambiguous") => return Err(e),
        Err(_) => {}
    }
    let (name, path) = spec.split_once('/').unwrap_or((spec, ""));
    Ok((find_notebook(books, name)?, folder_names(path)))
}

/// The non-empty segments of a folder path.
pub fn folder_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Split a note ref into its notebook name and note id.
pub fn parse_note_ref(s: &str) -> Result<(&str, u64), String> {
    let (nb, id) = s
        .rsplit_once(':')
        .filter(|(nb, _)| !nb.is_empty())
        .ok_or_else(|| format!("not a note ref (want <notebook>:<id>): {s}"))?;
    Ok((nb, parse_id(id, "note")?))
}

/// The printed form of a note's ref.
pub fn note_ref(nb: &NotebookSummary, id: u64) -> String {
    format!("{}:{id}", nb.flag_name)
}

/// The id of the folder at `names` below `root`, walking one level per name.
pub fn resolve_folder(folders: &[Folder], root: u64, names: &[String]) -> Result<u64, String> {
    let mut cur = root;
    for name in names {
        cur = folders
            .iter()
            .find(|f| f.parent_folder_id == Some(cur) && &f.name == name)
            .map(|f| f.id)
            .ok_or_else(|| format!("no folder {}", names.join("/")))?;
    }
    Ok(cur)
}

/// The `/`-joined path of folder `id` below `root` (empty for the root).
pub fn folder_path(folders: &[Folder], root: u64, id: u64) -> String {
    let mut names = Vec::new();
    let mut cur = id;
    while cur != root {
        let Some(f) = folders.iter().find(|f| f.id == cur) else {
            break;
        };
        names.push(f.name.as_str());
        match f.parent_folder_id {
            Some(p) => cur = p,
            None => break,
        }
    }
    names.reverse();
    names.join("/")
}

/// A notebook list plus the notebook named by a note ref.
async fn note_target(client: &Client, note_ref: &str) -> Result<(NotebookSummary, u64), String> {
    let (nb, id) = parse_note_ref(note_ref)?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    Ok((find_notebook(&books, nb)?.clone(), id))
}

/// Resolve a note ref to its notebook and the note itself.
pub async fn fetch_note(
    client: &Client,
    note_ref: &str,
) -> Result<(NotebookSummary, Note), String> {
    let (nb, id) = note_target(client, note_ref).await?;
    let note = client
        .get_note(&nb.host, &nb.flag_name, id)
        .await
        .map_err(client_error)?;
    Ok((nb, note))
}

async fn notebooks(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    p.positionals(0, 0, "locus notes notebooks [--json]")?;
    let books = ctx
        .client()
        .await?
        .list_notebooks()
        .await
        .map_err(client_error)?;
    if p.has("--json") {
        let items: Vec<_> = books
            .iter()
            .map(|b| json!({"name": b.flag_name, "title": b.notebook.title, "id": b.notebook.id, "host": b.host}))
            .collect();
        fmt::json(&items);
    } else {
        let rows: Vec<Vec<String>> = books
            .iter()
            .map(|b| vec![b.flag_name.clone(), b.notebook.title.clone()])
            .collect();
        emit(&table(&["NOTEBOOK", "TITLE"], &rows));
    }
    Ok(())
}

async fn notebook_new(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let title = &p.positionals(1, 1, "locus notes notebook-new <title>")?[0];
    let nb = ctx
        .client()
        .await?
        .create_notebook(title)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", nb.flag_name));
    Ok(())
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let pos = p.positionals(0, 1, "locus notes ls [<notebook>[/<folder path>]] [--json]")?;
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let mut items = Vec::new();
    let targets: Vec<(&NotebookSummary, Vec<String>)> = match pos.first() {
        Some(spec) => vec![locate(&books, spec)?],
        None => books.iter().map(|b| (b, Vec::new())).collect(),
    };
    for (nb, names) in targets {
        let folders = client
            .list_folders(&nb.host, &nb.flag_name)
            .await
            .map_err(client_error)?;
        let root = nb.notebook.root_folder_id;
        let only = if names.is_empty() {
            None
        } else {
            Some(resolve_folder(&folders, root, &names)?)
        };
        let mut notes = client
            .list_notes(&nb.host, &nb.flag_name)
            .await
            .map_err(client_error)?;
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        for n in notes
            .iter()
            .filter(|n| only.is_none_or(|f| n.folder_id == f))
        {
            items.push((
                note_ref(nb, n.id),
                nb.notebook.title.clone(),
                folder_path(&folders, root, n.folder_id),
                n.clone(),
            ));
        }
    }
    if p.has("--json") {
        let out: Vec<_> = items
            .iter()
            .map(|(r, nb, folder, n)| {
                json!({"ref": r, "notebook": nb, "folder": folder, "title": n.title,
                       "revision": n.revision, "updatedAt": n.updated_at})
            })
            .collect();
        fmt::json(&out);
    } else {
        let rows: Vec<Vec<String>> = items
            .iter()
            .map(|(r, _, folder, n)| {
                vec![
                    r.clone(),
                    stamp(n.updated_at, ctx.tz),
                    folder.clone(),
                    one_line(&n.title),
                ]
            })
            .collect();
        emit(&table(&["REF", "UPDATED", "FOLDER", "TITLE"], &rows));
    }
    Ok(())
}

async fn show(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let r = &p.positionals(1, 1, "locus notes show <ref> [--json]")?[0];
    let (nb, note) = fetch_note(&ctx.client().await?, r).await?;
    if p.has("--json") {
        fmt::json(&json!({"ref": note_ref(&nb, note.id), "title": note.title,
            "revision": note.revision, "updatedAt": note.updated_at, "body": note.body_md}));
    } else {
        emit(&note.body_md);
        if !note.body_md.ends_with('\n') && !note.body_md.is_empty() {
            emit("\n");
        }
    }
    Ok(())
}

async fn new(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let usage = "locus notes new <notebook> [--folder <path>] <title>";
    let p = Parsed::parse(&args, &["--folder"], &[])?;
    let pos = p.positionals(2, 2, usage)?;
    let body = piped_stdin()?.unwrap_or_default();
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let nb = find_notebook(&books, &pos[0])?;
    let folder = match p.value("--folder") {
        Some(path) => {
            let folders = client
                .list_folders(&nb.host, &nb.flag_name)
                .await
                .map_err(client_error)?;
            resolve_folder(&folders, nb.notebook.root_folder_id, &folder_names(path))?
        }
        None => nb.notebook.root_folder_id,
    };
    let note = client
        .create_note(&nb.host, &nb.flag_name, folder, &pos[1], &body)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", note_ref(nb, note.id)));
    Ok(())
}

async fn edit(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let r = &p.positionals(1, 1, "locus notes edit <ref>")?[0];
    let piped = piped_stdin()?;
    let client = ctx.client().await?;
    let (nb, note) = fetch_note(&client, r).await?;
    let body = match piped {
        Some(body) => body,
        None => edit_in_editor(&note.body_md, ".md")?,
    };
    if body == note.body_md {
        emit("no changes\n");
        return Ok(());
    }
    client
        .save_note(&nb.host, &nb.flag_name, note.id, &body, note.revision)
        .await
        .map_err(client_error)?;
    emit(&format!("saved {}\n", note_ref(&nb, note.id)));
    Ok(())
}

async fn rename(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(2, 2, "locus notes rename <ref> <title>")?;
    let client = ctx.client().await?;
    let (nb, id) = note_target(&client, &pos[0]).await?;
    client
        .rename_note(&nb.host, &nb.flag_name, id, &pos[1])
        .await
        .map_err(client_error)?;
    emit(&format!("renamed {}\n", note_ref(&nb, id)));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let r = &p.positionals(1, 1, "locus notes rm <ref>")?[0];
    let client = ctx.client().await?;
    let (nb, id) = note_target(&client, r).await?;
    client
        .delete_note(&nb.host, &nb.flag_name, id)
        .await
        .map_err(client_error)?;
    emit(&format!("deleted {}\n", note_ref(&nb, id)));
    Ok(())
}

async fn history(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let r = &p.positionals(1, 1, "locus notes history <ref> [--json]")?[0];
    let client = ctx.client().await?;
    let (nb, id) = note_target(&client, r).await?;
    let revs = client
        .history(&nb.host, &nb.flag_name, id)
        .await
        .map_err(client_error)?;
    if p.has("--json") {
        fmt::json(&revs);
    } else {
        let rows: Vec<Vec<String>> = revs
            .iter()
            .map(|v| {
                vec![
                    v.rev.to_string(),
                    stamp(v.at, ctx.tz),
                    v.author.clone(),
                    one_line(&v.title),
                ]
            })
            .collect();
        emit(&table(&["REV", "WHEN", "AUTHOR", "TITLE"], &rows));
    }
    Ok(())
}

async fn search(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let query = p
        .positionals(1, usize::MAX, "locus notes search <query> [--json]")?
        .join(" ");
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let mut hits = Vec::new();
    for nb in &books {
        let mut from = None;
        // The server examines a bounded number of notes per page.
        for _ in 0..10_000 {
            let page = match client
                .search(&nb.host, &nb.flag_name, &query, from, None)
                .await
            {
                Ok(page) => page,
                Err(Error::NotFound { .. }) => break,
                Err(e) => return Err(client_error(e)),
            };
            hits.extend(page.notes.into_iter().map(|n| (nb, n)));
            if page.last == 0 {
                break;
            }
            from = Some(page.last);
        }
    }
    if p.has("--json") {
        let out: Vec<_> = hits
            .iter()
            .map(|(nb, n)| {
                json!({"ref": note_ref(nb, n.id), "notebook": nb.notebook.title,
                       "title": n.title, "updatedAt": n.updated_at})
            })
            .collect();
        fmt::json(&out);
    } else {
        let rows: Vec<Vec<String>> = hits
            .iter()
            .map(|(nb, n)| {
                vec![
                    note_ref(nb, n.id),
                    stamp(n.updated_at, ctx.tz),
                    one_line(&n.title),
                ]
            })
            .collect();
        emit(&table(&["REF", "UPDATED", "TITLE"], &rows));
    }
    Ok(())
}

async fn mkdir(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(2, 2, "locus notes mkdir <notebook> <folder path>")?;
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let nb = find_notebook(&books, &pos[0])?;
    let names = folder_names(&pos[1]);
    if names.is_empty() {
        return Err("folder path is empty".into());
    }
    let mut folders = client
        .list_folders(&nb.host, &nb.flag_name)
        .await
        .map_err(client_error)?;
    let mut cur = nb.notebook.root_folder_id;
    for name in &names {
        let existing = folders
            .iter()
            .find(|f| f.parent_folder_id == Some(cur) && &f.name == name)
            .map(|f| f.id);
        cur = match existing {
            Some(id) => id,
            None => {
                let f = client
                    .create_folder(&nb.host, &nb.flag_name, cur, name)
                    .await
                    .map_err(client_error)?;
                folders.push(f.clone());
                f.id
            }
        };
    }
    emit(&format!("{}/{}\n", nb.flag_name, names.join("/")));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use locus_client::notes::Notebook;

    fn book(flag: &str, title: &str) -> NotebookSummary {
        NotebookSummary {
            host: "~zod".into(),
            flag_name: flag.into(),
            notebook: Notebook {
                id: 1,
                title: title.into(),
                root_folder_id: 10,
            },
            visibility: String::new(),
        }
    }

    fn folder(id: u64, parent: u64, name: &str) -> Folder {
        Folder {
            id,
            notebook_id: 1,
            name: name.into(),
            parent_folder_id: Some(parent),
        }
    }

    #[test]
    fn notebook_matches_flag_or_title() {
        let books = [book("notes", "Notes"), book("work", "Work Log")];
        assert_eq!(find_notebook(&books, "notes").unwrap().flag_name, "notes");
        assert_eq!(find_notebook(&books, "work log").unwrap().flag_name, "work");
        assert!(find_notebook(&books, "nope").is_err());
    }

    #[test]
    fn ambiguous_notebook_is_an_error() {
        let books = [book("a", "Same"), book("b", "same")];
        let err = find_notebook(&books, "SAME").unwrap_err();
        assert!(err.contains("ambiguous") && err.contains("a, b"));
        assert_eq!(find_notebook(&books, "a").unwrap().flag_name, "a");
    }

    #[test]
    fn locate_splits_folder_path() {
        let books = [book("notes", "Notes"), book("odd", "A/B")];
        let (nb, names) = locate(&books, "notes/projects/x").unwrap();
        assert_eq!(nb.flag_name, "notes");
        assert_eq!(names, ["projects", "x"]);
        assert_eq!(locate(&books, "A/B").unwrap().0.flag_name, "odd");
        assert!(locate(&books, "missing/x").is_err());
    }

    #[test]
    fn note_refs_parse() {
        assert_eq!(parse_note_ref("notes:42"), Ok(("notes", 42)));
        assert_eq!(parse_note_ref("a:b:7"), Ok(("a:b", 7)));
        assert!(parse_note_ref("notes").is_err());
        assert!(parse_note_ref(":3").is_err());
        assert!(parse_note_ref("notes:x").is_err());
    }

    #[test]
    fn folders_resolve_and_print() {
        let folders = [
            folder(11, 10, "a"),
            folder(12, 11, "b"),
            folder(13, 10, "c"),
        ];
        let names = folder_names("/a//b/");
        assert_eq!(resolve_folder(&folders, 10, &names), Ok(12));
        assert_eq!(resolve_folder(&folders, 10, &[]), Ok(10));
        assert!(resolve_folder(&folders, 10, &folder_names("a/c")).is_err());
        assert_eq!(folder_path(&folders, 10, 12), "a/b");
        assert_eq!(folder_path(&folders, 10, 10), "");
    }
}
