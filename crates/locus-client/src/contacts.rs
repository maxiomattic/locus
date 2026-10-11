//! `%contacts`: canonical clan contacts on the ship.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{Client, Error, Result, check_ok, with_revision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub ship: String,
    #[serde(default)]
    pub revision: u64,
}

/// Fields to change on an existing contact. `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ContactPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ship: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    ok: bool,
    error: Option<String>,
    message: Option<String>,
    contact: Option<Contact>,
    contacts: Option<Vec<Contact>>,
}

impl Envelope {
    fn into_contact(self, path: &str) -> Result<Contact> {
        check_ok(self.ok, self.error, self.message)?;
        self.contact.ok_or_else(|| Error::Decode {
            path: path.into(),
            detail: "envelope has no `contact`".into(),
        })
    }
}

impl Client {
    pub async fn list_contacts(&self) -> Result<Vec<Contact>> {
        let env: Envelope = self.get_json("/contacts/~/v1/contacts").await?;
        check_ok(env.ok, env.error, env.message)?;
        Ok(env.contacts.unwrap_or_default())
    }

    /// `email` and `ship` may be empty strings.
    pub async fn create_contact(&self, name: &str, email: &str, ship: &str) -> Result<Contact> {
        let p = "/contacts/~/v1/contacts";
        let env: Envelope = self
            .json(
                Method::POST,
                p,
                Some(json!({ "name": name, "email": email, "ship": ship })),
            )
            .await?;
        env.into_contact(p)
    }

    /// Patch a contact. Stale `expected_revision` returns [`Error::Conflict`].
    pub async fn patch_contact(
        &self,
        id: u64,
        expected_revision: u64,
        patch: &ContactPatch,
    ) -> Result<Contact> {
        let p = format!("/contacts/~/v1/contacts/{id}");
        let mut body = serde_json::to_value(patch).map_err(|e| Error::Decode {
            path: p.clone(),
            detail: e.to_string(),
        })?;
        body["expectedRevision"] = expected_revision.into();
        let env: Envelope = self.json(Method::PUT, &p, Some(body)).await?;
        env.into_contact(&p)
    }

    /// Delete a contact. `expected_revision` is optional; when set and stale the
    /// server answers 409 ([`Error::Conflict`]). A missing contact is
    /// [`Error::NotFound`].
    pub async fn delete_contact(&self, id: u64, expected_revision: Option<u64>) -> Result<()> {
        let p = with_revision(format!("/contacts/~/v1/contacts/{id}"), expected_revision);
        let env: Envelope = self.json(Method::DELETE, &p, None).await?;
        check_ok(env.ok, env.error, env.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_envelope_decodes() {
        let raw = r#"{"ok":true,"contact":{"id":5,"name":"Ada","email":"ada@example.com","ship":"~zod","revision":1}}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        let c = env.into_contact("/p").unwrap();
        assert_eq!(c.name, "Ada");
        assert_eq!(c.ship, "~zod");
        assert_eq!(c.revision, 1);
    }

    #[test]
    fn list_envelope_defaults_missing_fields() {
        let raw = r#"{"ok":true,"contacts":[{"id":1,"name":"Bob"}]}"#;
        let env: Envelope = serde_json::from_str(raw).unwrap();
        check_ok(env.ok, env.error.clone(), env.message.clone()).unwrap();
        let list = env.contacts.unwrap();
        assert_eq!(list[0].email, "");
        assert_eq!(list[0].revision, 0);
    }

    #[test]
    fn conflict_envelope_maps_to_conflict() {
        let env: Envelope = serde_json::from_str(
            r#"{"ok":false,"error":"conflict","message":"stale expectedRevision"}"#,
        )
        .unwrap();
        assert!(matches!(
            env.into_contact("/p"),
            Err(Error::Conflict { current: None })
        ));
    }

    #[test]
    fn patch_serializes_only_set_fields() {
        let patch = ContactPatch {
            email: Some("new@example.com".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            json!({ "email": "new@example.com" })
        );
    }
}
