//! `%calendar`: canonical events on the ship. Host CalDAV bridges into this API.
//!
//! `start` and `end` are Unix seconds (the server converts them with
//! `unix-da`, which multiplies by `~s1`).

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Client, Error, Result, check_ok, with_revision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    #[serde(default)]
    pub calendar: String,
    pub title: String,
    /// Unix seconds.
    pub start: u64,
    /// Unix seconds.
    pub end: u64,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub revision: u64,
}

/// Fields to change on an existing event. `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EventPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    event: Option<Event>,
    events: Option<Vec<Event>>,
}

impl Envelope {
    fn into_event(self, path: &str) -> Result<Event> {
        check_ok(self.ok, self.error, self.message)?;
        self.event.ok_or_else(|| Error::Decode {
            path: path.into(),
            detail: "envelope has no `event`".into(),
        })
    }
}

impl Client {
    pub async fn list_events(&self) -> Result<Vec<Event>> {
        let env: Envelope = self.get_json("/calendar/~/v1/events").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.events.unwrap_or_default())
    }

    /// `start` and `end` are Unix seconds; the server rejects `end <= start`.
    /// `body` may be empty.
    pub async fn create_event(
        &self,
        title: &str,
        start: u64,
        end: u64,
        body: &str,
    ) -> Result<Event> {
        let p = "/calendar/~/v1/events";
        let env: Envelope = self
            .json(
                Method::POST,
                p,
                Some(json!({ "title": title, "start": start, "end": end, "body": body })),
            )
            .await?;
        env.into_event(p)
    }

    /// Patch an event. Stale `expected_revision` returns [`Error::Conflict`].
    pub async fn patch_event(
        &self,
        id: u64,
        expected_revision: u64,
        patch: &EventPatch,
    ) -> Result<Event> {
        let p = format!("/calendar/~/v1/events/{id}");
        let mut body = serde_json::to_value(patch).map_err(|e| Error::Decode {
            path: p.clone(),
            detail: e.to_string(),
        })?;
        body["expectedRevision"] = expected_revision.into();
        let env: Envelope = self.json(Method::PUT, &p, Some(body)).await?;
        env.into_event(&p)
    }

    /// Delete an event. `expected_revision` is optional; when set and stale the
    /// server answers 409 ([`Error::Conflict`]). A missing event is
    /// [`Error::NotFound`].
    pub async fn delete_event(&self, id: u64, expected_revision: Option<u64>) -> Result<()> {
        let p = with_revision(format!("/calendar/~/v1/events/{id}"), expected_revision);
        let env: Envelope = self.json(Method::DELETE, &p, None).await?;
        check_ok(env.ok, env.error, env.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_envelope_decodes() {
        let raw = r#"{"ok":true,"event":{"id":9,"calendar":"family","title":"Dentist","start":1767225600,"end":1767229200,"body":"bring card","revision":4}}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        let e = env.into_event("/p").unwrap();
        assert_eq!(e.id, 9);
        assert_eq!(e.calendar, "family");
        assert_eq!(e.start, 1_767_225_600);
        assert_eq!(e.end - e.start, 3600);
        assert_eq!(e.body, "bring card");
        assert_eq!(e.revision, 4);
    }

    #[test]
    fn list_envelope_defaults_optional_fields() {
        let raw =
            r#"{"ok":true,"events":[{"id":1,"calendar":"c","title":"t","start":10,"end":20}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        let events = env.events.unwrap();
        assert_eq!(events[0].body, "");
        assert_eq!(events[0].revision, 0);
    }

    #[test]
    fn conflict_envelope_maps_to_conflict() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"conflict","message":"stale expectedRevision"}"#,
        )
        .unwrap();
        assert!(matches!(
            env.into_event("/p"),
            Err(Error::Conflict { current: None })
        ));
    }

    #[test]
    fn patch_serializes_only_set_fields() {
        let patch = EventPatch {
            start: Some(100),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            json!({ "start": 100 })
        );
    }
}
