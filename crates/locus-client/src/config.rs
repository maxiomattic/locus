//! `~/.config/locus/desktop.json`: the gateway address and ship code used at
//! the next start. The file is written with mode 0600 because the code is a
//! login secret. `LOCUS_SERVER` and `LOCUS_CODE` override it for one run and
//! are never written back.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

/// The saved gateway address and ship code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub server: String,
    #[serde(default)]
    pub code: String,
}

/// Where the settings file lives, when the platform has a config directory.
pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("locus").join("desktop.json"))
}

/// The saved settings with the environment overrides applied.
pub fn load() -> Settings {
    let mut settings = path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .unwrap_or_default();
    for (var, field) in [
        ("LOCUS_SERVER", &mut settings.server),
        ("LOCUS_CODE", &mut settings.code),
    ] {
        if let Ok(value) = std::env::var(var)
            && !value.trim().is_empty()
        {
            *field = value.trim().to_string();
        }
    }
    settings
}

/// Write the settings with mode 0600.
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("no config directory")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(&body).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())
}
