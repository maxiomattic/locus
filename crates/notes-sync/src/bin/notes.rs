//! Agent/CLI surface for the same functions as the notes window.

use notes_sync::{
    keep_both, load_saved, push_dirty, search_local, store_path, vault_root, vault_sync, Session,
    Store,
};
use std::env;
use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".into());
    match cmd.as_str() {
        "help" | "-h" | "--help" => {
            print_help();
            Ok(())
        }
        "list" => cmd_list(),
        "open" => {
            let id = parse_id(args.next().ok_or("usage: notes open <id>")?)?;
            cmd_open(id)
        }
        "search" => {
            let q = args.collect::<Vec<_>>().join(" ");
            cmd_search(&q)
        }
        "create" => {
            let title = args.next().unwrap_or_else(|| "Untitled".into());
            let body = rest_or_stdin(args)?;
            cmd_create(&title, &body)
        }
        "patch" => {
            let id = parse_id(args.next().ok_or("usage: notes patch <id> [title]")?)?;
            let title = args.next();
            let body = rest_or_stdin(args)?;
            cmd_patch(id, title.as_deref(), &body)
        }
        "sync" => cmd_sync(),
        "vault-sync" => cmd_vault_sync(),
        "history" => {
            let id = parse_id(args.next().ok_or("usage: notes history <id>")?)?;
            cmd_history(id)
        }
        other => Err(format!("unknown command {other}. try: notes help")),
    }
}

fn print_help() {
    eprintln!(
        "notes — same functions as the COSMIC window\n\
         \n\
         notes list\n\
         notes open <id>\n\
         notes search <query>\n\
         notes create [title]     # body from remaining args or stdin\n\
         notes patch <id> [title] # body from remaining args or stdin\n\
         notes sync               # ship dirty heads\n\
         notes vault-sync         # two-way with NOTES_VAULT / ~/Documents/new\n\
         notes history <id>"
    );
}

fn parse_id(s: String) -> Result<u64, String> {
    s.parse().map_err(|_| format!("not a note id: {s}"))
}

