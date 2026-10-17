//! `locus messages`: direct messages with other ships, from the command line.
//! The same functions as the Messages app.

use locus_client::messages::{Conversation, Message};
use serde_json::json;

use super::Ctx;
use super::args::Parsed;
use super::fmt::{self, client_error, clip, emit, one_line, table};
use locus_core::messages::valid_ship;
use locus_core::view_model::short_ship;

/// Help for `locus messages --help`.
pub const HELP: &str = "locus messages <command>

  ls [--json]                       conversations, newest first, with unread counts
  show <ship> [--json]              the messages with a ship, oldest first
  send <ship> <text...>             send a message (a ship like ~zod)
  read <ship>                       mark a conversation as read
  rm <id>                           delete your copy of a message (the peer keeps theirs)

A message you send is pending until the other ship's agent has it.
";

/// Run one `locus messages` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "show" => show(ctx, args).await,
        "send" => send(ctx, args).await,
        "read" => read(ctx, args).await,
        "rm" => rm(ctx, args).await,
        other => Err(format!(
            "unknown messages command {other}. try: locus messages --help"
        )),
    }
}

fn ship_arg(raw: &str) -> Result<String, String> {
    let ship = raw.trim().to_string();
    if !valid_ship(&ship) {
        return Err(format!("not a ship name: {raw} (like ~zod)"));
    }
    Ok(ship)
}

fn preview(c: &Conversation) -> String {
    c.last
        .as_ref()
        .map(|m| clip(&one_line(&m.body), 60))
        .unwrap_or_default()
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    p.positionals(0, 0, "locus messages ls [--json]")?;
    let client = ctx.client().await?;
    let convos = client.list_conversations().await.map_err(client_error)?;
    if p.has("--json") {
        let rows: Vec<_> = convos
            .iter()
            .map(|c| {
                json!({
                    "peer": c.peer,
                    "count": c.count,
                    "unread": c.unread,
                    "last": c.last,
                })
            })
            .collect();
        fmt::json(&json!({ "conversations": rows }));
        return Ok(());
    }
    let rows: Vec<Vec<String>> = convos
        .iter()
        .map(|c| {
            vec![
                c.peer.clone(),
                if c.unread > 0 {
                    c.unread.to_string()
                } else {
                    String::new()
                },
                c.count.to_string(),
                preview(c),
            ]
        })
        .collect();
    emit(&table(&["SHIP", "NEW", "MESSAGES", "LAST"], &rows));
    Ok(())
}

async fn show(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let pos = p.positionals(1, 1, "locus messages show <ship> [--json]")?;
    let peer = ship_arg(&pos[0])?;
    let client = ctx.client().await?;
    let messages = client.thread(&peer).await.map_err(client_error)?;
    if p.has("--json") {
        fmt::json(&json!({ "peer": peer, "messages": messages }));
        return Ok(());
    }
    let rows: Vec<Vec<String>> = messages
        .iter()
        .map(|m: &Message| {
            vec![
                m.id.to_string(),
                if m.outgoing {
                    "you".into()
                } else {
                    short_ship(&m.peer)
                },
                fmt::stamp(m.sent_unix(), ctx.tz),
                status_word(m),
                one_line(&m.body),
            ]
        })
        .collect();
    emit(&table(&["ID", "FROM", "SENT", "STATUS", "MESSAGE"], &rows));
    Ok(())
}

fn status_word(m: &Message) -> String {
    use locus_client::messages::Status;
    match (m.outgoing, m.status) {
        (false, _) => String::new(),
        (true, Status::Pending) => "sending".into(),
        (true, Status::Delivered) => "delivered".into(),
        (true, Status::Failed) => "failed".into(),
        (true, Status::Received) => String::new(),
    }
}

async fn send(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(2, usize::MAX, "locus messages send <ship> <text...>")?;
    let to = ship_arg(&pos[0])?;
    let body = pos[1..].join(" ");
    if body.trim().is_empty() {
        return Err("the message is empty".into());
    }
    let client = ctx.client().await?;
    let id = client
        .send_message(&to, &body)
        .await
        .map_err(client_error)?;
    emit(&format!("{id}\n"));
    Ok(())
}

async fn read(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 1, "locus messages read <ship>")?;
    let peer = ship_arg(&pos[0])?;
    let client = ctx.client().await?;
    client.mark_read(&peer).await.map_err(client_error)?;
    emit(&format!("read {peer}\n"));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 1, "locus messages rm <id>")?;
    let id = super::args::parse_id(&pos[0], "message id")?;
    let client = ctx.client().await?;
    client.delete_message(id).await.map_err(client_error)?;
    emit(&format!("deleted {id}\n"));
    Ok(())
}
