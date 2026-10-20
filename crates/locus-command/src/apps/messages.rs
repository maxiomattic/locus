//! `locus messages`: direct messages with other ships, from the command line.
//! The same functions as the Messages app.

use locus_client::messages::{Conversation, Message, group_id, group_key};
use serde_json::json;

use super::Ctx;
use super::args::Parsed;
use super::fmt::{self, client_error, clip, emit, one_line, table};
use locus_core::messages::{parse_new_group, valid_ship};
use locus_core::view_model::short_ship;

/// Help for `locus messages --help`.
pub const HELP: &str = "locus messages <command>

  ls [--json]                       conversations and groups, newest first, with unread counts
  show <ship|group:~zod-1> [--json] the messages with a ship, or in a group, oldest first
  send <ship|group:~zod-1> <text...>  send a message, to a ship or to every member of a group
  group-new <title...> <ship...>    start a group with these members (you are added)
  read <ship|group:~zod-1>          mark a conversation or a group as read
  block <ship>                      refuse messages from a ship (your messages to it still go)
  unblock <ship>                    accept messages from a ship again
  rm <id>                           delete your copy of a message (the peer keeps theirs)

A message you send is pending until the other ship's agent has it (for a group,
until every member has it).
";

/// Run one `locus messages` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "show" => show(ctx, args).await,
        "send" => send(ctx, args).await,
        "group-new" => group_new(ctx, args).await,
        "read" => read(ctx, args).await,
        "block" => block(ctx, args).await,
        "unblock" => unblock(ctx, args).await,
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

/// A ship, or a group written `group:~zod-1` (as `ls` prints it).
fn target_arg(raw: &str) -> Result<String, String> {
    match group_id(raw.trim()) {
        Some(gid) if valid_ship(gid.split('-').next().unwrap_or("")) && gid.contains('-') => {
            Ok(raw.trim().to_string())
        }
        Some(_) => Err(format!("not a group id: {raw} (like group:~zod-1)")),
        None => ship_arg(raw),
    }
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
    let convos = client
        .list_all_conversations()
        .await
        .map_err(client_error)?;
    if p.has("--json") {
        let rows: Vec<_> = convos
            .iter()
            .map(|c| {
                json!({
                    "peer": c.peer,
                    "title": c.title,
                    "count": c.count,
                    "unread": c.unread,
                    "blocked": c.blocked,
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
                match &c.title {
                    Some(title) => format!("{title} ({})", c.peer),
                    None => c.peer.clone(),
                },
                if c.unread > 0 {
                    c.unread.to_string()
                } else {
                    String::new()
                },
                c.count.to_string(),
                if c.blocked {
                    "yes".into()
                } else {
                    String::new()
                },
                preview(c),
            ]
        })
        .collect();
    emit(&table(
        &["SHIP", "NEW", "MESSAGES", "BLOCKED", "LAST"],
        &rows,
    ));
    Ok(())
}

async fn show(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let pos = p.positionals(1, 1, "locus messages show <ship|group:~zod-1> [--json]")?;
    let peer = target_arg(&pos[0])?;
    let client = ctx.client().await?;
    let messages = match group_id(&peer) {
        Some(gid) => client.group_thread(gid).await,
        None => client.thread(&peer).await,
    }
    .map_err(client_error)?;
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
    let pos = p.positionals(
        2,
        usize::MAX,
        "locus messages send <ship|group:~zod-1> <text...>",
    )?;
    let to = target_arg(&pos[0])?;
    let body = pos[1..].join(" ");
    if body.trim().is_empty() {
        return Err("the message is empty".into());
    }
    let client = ctx.client().await?;
    let id = match group_id(&to) {
        Some(gid) => client.send_group_message(gid, &body).await,
        None => client.send_message(&to, &body).await,
    }
    .map_err(client_error)?;
    emit(&format!("{id}\n"));
    Ok(())
}

async fn read(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 1, "locus messages read <ship|group:~zod-1>")?;
    let peer = target_arg(&pos[0])?;
    let client = ctx.client().await?;
    match group_id(&peer) {
        Some(gid) => client.mark_group_read(gid).await,
        None => client.mark_read(&peer).await,
    }
    .map_err(client_error)?;
    emit(&format!("read {peer}\n"));
    Ok(())
}

async fn group_new(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(
        2,
        usize::MAX,
        "locus messages group-new <title...> <ship...>",
    )?;
    let (title, members) = parse_new_group(&pos.join(" "));
    if title.is_empty() {
        return Err("give the group a title, then its members, like: Team ~nec ~dev".into());
    }
    if members.is_empty() {
        return Err("add at least one member, like ~nec".into());
    }
    let client = ctx.client().await?;
    let gid = client
        .create_group(&title, &members)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", group_key(&gid)));
    Ok(())
}

async fn block(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 1, "locus messages block <ship>")?;
    let peer = ship_arg(&pos[0])?;
    let client = ctx.client().await?;
    client.block(&peer).await.map_err(client_error)?;
    emit(&format!("blocked {peer}\n"));
    Ok(())
}

async fn unblock(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let pos = p.positionals(1, 1, "locus messages unblock <ship>")?;
    let peer = ship_arg(&pos[0])?;
    let client = ctx.client().await?;
    client.unblock(&peer).await.map_err(client_error)?;
    emit(&format!("unblocked {peer}\n"));
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