fn rest_or_stdin(args: impl Iterator<Item = String>) -> Result<String, String> {
    let rest = args.collect::<Vec<_>>().join(" ");
    if !rest.is_empty() {
        return Ok(rest);
    }
    if io::stdin().is_terminal() {
        return Ok(String::new());
    }
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn open_store() -> Result<Store, String> {
    Store::open(store_path()).map_err(|e| e.to_string())
}

fn try_session() -> Option<Session> {
    let saved = load_saved();
    let key = saved.api_key.filter(|k| !k.is_empty())?;
    Session::connect_with_api_key(&saved.eyre_url, &key).ok()
}

fn cmd_list() -> Result<(), String> {
    let store = open_store()?;
    run_vault(&store)?;
    if let Some(session) = try_session() {
        if let Ok(notes) = session.note_list() {
            for n in &notes {
                let _ = store.upsert_head(n);
            }
        }
        let _ = push_dirty(&session, &store);
    }
    for n in store.list_heads().map_err(|e| e.to_string())? {
        println!("{}\t{}", n.id, n.title);
    }
    for (id, title, _) in store.pending_creates().map_err(|e| e.to_string())? {
        println!("pending-{id}\t{title}");
    }
    Ok(())
}

fn cmd_open(id: u64) -> Result<(), String> {
    let store = open_store()?;
    if store.is_vault(id).unwrap_or(false) {
        let note = store
            .head(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("no local note {id}"))?;
        print!("{}", note.body_md);
        return Ok(());
    }
    if let Some(session) = try_session() {
        match session.note_open(id) {
            Ok(n) => {
                let _ = store.upsert_head(&n);
                print!("{}", n.body_md);
                return Ok(());
            }
            Err(_) => {}
        }
    }
    let note = store
        .head(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no local note {id}"))?;
    print!("{}", note.body_md);
    Ok(())
}

fn cmd_search(q: &str) -> Result<(), String> {
    let store = open_store()?;
    for n in search_local(&store, q).map_err(|e| e.to_string())? {
        println!("{}\t{}", n.id, n.title);
    }
    Ok(())
}

fn cmd_create(title: &str, body: &str) -> Result<(), String> {
    let store = open_store()?;
    if let Some(session) = try_session() {
        let note = session
            .note_create(title, body)
            .map_err(|e| e.to_string())?;
        store.upsert_head(&note).map_err(|e| e.to_string())?;
        let _ = store.snapshot(note.id, &note.title, &note.body_md);
        println!("{}", note.id);
        return Ok(());
    }
    let qid = store
        .enqueue_create(title, body)
        .map_err(|e| e.to_string())?;
    println!("pending-{qid}");
    Ok(())
}

fn cmd_patch(id: u64, title: Option<&str>, body: &str) -> Result<(), String> {
    let store = open_store()?;
    let mut note = store
        .head(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no local note {id}"))?;
    if let Some(t) = title {
        note.title = t.to_string();
    }
    note.body_md = body.to_string();
    store
        .snapshot(id, &note.title, &note.body_md)
        .map_err(|e| e.to_string())?;
    if store.is_vault(id).map_err(|e| e.to_string())? {
        run_vault(&store)?;
        println!("vault");
        return Ok(());
    }
    if let Some(session) = try_session() {
        match session.note_patch(id, &note.title, &note.body_md, note.revision) {
            Ok(saved) => {
                store.upsert_head(&saved).map_err(|e| e.to_string())?;
                store.clear_dirty(id).map_err(|e| e.to_string())?;
                println!("{}", saved.revision);
                return Ok(());
            }
            Err(e) if e.is_conflict() => {
                let host = session.note_open(id).map_err(|e| e.to_string())?;
                let merged = keep_both(&note.body_md, &host);
                store
                    .snapshot(id, &note.title, &merged)
                    .map_err(|e| e.to_string())?;
                let saved = session
                    .note_patch(id, &note.title, &merged, host.revision)
                    .map_err(|e| e.to_string())?;
                store.upsert_head(&saved).map_err(|e| e.to_string())?;
                writeln!(
                    io::stderr(),
                    "conflict: kept both (ship r{})",
                    host.revision
                )
                .ok();
                println!("{}", saved.revision);
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    println!("queued");
    Ok(())
}

fn cmd_sync() -> Result<(), String> {
    let store = open_store()?;
    run_vault(&store)?;
    let session = try_session().ok_or("not signed in (open the notes window once)")?;
    let n = push_dirty(&session, &store).map_err(|e| e.to_string())?;
    println!("synced {n}");
    Ok(())
}

fn cmd_vault_sync() -> Result<(), String> {
    let store = open_store()?;
    let Some(root) = vault_root() else {
        return Err(
            "no vault (set NOTES_VAULT or OBSIDIAN_VAULT_PATH, or keep ~/Documents/new)".into(),
        );
    };
    let r = vault_sync(&store, &root).map_err(|e| e.to_string())?;
    eprintln!(
        "vault {}  +{} ~{} !{} -{}",
        root.display(),
        r.pulled,
        r.pushed,
        r.conflicts,
        r.removed
    );
    Ok(())
}

fn run_vault(store: &Store) -> Result<(), String> {
    let Some(root) = vault_root() else {
        return Ok(());
    };
    let r = vault_sync(store, &root).map_err(|e| e.to_string())?;
    if r.pulled + r.pushed + r.conflicts + r.removed > 0 {
        eprintln!(
            "vault {}  +{} ~{} !{} -{}",
            root.display(),
            r.pulled,
            r.pushed,
            r.conflicts,
            r.removed
        );
    }
    Ok(())
}

fn cmd_history(id: u64) -> Result<(), String> {
    let store = open_store()?;
    for snap in store.history(id, 20).map_err(|e| e.to_string())? {
        println!("{}\t{}", snap.seq, snap.title);
    }
    Ok(())
}
