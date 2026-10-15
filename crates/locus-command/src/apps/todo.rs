//! `locus todo`: the task lines (`- [ ] text`) found in note bodies.
//!
//! A task is addressed as `<notebook>:<note id>:<line>` (line is the 0-based
//! line index in the note body). New tasks go where the GUI puts them: the
//! `Inbox` note of the first notebook, created when missing.

use locus_client::Client;
use locus_client::notes::{Note, NotebookSummary};
use locus_client::tasks::{add_task, parse_tasks, toggle_task};
use locus_core::date::Date;
use serde_json::json;

use super::Ctx;
use super::args::Parsed;
use super::fmt::{self, client_error, clip, emit, one_line, table};
use super::notes::{fetch_note, find_notebook, note_ref};

/// Title of the note new tasks go to (the same as in the GUI).
pub const INBOX_TITLE: &str = "Inbox";

/// Help for `locus todo --help`.
pub const HELP: &str = "locus todo <command>

  ls [--done|--all] [--json]                   list open tasks (default), done, or all
  add <text> [--due YYYY-MM-DD] [--note <ref>] add a task (default: Inbox note of the first notebook)
  done <ref>                                   mark a task done
  undo <ref>                                   mark a task not done

A task <ref> is <notebook>:<note id>:<line>, as printed by ls.
";

/// Run one `locus todo` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "add" => add(ctx, args).await,
        "done" => set_done(ctx, args, true).await,
        "undo" => set_done(ctx, args, false).await,
        other => Err(format!(
            "unknown todo command {other}. try: locus todo --help"
        )),
    }
}

/// Split a task ref into notebook name, note id and line.
pub fn parse_task_ref(s: &str) -> Result<(&str, u64, usize), String> {
    let bad = || format!("not a task ref (want <notebook>:<note id>:<line>): {s}");
    let (rest, line) = s.rsplit_once(':').ok_or_else(bad)?;
    let (nb, id) = rest.rsplit_once(':').ok_or_else(bad)?;
    if nb.is_empty() {
        return Err(bad());
    }
    Ok((
        nb,
        id.parse().map_err(|_| bad())?,
        line.parse().map_err(|_| bad())?,
    ))
}

/// Parse `--due`: a real `YYYY-MM-DD` date.
pub fn parse_due(s: &str) -> Result<(i32, u8, u8), String> {
    Date::parse(s.trim())
        .map(|d| (d.y, d.m, d.d))
        .ok_or_else(|| format!("due date must be YYYY-MM-DD: {s}"))
}

/// One task found in a note.
struct Found {
    reference: String,
    /// Notebook, note id and line, to order refs numerically.
    key: (String, u64, usize),
    notebook: String,
    note: String,
    done: bool,
    text: String,
    due: Option<(i32, u8, u8)>,
}

/// Every note of `nb` with its body (the list reply may omit bodies).
async fn notes_with_bodies(client: &Client, nb: &NotebookSummary) -> Result<Vec<Note>, String> {
    let mut all = client
        .list_notes(&nb.host, &nb.flag_name)
        .await
        .map_err(client_error)?;
    let missing: Vec<usize> = (0..all.len())
        .filter(|&i| all[i].body_md.is_empty())
        .collect();
    for chunk in missing.chunks(8) {
        let mut set = tokio::task::JoinSet::new();
        for &i in chunk {
            let (c, host, name, id) = (
                client.clone(),
                nb.host.clone(),
                nb.flag_name.clone(),
                all[i].id,
            );
            set.spawn(async move { (i, c.get_note(&host, &name, id).await) });
        }
        while let Some(joined) = set.join_next().await {
            let (i, res) = joined.map_err(|e| e.to_string())?;
            all[i] = res.map_err(client_error)?;
        }
    }
    Ok(all)
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--done", "--all", "--json"])?;
    p.positionals(0, 0, "locus todo ls [--done|--all] [--json]")?;
    if p.has("--done") && p.has("--all") {
        return Err("use either --done or --all".into());
    }
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let mut found = Vec::new();
    for nb in &books {
        let notes = notes_with_bodies(&client, nb).await?;
        for n in &notes {
            for t in parse_tasks(&n.body_md) {
                found.push(Found {
                    reference: format!("{}:{}", note_ref(nb, n.id), t.line),
                    key: (nb.flag_name.clone(), n.id, t.line),
                    notebook: nb.notebook.title.clone(),
                    note: n.title.clone(),
                    done: t.done,
                    text: t.text,
                    due: t.due,
                });
            }
        }
    }
    found.retain(|t| match (p.has("--all"), p.has("--done")) {
        (true, _) => true,
        (_, true) => t.done,
        _ => !t.done,
    });
    sort_tasks(&mut found);
    if p.has("--json") {
        let out: Vec<_> = found
            .iter()
            .map(|t| {
                json!({"ref": t.reference, "done": t.done, "text": t.text,
                       "due": t.due.map(due_text), "notebook": t.notebook, "note": t.note})
            })
            .collect();
        fmt::json(&out);
    } else {
        let rows: Vec<Vec<String>> = found
            .iter()
            .map(|t| {
                vec![
                    t.reference.clone(),
                    if t.done { "[x]" } else { "[ ]" }.into(),
                    t.due.map(due_text).unwrap_or_default(),
                    clip(&one_line(&t.text), 90),
                    clip(&one_line(&t.note), 40),
                ]
            })
            .collect();
        emit(&table(&["REF", "DONE", "DUE", "TASK", "NOTE"], &rows));
    }
    Ok(())
}

