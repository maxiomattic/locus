//! Command-line twins of the Locus GUI apps: `locus notes|todo|calendar|
//! photos|files|contacts <command>`, plus `locus connect` and `locus open`.
//!
//! The commands talk to the gateway with the async [`locus_client::Client`],
//! using the same `~/.config/locus/desktop.json` as the desktop app. Listings
//! print an aligned table, or JSON with `--json`. Parsing and formatting are
//! plain functions so they can be unit tested without a server.

pub mod args;
pub mod calendar;
pub mod contacts;
pub mod files;
pub mod fmt;
pub mod notes;
pub mod photos;
pub mod todo;

use locus_client::{Client, config};
use std::io::{IsTerminal, Read};
use url::Url;

/// The app names accepted by `locus <app>` and `locus open <app>`.
pub const APPS: [&str; 6] = ["notes", "todo", "calendar", "photos", "files", "contacts"];

/// Per-run state shared by the commands.
pub struct Ctx {
    /// Local UTC offset in seconds, read before any thread exists.
    pub tz: i32,
}

impl Ctx {
    /// A client logged in with the saved settings.
    pub async fn client(&self) -> Result<Client, String> {
        let settings = config::load();
        if settings.server.is_empty() {
            return Err("not connected; run: locus connect <server-url>".into());
        }
        let client = Client::new(parse_server(&settings.server)?).map_err(fmt::client_error)?;
        if !settings.code.is_empty() {
            client
                .login(&settings.code)
                .await
                .map_err(fmt::client_error)?;
        }
        Ok(client)
    }
}

/// Whether `name` is one of the app names.
pub fn is_app(name: &str) -> bool {
    APPS.contains(&name)
}

/// Run `locus <app> <command> ...`. Prints the app's help when no command is
/// given or the command is `help`.
pub fn run(app: &str, args: Vec<String>) -> Result<(), String> {
    let help = match app {
        "notes" => notes::HELP,
        "todo" => todo::HELP,
        "calendar" => calendar::HELP,
        "photos" => photos::HELP,
        "files" => files::HELP,
        "contacts" => contacts::HELP,
        other => return Err(format!("unknown app {other}")),
    };
    let Some(cmd) = args.first().cloned() else {
        fmt::emit(help);
        return Ok(());
    };
    if matches!(cmd.as_str(), "help" | "-h" | "--help") {
        fmt::emit(help);
        return Ok(());
    }
    let rest = args[1..].to_vec();
    if rest.iter().any(|a| a == "-h" || a == "--help") {
        fmt::emit(help);
        return Ok(());
    }
    // `time` refuses to read the zone once the process has other threads.
    let tz = time::UtcOffset::current_local_offset()
        .map(|o| o.whole_seconds())
        .unwrap_or(0);
    let ctx = Ctx { tz };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        match app {
            "notes" => notes::run(&ctx, &cmd, rest).await,
            "todo" => todo::run(&ctx, &cmd, rest).await,
            "calendar" => calendar::run(&ctx, &cmd, rest).await,
            "photos" => photos::run(&ctx, &cmd, rest).await,
            "files" => files::run(&ctx, &cmd, rest).await,
            _ => contacts::run(&ctx, &cmd, rest).await,
        }
    })
}

/// `locus open <app> [target]`: replace this process with the GUI app.
pub fn open(args: Vec<String>) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let usage = "usage: locus open <notes|todo|calendar|photos|files|contacts> [target]";
    let app = args.first().ok_or(usage)?;
    if !is_app(app) || args.len() > 2 {
        return Err(usage.into());
    }
    let mut cmd = std::process::Command::new(format!("locus-{app}"));
    if let Some(target) = args.get(1) {
        cmd.arg("--open").arg(target);
    }
    let err = cmd.exec();
    Err(format!("cannot start locus-{app}: {err}"))
}

/// `locus connect [<server-url>] [--code-file <path>]`: save the gateway
/// address and ship code after checking that they log in. With no arguments,
/// print the configured server (never the code).
pub fn connect(args: Vec<String>) -> Result<(), String> {
    let p = args::Parsed::parse(&args, &["--code-file"], &[])?;
    let usage = "locus connect [<server-url> [--code-file <path>]]";
    let pos = p.positionals(0, 1, usage)?;
    let Some(server) = pos.first() else {
        let settings = config::load();
        if settings.server.is_empty() {
            fmt::emit("not connected\n");
        } else {
            fmt::emit(&format!("{}\n", settings.server));
        }
        return Ok(());
    };
    let url = parse_server(server)?;
    let code = match p.value("--code-file") {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?,
        None if !std::io::stdin().is_terminal() => read_stdin()?,
        None => return Err(format!("usage: {usage}\n(code comes from a file or stdin)")),
    };
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err("the code is empty".into());
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(async {
        let client = Client::new(url.clone()).map_err(fmt::client_error)?;
        client.login(&code).await.map_err(fmt::client_error)
    })?;
    config::save(&config::Settings {
        server: url.as_str().trim_end_matches('/').to_string(),
        code,
    })?;
    fmt::emit(&format!("connected to {url}\n"));
    Ok(())
}

/// A server address as typed: a missing scheme means `http://`.
pub fn parse_server(raw: &str) -> Result<Url, String> {
    let t = raw.trim();
    let full = if t.contains("://") {
        t.to_string()
    } else {
        format!("http://{t}")
    };
    Url::parse(&full).map_err(|e| format!("bad server address {raw}: {e}"))
}

/// All of stdin as text.
pub fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("reading stdin: {e}"))?;
    Ok(buf)
}

/// The piped-in body when stdin is not a terminal.
pub fn piped_stdin() -> Result<Option<String>, String> {
    if std::io::stdin().is_terminal() {
        Ok(None)
    } else {
        read_stdin().map(Some)
    }
}

/// Edit `initial` in `$VISUAL`/`$EDITOR` (default `vi`) on a private temp
/// file and return the saved text.
pub fn edit_in_editor(initial: &str, suffix: &str) -> Result<String, String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("locus-edit-{}{suffix}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(initial.as_bytes())
        .map_err(|e| e.to_string())?;
    drop(file);
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(&path)
        .status();
    let text = match status {
        Ok(s) if s.success() => std::fs::read_to_string(&path).map_err(|e| e.to_string()),
        Ok(s) => Err(format!("editor exited with {s}")),
        Err(e) => Err(format!("cannot run {editor}: {e}")),
    };
    let _ = std::fs::remove_file(&path);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_gets_default_scheme() {
        assert_eq!(
            parse_server("surface:8090").unwrap().as_str(),
            "http://surface:8090/"
        );
        assert_eq!(
            parse_server(" https://x.example ").unwrap().as_str(),
            "https://x.example/"
        );
        assert!(parse_server("http://").is_err());
    }

    #[test]
    fn app_names() {
        assert!(is_app("notes") && is_app("contacts"));
        assert!(!is_app("login"));
    }
}
