//! Drives the real `loc` binary through the whole core.

use std::process::{Command, Output};
use tempfile::TempDir;

fn loc(dir: &TempDir, who: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_loc"));
    c.arg("--dir").arg(dir.path());
    if let Some(w) = who {
        c.args(["--as", w]);
    }
    c.args(args).output().unwrap()
}

fn ok(dir: &TempDir, who: Option<&str>, args: &[&str]) -> String {
    let o = loc(dir, who, args);
    assert!(
        o.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn refused(dir: &TempDir, who: Option<&str>, args: &[&str]) -> String {
    let o = loc(dir, who, args);
    assert!(!o.status.success(), "{args:?} should have failed");
    String::from_utf8(o.stderr).unwrap()
}

#[test]
fn the_whole_core_from_the_shell() {
    let (s, a, b) = (
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
        TempDir::new().unwrap(),
    );
    let admin = ok(&s, None, &["init", "home", "Admin"]);
    let alice = ok(&s, None, &["member", "Alice"]);
    let bob = ok(&s, None, &["member", "Bob"]);
    let bot = ok(&s, None, &["agent", "Bot"]);
    for who in [&alice, &bob] {
        for action in ["read", "object.*", "body.*", "blob.put", "proposal.*"] {
            ok(&s, None, &["grant", who, action]);
        }
    }
    ok(&s, None, &["grant", &bot, "read"]);
    ok(
        &s,
        None,
        &["grant", &bot, "object.set", "--mode", "propose"],
    );
    let sdir = s.path().to_str().unwrap();
    ok(&a, Some(&alice), &["join", sdir, "home", "alice-laptop"]);
    ok(&b, Some(&bob), &["join", sdir, "home", "bob-phone"]);

    // A note made on one client reaches the other through the server.
    let note = ok(
        &a,
        None,
        &[
            "create",
            "note",
            "title=Trip",
            "--body",
            "<p id=\"b1\">hi</p>",
        ],
    );
    assert!(ok(&a, None, &["pending"]).contains("object.create"));
    assert!(ok(&a, None, &["sync"]).contains("accepted 1"));
    ok(&b, None, &["sync"]);
    assert!(ok(&b, None, &["list", "note"]).contains("Trip"));

    // Both edit without syncing: a conflict that keeps both.
    ok(&a, None, &["set", &note, "title=Alice's"]);
    ok(&b, None, &["set", &note, "title=Bob's"]);
    ok(&a, None, &["sync"]);
    ok(&b, None, &["sync"]);
    ok(&a, None, &["sync"]);
    let conflicts = ok(&a, None, &["conflicts"]);
    assert!(
        conflicts.contains("Alice's") && conflicts.contains("Bob's"),
        "{conflicts}"
    );
    let cid = conflicts.rsplit(' ').next().unwrap().to_string();
    ok(&a, None, &["resolve", &note, &cid, "--take"]);
    ok(&a, None, &["sync"]);
    ok(&b, None, &["sync"]);
    assert!(ok(&b, None, &["get", &note]).contains("Bob's"));
    assert_eq!(ok(&b, None, &["conflicts"]), "");

    // A refusal, audited at the server.
    let mallory = ok(&s, None, &["member", "Mallory"]);
    assert!(refused(&s, Some(&mallory), &["set", &note, "title=mine"]).contains("no grant"));
    assert!(ok(&s, None, &["audit"]).contains("refused object.set"));

    // Delete and recover.
    ok(&s, Some(&alice), &["delete", &note]);
    assert!(
        ok(&s, Some(&alice), &["deleted"]).contains("Trip")
            || ok(&s, Some(&alice), &["deleted"]).contains(&note)
    );
    let history = ok(&s, Some(&alice), &["history", &note]);
    let del = history
        .lines()
        .last()
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_string();
    ok(&s, Some(&alice), &["revert", &del, &note]);
    assert_eq!(ok(&s, Some(&alice), &["deleted"]), "");

    // An agent's edit becomes a proposal, reviewed as a diff.
    let out = ok(
        &s,
        None,
        &["--as", &bot, "--for", &alice, "set", &note, "title=Better"],
    );
    assert!(out.contains("proposed"), "{out}");
    let p = out
        .split_whitespace()
        .nth(1)
        .unwrap()
        .trim_end_matches(':')
        .to_string();
    let review = ok(&s, Some(&alice), &["review", &p]);
    assert!(review.contains("-> \"Better\""), "{review}");
    assert!(ok(&s, Some(&alice), &["proposals"]).contains(&p));
    ok(&s, Some(&alice), &["approve", &p]);
    assert!(ok(&s, Some(&alice), &["get", &note]).contains("Better"));

    // Blobs, a purge, and a view.
    std::fs::write(a.path().join("f.bin"), b"bytes").unwrap();
    let h = ok(
        &a,
        None,
        &["blob-put", a.path().join("f.bin").to_str().unwrap()],
    );
    let file = ok(
        &a,
        None,
        &["create", "file", "title=f", &format!("blob=blob:{h}")],
    );
    ok(&a, None, &["sync"]);
    ok(&b, None, &["sync"]);
    let out = b.path().join("out.bin");
    ok(&b, None, &["blob-get", &h, out.to_str().unwrap()]);
    assert_eq!(std::fs::read(out).unwrap(), b"bytes");
    let secret = ok(
        &s,
        Some(&alice),
        &["create", "note", "title=password hunter2"],
    );
    assert!(refused(&s, Some(&alice), &["purge", &secret]).contains("no grant"));
    ok(&s, Some(&admin), &["purge", &secret]);
    assert!(!ok(&s, Some(&admin), &["history", &secret]).contains("hunter2"));
    let view = ok(&s, Some(&admin), &["view", "Files", r#"{"type":"file"}"#]);
    assert!(ok(&s, Some(&admin), &["show-view", &view]).contains(&file));
}
