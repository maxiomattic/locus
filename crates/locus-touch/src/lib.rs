//! Touch clients talk to the **same Eyre APIs** as the COSMIC apps.
//! Stock GrapheneOS (P5). Not a nested COSMIC session. P7 (AOSP fork) waits
//! until these surfaces exist.

/// OpenAPI paths a phone can fetch after login. Order: notes, then mail/calendar,
/// then photos/contacts, then the command plane.
pub const TOUCH_OPENAPI: &[&str] = &[
    "/notes/openapi.json",
    "/mail/openapi.json",
    "/calendar/openapi.json",
    "/photos/openapi.json",
    "/contacts/openapi.json",
    "/locus/openapi.json",
];

pub fn is_nested_cosmic_session() -> bool {
    false
}

/// Relative ZFS paths for photo bytes — same rule as the desktop catalog.
pub fn photo_rel(path: &str) -> photos_protocol::Result<&str> {
    photos_protocol::zfs_rel_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_ship_not_a_cosmic_session() {
        assert!(!is_nested_cosmic_session());
        assert!(TOUCH_OPENAPI.iter().any(|p| p.starts_with("/notes/")));
        assert!(TOUCH_OPENAPI.iter().any(|p| p.starts_with("/mail/")));
        assert!(TOUCH_OPENAPI.iter().any(|p| p.starts_with("/photos/")));
        assert!(TOUCH_OPENAPI.iter().all(|p| !p.contains("cosmic")));
    }

    #[test]
    fn photo_paths_match_desktop_catalog() {
        assert!(photo_rel("alb/x.jpg").is_ok());
        assert!(photo_rel("../x").is_err());
    }
}
