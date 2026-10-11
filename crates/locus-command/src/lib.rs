//! Named functions for clan identity and OS-mutating commands.
//!
//! The ship is source of truth (grants, pending jobs, audit). This crate
//! is the host-side skin: CLI, session, and the nixos-rebuild wrapper that
//! only runs after the ship says a job is approved.

use locus_protocol::{AuditEntry, Client, Envelope, Error as ProtoError, Job, Member};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const APP_ID: &str = "org.locus.household";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Protocol(#[from] ProtoError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Denied(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedSession {
    pub eyre_url: String,
    pub api_key: Option<String>,
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_ID)
}

pub fn session_path() -> PathBuf {
    data_dir().join("session.json")
}

pub fn load_saved() -> SavedSession {
    let raw = std::fs::read_to_string(session_path()).ok();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| SavedSession {
            eyre_url: std::env::var("LOCUS_EYRE_URL")
                .or_else(|_| std::env::var("NOTES_EYRE_URL"))
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
            api_key: None,
        })
}

pub fn save_session(eyre_url: &str, api_key: Option<&str>) {
    let path = session_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let saved = SavedSession {
        eyre_url: eyre_url.to_string(),
        api_key: api_key.map(str::to_string),
    };
    let _ = std::fs::write(
        path,
        serde_json::to_string_pretty(&saved).unwrap_or_default(),
    );
}

pub struct Session {
    client: Client,
}

impl Session {
    pub fn login(eyre_url: &str, web_code: &str) -> Result<Self> {
        let mut client = Client::new(eyre_url);
        client.login(web_code)?;
        save_session(client.base_url(), client.api_key());
        Ok(Self { client })
    }

    pub fn connect_saved() -> Result<Self> {
        let saved = load_saved();
        let key = saved
            .api_key
            .filter(|k| !k.is_empty())
            .ok_or(Error::Denied("not logged in; run: locus login"))?;
        Ok(Self {
            client: Client::with_api_key(&saved.eyre_url, &key),
        })
    }

    pub fn connect_with_api_key(eyre_url: &str, api_key: &str) -> Self {
        Self {
            client: Client::with_api_key(eyre_url, api_key),
        }
    }

    pub fn members(&self) -> Result<Vec<Member>> {
        Ok(self.client.members()?)
    }

    pub fn whoami(&self) -> Result<Member> {
        Ok(self.client.whoami()?)
    }

    pub fn issue_key(&self, ship: &str) -> Result<Envelope> {
        self.call(
            "household.issue-key",
            None,
            serde_json::json!({ "ship": ship }),
        )
    }

    pub fn audit(&self) -> Result<Vec<AuditEntry>> {
        Ok(self.client.audit()?)
    }

    pub fn pending(&self) -> Result<Vec<Job>> {
        Ok(self.client.pending()?)
    }

    pub fn jobs(&self) -> Result<Vec<Job>> {
        Ok(self.client.jobs()?)
    }

    pub fn call(
        &self,
        fn_name: &str,
        as_ship: Option<&str>,
        args: serde_json::Value,
    ) -> Result<Envelope> {
        Ok(self.client.call(fn_name, as_ship, args)?)
    }

    pub fn add_member(&self, ship: &str, name: &str, role: &str) -> Result<Envelope> {
        self.call(
            "household.add",
            None,
            serde_json::json!({ "ship": ship, "name": name, "role": role }),
        )
    }

    pub fn grant(&self, ship: &str, role: &str) -> Result<Envelope> {
        self.call(
            "household.grant",
            None,
            serde_json::json!({ "ship": ship, "role": role }),
        )
    }

    pub fn queue_rebuild(&self, flake: &str, as_ship: Option<&str>) -> Result<Envelope> {
        self.call(
            "os.rebuild",
            as_ship,
            serde_json::json!({ "flake": flake }),
        )
    }

    pub fn queue_rollback(&self, as_ship: Option<&str>) -> Result<Envelope> {
        self.call("os.rollback", as_ship, serde_json::json!({}))
    }

    pub fn approve(&self, id: u64, as_ship: Option<&str>) -> Result<Envelope> {
        self.call("os.approve", as_ship, serde_json::json!({ "id": id }))
    }

    pub fn deny(&self, id: u64, as_ship: Option<&str>) -> Result<Envelope> {
        self.call("os.deny", as_ship, serde_json::json!({ "id": id }))
    }

    pub fn mark_executed(&self, id: u64) -> Result<Envelope> {
        self.call("os.executed", None, serde_json::json!({ "id": id }))
    }
}

/// Host-side wrapper. Never runs a mutating rebuild unless `LOCUS_OS_EXECUTE=1`.
pub fn os_argv(function: &str, flake: &str, execute: bool) -> Result<Vec<String>> {
    match function {
        "os.rebuild" => {
            if execute {
                Ok(vec![
                    "nixos-rebuild".into(),
                    "switch".into(),
                    "--flake".into(),
                    flake.into(),
                ])
            } else {
                Ok(vec![
                    "nixos-rebuild".into(),
                    "dry-activate".into(),
                    "--flake".into(),
                    flake.into(),
                ])
            }
        }
        "os.rollback" => {
            if execute {
                Ok(vec!["nixos-rebuild".into(), "--rollback".into()])
            } else {
                Ok(vec![
                    "nixos-rebuild".into(),
                    "dry-activate".into(),
                    "--rollback".into(),
                ])
            }
        }
        _ => Err(Error::Denied("not an OS-mutating function")),
    }
}

pub fn execute_allowed() -> bool {
    matches!(std::env::var("LOCUS_OS_EXECUTE"), Ok(v) if v == "1")
}

/// Run an approved job on this machine. Dry-activate unless execute is allowed.
pub fn run_approved_job(job: &Job, execute: bool) -> Result<String> {
    if job.status != "approved" {
        return Err(Error::Denied("job is not approved on the ship"));
    }
    if execute && !execute_allowed() {
        return Err(Error::Denied(
            "refusing mutating rebuild; set LOCUS_OS_EXECUTE=1",
        ));
    }
    let flake = job
        .args
        .get("flake")
        .and_then(|v| v.as_str())
        .unwrap_or(".#server");
    let argv = os_argv(&job.function, flake, execute)?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    if let Some(dir) = flake_dir(flake) {
        cmd.current_dir(dir);
    }
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(format!("{stdout}{stderr}"))
    } else {
        Err(Error::Io(std::io::Error::other(format!(
            "nixos-rebuild failed: {stderr}{stdout}"
        ))))
    }
}

fn flake_dir(flake: &str) -> Option<&Path> {
    let path = flake.split('#').next()?;
    let p = Path::new(path);
    if p.is_dir() {
        Some(p)
    } else {
        p.parent()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebuild_is_dry_without_execute() {
        let argv = os_argv("os.rebuild", ".#server", false).unwrap();
        assert_eq!(argv[1], "dry-activate");
        assert!(!argv.contains(&"switch".into()));
    }

    #[test]
    fn rebuild_switch_only_when_execute() {
        let argv = os_argv("os.rebuild", ".#server", true).unwrap();
        assert_eq!(argv[1], "switch");
    }

    #[test]
    fn execute_job_rejects_unapproved() {
        let job = Job {
            id: 1,
            actor: "~dev".into(),
            function: "os.rebuild".into(),
            args: serde_json::json!({"flake":".#server"}),
            status: "pending".into(),
            approved_by: None,
            result: "queued".into(),
        };
        let err = run_approved_job(&job, false).unwrap_err();
        assert!(err.to_string().contains("not approved"));
    }
}
