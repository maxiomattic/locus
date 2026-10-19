//! `%messages`: direct messages between ships. Each ship keeps its own copy.
//! A sent message is `pending` until the peer's agent acks it, then
//! `delivered` or `failed`. Messages from others are `received`.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Client, Error, Result, check_ok, seg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Delivered,
    Failed,
    Received,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: u64,
    /// The other ship, e.g. `~zod`.
    pub peer: String,
    /// True when this ship wrote the message.
    pub outgoing: bool,
    pub body: String,
    /// The ship's `@da`, e.g. `~2026.10.17..06.53.53..223a`.
    pub sent: String,
    pub status: Status,
}

impl Message {
    /// Unix seconds of `sent`, or 0 when it does not parse.
    pub fn sent_unix(&self) -> u64 {
        parse_da(&self.sent).unwrap_or(0)
    }
}

/// One row per peer, newest conversation data included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    pub peer: String,
    pub count: u64,
    pub unread: u64,
    pub last: Option<Message>,
    /// True when this ship refuses incoming messages from `peer`. Messages
    /// this ship sends to `peer` still go.
    #[serde(default)]
    pub blocked: bool,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    id: Option<u64>,
    conversations: Option<Vec<Conversation>>,
    messages: Option<Vec<Message>>,
}

/// Parse a Hoon `@da` such as `~2026.10.17..06.53.53..223a` into Unix
/// seconds. Fractions are dropped; a date with no time is midnight.
pub fn parse_da(raw: &str) -> Option<u64> {
    let body = raw.strip_prefix('~')?;
    let mut parts = body.split("..");
    let date = parts.next()?;
    let time = parts.next().unwrap_or("0.0.0");
    let mut d = date.split('.').map(|s| s.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split('.').map(|s| s.parse::<i64>());
    let (h, mi, s) = (
        t.next().unwrap_or(Ok(0)).ok()?,
        t.next().unwrap_or(Ok(0)).ok()?,
        t.next().unwrap_or(Ok(0)).ok()?,
    );
    let days = days_from_civil(y, m, day);
    let secs = days * 86_400 + h * 3_600 + mi * 60 + s;
    u64::try_from(secs).ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Client {
    /// Every peer you have messages with, newest first by last message.
    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let env: Envelope = self.get_json("/messages/~/v1/conversations").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.conversations.unwrap_or_default())
    }

    /// All messages with `peer`, oldest first.
    pub async fn thread(&self, peer: &str) -> Result<Vec<Message>> {
        let path = format!("/messages/~/v1/conversations/{}", seg(peer));
        let env: Envelope = self.get_json(&path).await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.messages.unwrap_or_default())
    }

    /// Mark everything from `peer` as read, through the newest message.
    pub async fn mark_read(&self, peer: &str) -> Result<()> {
        let path = format!("/messages/~/v1/conversations/{}/read", seg(peer));
        let env: Envelope = self.json(Method::POST, &path, Some(json!({}))).await?;
        check_ok(env.ok, env.error, env.message)
    }

    /// Refuse incoming messages from `peer`. Messages to `peer` still go.
    pub async fn block(&self, peer: &str) -> Result<()> {
        let path = format!("/messages/~/v1/conversations/{}/block", seg(peer));
        let env: Envelope = self.json(Method::POST, &path, Some(json!({}))).await?;
        check_ok(env.ok, env.error, env.message)
    }

    /// Accept incoming messages from `peer` again.
    pub async fn unblock(&self, peer: &str) -> Result<()> {
        let path = format!("/messages/~/v1/conversations/{}/unblock", seg(peer));
        let env: Envelope = self.json(Method::POST, &path, Some(json!({}))).await?;
        check_ok(env.ok, env.error, env.message)
    }

    /// Send `body` to `to` (a ship, e.g. `~zod`). Returns the local message id;
    /// the status moves from `pending` once the peer acks.
    pub async fn send_message(&self, to: &str, body: &str) -> Result<u64> {
        let p = "/messages/~/v1/messages";
        let env: Envelope = self
            .json(Method::POST, p, Some(json!({ "to": to, "body": body })))
            .await?;
        check_ok(env.ok, env.error, env.message)?;
        env.id.ok_or_else(|| Error::Decode {
            path: p.into(),
            detail: "envelope has no `id`".into(),
        })
    }

    /// Delete this ship's copy of a message. The peer keeps theirs.
    pub async fn delete_message(&self, id: u64) -> Result<()> {
        let path = format!("/messages/~/v1/messages/{id}");
        let env: Envelope = self.json(Method::DELETE, &path, None).await?;
        check_ok(env.ok, env.error, env.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_da_with_time_and_fraction() {
        // 2026-10-17 06:53:53 UTC
        let secs = parse_da("~2026.10.17..06.53.53..223a").unwrap();
        assert_eq!(secs, 1_792_220_033);
    }

    #[test]
    fn parses_date_only_as_midnight() {
        let secs = parse_da("~1970.1.2").unwrap();
        assert_eq!(secs, 86_400);
    }

    #[test]
    fn rejects_non_da() {
        assert_eq!(parse_da("hello"), None);
        assert_eq!(parse_da("~zod"), None);
    }

    #[test]
    fn conversation_without_blocked_is_not_blocked() {
        let raw = r#"{"peer":"~zod","count":1,"unread":0,"last":null}"#;
        let c: Conversation = serde_json::from_str(raw).unwrap();
        assert!(!c.blocked);
    }

    #[test]
    fn conversation_with_blocked_is_blocked() {
        let raw = r#"{"peer":"~zod","count":1,"unread":0,"last":null,"blocked":true}"#;
        let c: Conversation = serde_json::from_str(raw).unwrap();
        assert!(c.blocked);
    }
}
