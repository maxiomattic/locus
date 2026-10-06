//! Host-side mail edge. Canonical mail stays on `%mail`.
//! This process parses RFC822 and POSTs ingest; send prints SMTP (dry-run).

use mail_protocol::Client;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMail {
    pub from: String,
    pub to: Vec<String>,
    pub subject: String,
    pub body: String,
}

pub fn parse_eml(raw: &str) -> ParsedMail {
    let normalized = raw.replace("\r\n", "\n");
    let (head, body) = match normalized.split_once("\n\n") {
        Some((h, b)) => (h, b.trim_end().to_string()),
        None => (normalized.as_str(), String::new()),
    };
    let mut from = String::new();
    let mut to = Vec::new();
    let mut subject = String::new();
    for line in head.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "from" => from = value.trim().to_string(),
            "to" => {
                to = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "subject" => subject = value.trim().to_string(),
            _ => {}
        }
    }
    ParsedMail {
        from,
        to,
        subject,
        body,
    }
}

fn eyre_url() -> String {
    env::var("LOCUS_EYRE_URL")
        .or_else(|_| env::var("NOTES_EYRE_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:8080".into())
}

fn client() -> Result<Client, String> {
    if let Ok(key) = env::var("MAIL_API_KEY") {
        return Ok(Client::mail_with_key(eyre_url(), key));
    }
    let code = if let Ok(p) = env::var("NOTES_CODE_FILE") {
        fs::read_to_string(p).map_err(|e| e.to_string())?
    } else {
        return Err("set MAIL_API_KEY or NOTES_CODE_FILE".into());
    };
    let mut c = Client::mail(eyre_url());
    c.login(code.trim()).map_err(|e| e.to_string())?;
    Ok(c)
}

fn main() {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".into());
    let status = match cmd.as_str() {
        "ingest" => cmd_ingest(args.next().as_deref()),
        "send" => cmd_send(args.next().as_deref()),
        _ => {
            eprintln!(
                "mail-bridge ingest <file.eml>\nmail-bridge send <id>\nCanonical store is %mail; this is a pipe."
            );
            Ok(())
        }
    };
    if let Err(e) = status {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn cmd_ingest(path: Option<&str>) -> Result<(), String> {
    let path = path.ok_or("ingest needs a file")?;
    let raw = fs::read_to_string(Path::new(path)).map_err(|e| e.to_string())?;
    let parsed = parse_eml(&raw);
    if parsed.from.is_empty() {
        return Err("From: header required".into());
    }
    let to: Vec<&str> = parsed.to.iter().map(String::as_str).collect();
    let c = client()?;
    let msg = c
        .ingest(&parsed.from, &to, &parsed.subject, &parsed.body)
        .map_err(|e| e.to_string())?;
    println!(
        "ingested id={} folder={} subject={}",
        msg.id, msg.folder, msg.subject
    );
    Ok(())
}

fn cmd_send(id: Option<&str>) -> Result<(), String> {
    let id: u64 = id
        .ok_or("send needs an id")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let c = client()?;
    let msg = c.send(id).map_err(|e| e.to_string())?;
    println!(
        "dry-run SMTP\nFrom: {}\nTo: {}\nSubject: {}\n\n{}\n",
        msg.from,
        msg.to.join(", "),
        msg.subject,
        msg.body
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_eml() {
        let raw = "From: alice@example.com\nTo: bob@example.com\nSubject: P2 ingest\n\nHello from the host bridge.\n";
        let p = parse_eml(raw);
        assert_eq!(p.from, "alice@example.com");
        assert_eq!(p.to, vec!["bob@example.com"]);
        assert_eq!(p.subject, "P2 ingest");
        assert_eq!(p.body, "Hello from the host bridge.");
    }
}
