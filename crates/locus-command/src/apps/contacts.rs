//! `locus contacts`: the clan address book.

use locus_client::contacts::{Contact, ContactPatch};
use serde_json::json;

use super::Ctx;
use super::args::{Parsed, parse_id};
use super::fmt::{self, client_error, emit, one_line, table};

/// Help for `locus contacts --help`.
pub const HELP: &str = "locus contacts <command>

  ls [--json]                                    list contacts
  show <id> [--json]                             show one contact
  add <name> [--email <e>] [--ship ~s]           add a contact; prints the id
  edit <id> [--name <n>] [--email <e>] [--ship ~s]
  rm <id>                                        delete a contact
";

/// Run one `locus contacts` command.
pub async fn run(ctx: &Ctx, cmd: &str, args: Vec<String>) -> Result<(), String> {
    match cmd {
        "ls" => ls(ctx, args).await,
        "show" => show(ctx, args).await,
        "add" => add(ctx, args).await,
        "edit" => edit(ctx, args).await,
        "rm" => rm(ctx, args).await,
        other => Err(format!(
            "unknown contacts command {other}. try: locus contacts --help"
        )),
    }
}

fn to_json(c: &Contact) -> serde_json::Value {
    json!({"id": c.id, "name": c.name, "email": c.email, "ship": c.ship, "revision": c.revision})
}

/// A ship name as the server expects it: empty, or with a leading `~`.
pub fn normalize_ship(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(String::new());
    }
    let name = t.strip_prefix('~').unwrap_or(t);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        return Err(format!("not a ship name: {s}"));
    }
    Ok(format!("~{name}"))
}

async fn find(client: &locus_client::Client, id: u64) -> Result<Contact, String> {
    client
        .list_contacts()
        .await
        .map_err(client_error)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("no contact {id}"))
}

async fn ls(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    p.positionals(0, 0, "locus contacts ls [--json]")?;
    let contacts = ctx
        .client()
        .await?
        .list_contacts()
        .await
        .map_err(client_error)?;
    if p.has("--json") {
        fmt::json(&contacts.iter().map(to_json).collect::<Vec<_>>());
    } else {
        let rows: Vec<Vec<String>> = contacts
            .iter()
            .map(|c| {
                vec![
                    c.id.to_string(),
                    one_line(&c.name),
                    c.email.clone(),
                    c.ship.clone(),
                ]
            })
            .collect();
        emit(&table(&["ID", "NAME", "EMAIL", "SHIP"], &rows));
    }
    Ok(())
}

async fn show(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &["--json"])?;
    let id = parse_id(
        &p.positionals(1, 1, "locus contacts show <id> [--json]")?[0],
        "contact",
    )?;
    let c = find(&ctx.client().await?, id).await?;
    if p.has("--json") {
        fmt::json(&to_json(&c));
    } else {
        emit(&format!(
            "id:    {}\nname:  {}\nemail: {}\nship:  {}\n",
            c.id, c.name, c.email, c.ship
        ));
    }
    Ok(())
}

async fn add(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &["--email", "--ship"], &[])?;
    let name = &p.positionals(1, 1, "locus contacts add <name> [--email <e>] [--ship ~s]")?[0];
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    let ship = normalize_ship(p.value("--ship").unwrap_or(""))?;
    let c = ctx
        .client()
        .await?
        .create_contact(name.trim(), p.value("--email").unwrap_or("").trim(), &ship)
        .await
        .map_err(client_error)?;
    emit(&format!("{}\n", c.id));
    Ok(())
}

async fn edit(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let usage = "locus contacts edit <id> [--name <n>] [--email <e>] [--ship ~s]";
    let p = Parsed::parse(&args, &["--name", "--email", "--ship"], &[])?;
    let id = parse_id(&p.positionals(1, 1, usage)?[0], "contact")?;
    let patch = ContactPatch {
        name: p.value("--name").map(|n| n.trim().to_string()),
        email: p.value("--email").map(|e| e.trim().to_string()),
        ship: p.value("--ship").map(normalize_ship).transpose()?,
    };
    if patch == ContactPatch::default() {
        return Err(format!("nothing to change\nusage: {usage}"));
    }
    if patch.name.as_deref() == Some("") {
        return Err("name is required".into());
    }
    let client = ctx.client().await?;
    let old = find(&client, id).await?;
    client
        .patch_contact(id, old.revision, &patch)
        .await
        .map_err(client_error)?;
    emit(&format!("updated {id}\n"));
    Ok(())
}

async fn rm(ctx: &Ctx, args: Vec<String>) -> Result<(), String> {
    let p = Parsed::parse(&args, &[], &[])?;
    let id = parse_id(
        &p.positionals(1, 1, "locus contacts rm <id>")?[0],
        "contact",
    )?;
    ctx.client()
        .await?
        .delete_contact(id, None)
        .await
        .map_err(client_error)?;
    emit(&format!("deleted {id}\n"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ships_are_normalized() {
        assert_eq!(normalize_ship("zod"), Ok("~zod".into()));
        assert_eq!(
            normalize_ship(" ~sampel-palnet "),
            Ok("~sampel-palnet".into())
        );
        assert_eq!(normalize_ship(""), Ok(String::new()));
        assert!(normalize_ship("~").is_err());
        assert!(normalize_ship("Bad Ship").is_err());
    }
}