/// Earliest due date first, tasks without one last; then by ref.
fn sort_tasks(tasks: &mut [Found]) {
    tasks.sort_by(|a, b| {
        let key = |t: &Found| (t.due.is_none(), t.due);
        key(a).cmp(&key(b)).then_with(|| a.key.cmp(&b.key))
    });
}

fn due_text((y, m, d): (i32, u8, u8)) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

async fn add(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["--due", "--note"], &[])?;
    let text = p
        .positionals(
            1,
            usize::MAX,
            "locus todo add <text> [--due YYYY-MM-DD] [--note <ref>]",
        )?
        .join(" ");
    if text.trim().is_empty() {
        return Err("task text is required".into());
    }
    let due = p.value("--due").map(parse_due).transpose()?;
    let client = ctx.client().await?;
    if let Some(r) = p.value("--note") {
        let (nb, note) = fetch_note(&client, r).await?;
        let body = add_task(&note.body_md, &text, due);
        client
            .save_note(&nb.host, &nb.flag_name, note.id, &body, note.revision)
            .await
            .map_err(client_error)?;
        emit(&format!("added to {}\n", note_ref(&nb, note.id)));
        return Ok(());
    }
    let books = client.list_notebooks().await.map_err(client_error)?;
    let nb = books.first().ok_or("no notebook yet")?;
    let inbox = client
        .list_notes(&nb.host, &nb.flag_name)
        .await
        .map_err(client_error)?
        .into_iter()
        .find(|n| n.title == INBOX_TITLE);
    let id = match inbox {
        Some(n) => {
            let note = client
                .get_note(&nb.host, &nb.flag_name, n.id)
                .await
                .map_err(client_error)?;
            let body = add_task(&note.body_md, &text, due);
            client
                .save_note(&nb.host, &nb.flag_name, note.id, &body, note.revision)
                .await
                .map_err(client_error)?;
            note.id
        }
        None => {
            client
                .create_note(
                    &nb.host,
                    &nb.flag_name,
                    nb.notebook.root_folder_id,
                    INBOX_TITLE,
                    &add_task("", &text, due),
                )
                .await
                .map_err(client_error)?
                .id
        }
    };
    emit(&format!("added to {}\n", note_ref(nb, id)));
    Ok(())
}

async fn set_done(ctx: &Ctx, args: Vec<String>, done: bool) -> Result<(), String> {
    let usage = if done {
        "locus todo done <ref>"
    } else {
        "locus todo undo <ref>"
    };
    let p = Parsed::parse(&args, &[], &[])?;
    let r = &p.positionals(1, 1, usage)?[0];
    let (nb, id, line) = parse_task_ref(r)?;
    let client = ctx.client().await?;
    let books = client.list_notebooks().await.map_err(client_error)?;
    let nb = find_notebook(&books, nb)?;
    let note = client
        .get_note(&nb.host, &nb.flag_name, id)
        .await
        .map_err(client_error)?;
    let task = parse_tasks(&note.body_md)
        .into_iter()
        .find(|t| t.line == line)
        .ok_or_else(|| format!("line {line} of {} is not a task", note_ref(nb, id)))?;
    if task.done == done {
        emit(&format!(
            "already {}: {}\n",
            if done { "done" } else { "not done" },
            task.text
        ));
        return Ok(());
    }
    let body = toggle_task(&note.body_md, line);
    client
        .save_note(&nb.host, &nb.flag_name, id, &body, note.revision)
        .await
        .map_err(client_error)?;
    emit(&format!(
        "{}: {}\n",
        if done { "done" } else { "reopened" },
        task.text
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(reference: &str, due: Option<(i32, u8, u8)>) -> Found {
        Found {
            reference: reference.into(),
            key: {
                let mut it = reference.split(':');
                (
                    it.next().unwrap().into(),
                    it.next().unwrap().parse().unwrap(),
                    it.next().unwrap().parse().unwrap(),
                )
            },
            notebook: String::new(),
            note: String::new(),
            done: false,
            text: String::new(),
            due,
        }
    }

    #[test]
    fn task_refs_parse() {
        assert_eq!(parse_task_ref("notes:12:3"), Ok(("notes", 12, 3)));
        assert_eq!(parse_task_ref("a:b:12:0"), Ok(("a:b", 12, 0)));
        assert!(parse_task_ref("notes:12").is_err());
        assert!(parse_task_ref("notes:x:1").is_err());
        assert!(parse_task_ref(":1:2").is_err());
    }

    #[test]
    fn due_dates_are_validated() {
        assert_eq!(parse_due("2026-02-28"), Ok((2026, 2, 28)));
        assert!(parse_due("2026-02-30").is_err());
        assert!(parse_due("tomorrow").is_err());
        assert_eq!(due_text((2026, 3, 4)), "2026-03-04");
    }

    #[test]
    fn sorts_by_due_then_ref() {
        let mut v = vec![
            found("b:1:0", None),
            found("a:1:0", Some((2026, 5, 1))),
            found("c:1:0", Some((2026, 4, 1))),
            found("a:1:5", None),
            found("a:1:10", None),
            found("a:1:6", None),
        ];
        sort_tasks(&mut v);
        let refs: Vec<&str> = v.iter().map(|t| t.reference.as_str()).collect();
        assert_eq!(
            refs,
            ["c:1:0", "a:1:0", "a:1:5", "a:1:6", "a:1:10", "b:1:0"]
        );
    }
}
