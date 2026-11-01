//! End to end: a home server and two clients, over the in-process transport.

use locus_model::proposal::render;
use locus_model::*;
use locus_store::Store;
use locus_sync::*;
use tempfile::TempDir;

struct World {
    _dirs: Vec<TempDir>,
    server: HomeServer,
    admin: Principal,
    alice: Principal,
    bob: Principal,
}

fn grant(server: &HomeServer, admin: &Principal, who: Id, action: &str, extra: Fields) -> Id {
    let id = Id::new();
    server.with_store(|s| {
        let mut f = fields([("subject", who.into()), ("action", action.into())]);
        f.extend(extra);
        let op = s.create_op(id, "locus.grant", f, None, admin.actor);
        s.execute(op, admin).unwrap();
    });
    id
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path(), "home").unwrap();
    let admin = Principal::acting(s.genesis("Admin").unwrap());
    let mut add = |name: &str| {
        let id = Id::new();
        let op = s.create_op(
            id,
            "locus.member",
            fields([("title", name.into())]),
            None,
            admin.actor,
        );
        s.execute(op, &admin).unwrap();
        Principal::acting(id)
    };
    let alice = add("Alice");
    let bob = add("Bob");
    let server = HomeServer::new(s);
    for who in [alice.actor, bob.actor] {
        for a in ["read", "object.*", "note.*", "body.*", "blob.put"] {
            grant(&server, &admin, who, a, Fields::new());
        }
    }
    World {
        _dirs: vec![dir],
        server,
        admin,
        alice,
        bob,
    }
}

impl World {
    fn client(&mut self, who: Principal, name: &str) -> (Replica, Network) {
        self.client_with(who, name, Subscription::everything())
    }

    fn client_with(&mut self, who: Principal, name: &str, sub: Subscription) -> (Replica, Network) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path(), name).unwrap();
        self._dirs.push(dir);
        let mut r = Replica::new(store, who, name);
        let net = Network::default();
        r.connect(
            "home",
            Box::new(LocalTransport::new(self.server.clone(), who, net.clone())),
            sub,
        );
        (r, net)
    }
}

fn title(r: &Replica, id: Id) -> String {
    r.get(id)
        .unwrap()
        .unwrap()
        .object
        .title()
        .unwrap()
        .to_string()
}

fn t(v: &str) -> Fields {
    fields([("title", v.into())])
}

#[test]
fn two_clients_and_a_server() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a
        .create("home", "note", t("hello"), Some("<p>x</p>"), "new note")
        .unwrap();
    // It applies at once, and stays provisional until the server has it.
    assert_eq!(title(&a, id), "hello");
    assert!(a.get(id).unwrap().unwrap().provisional);
    assert!(b.get(id).unwrap().is_none());
    a.sync().unwrap();
    assert!(!a.get(id).unwrap().unwrap().provisional);
    b.sync().unwrap();
    assert_eq!(title(&b, id), "hello");
    b.set(id, t("edited by b"), "edit").unwrap();
    b.sync().unwrap();
    a.sync().unwrap();
    assert_eq!(title(&a, id), "edited by b");
    w.server.with_store(|s| assert_eq!(s.rebuild().unwrap(), 0));
    let history = w.server.with_store(|s| s.history(id).unwrap());
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].op.actor, bob.actor);
    assert_eq!(
        a.store().get(id).unwrap().unwrap(),
        b.store().get(id).unwrap().unwrap()
    );
}

#[test]
fn offline_edits_stay_provisional_until_accepted() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a.create("home", "note", t("one"), None, "").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_a.set_online(false);
    a.set(id, t("two"), "offline edit").unwrap();
    a.set(id, t("three"), "offline edit").unwrap();
    let offline_new = a
        .create("home", "note", t("made offline"), None, "")
        .unwrap();
    assert!(matches!(a.sync(), Err(SyncError::Offline(_))));
    assert_eq!(title(&a, id), "three");
    assert_eq!(a.provisional().unwrap().len(), 3);
    assert!(a.get(id).unwrap().unwrap().provisional);
    assert_eq!(
        w.server.with_store(|s| s.get(id).unwrap()).unwrap().title(),
        Some("one")
    );
    net_a.set_online(true);
    let r = a.sync().unwrap();
    assert_eq!(r.accepted, 3);
    assert_eq!(a.provisional().unwrap().len(), 0);
    assert!(!a.get(id).unwrap().unwrap().provisional);
    b.sync().unwrap();
    assert_eq!(title(&b, id), "three");
    assert_eq!(title(&b, offline_new), "made offline");
    assert!(
        w.server
            .with_store(|s| s.get(id).unwrap())
            .unwrap()
            .conflicts
            .is_empty()
    );
}

#[test]
fn a_conflict_keeps_both_versions_everywhere() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a.create("home", "note", t("base"), None, "").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_a.set_online(false);
    a.set(id, t("alice 3pm"), "move it").unwrap();
    b.set(id, t("bob 4pm"), "move it").unwrap();
    b.sync().unwrap();
    net_a.set_online(true);
    a.sync().unwrap();
    b.sync().unwrap();
    for r in [&a, &b] {
        let o = r.get(id).unwrap().unwrap().object;
        assert_eq!(
            o.title(),
            Some("bob 4pm"),
            "the server's first stays current"
        );
        assert_eq!(o.conflicts.len(), 1);
        assert_eq!(o.conflicts[0].value, Some("alice 3pm".into()));
        assert_eq!(o.conflicts[0].actor, alice.actor);
    }
    assert_eq!(a.conflicts().unwrap().len(), 1);
    // Someone decides, and that is an operation like any other.
    let c = a.get(id).unwrap().unwrap().object.conflicts[0].id.clone();
    a.act(
        id,
        "object.resolve",
        fields([("conflict", c.into()), ("take", true.into())]),
        "alice's time wins",
    )
    .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    for r in [&a, &b] {
        let o = r.get(id).unwrap().unwrap().object;
        assert_eq!(o.title(), Some("alice 3pm"));
        assert!(o.conflicts.is_empty());
    }
    // Neither version left the log.
    let json = serde_json::to_string(&w.server.with_store(|s| s.history(id).unwrap())).unwrap();
    assert!(json.contains("bob 4pm") && json.contains("alice 3pm"));
}

#[test]
fn pending_changes_replay_on_top_of_the_servers_order() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a
        .create(
            "home",
            "note",
            fields([("title", "t".into()), ("place", "x".into())]),
            None,
            "",
        )
        .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_a.set_online(false);
    a.set(id, t("mine"), "").unwrap();
    b.set(id, fields([("place", "theirs".into())]), "").unwrap();
    b.sync().unwrap();
    // Alice's device hears the server but has not sent yet.
    net_a.set_online(true);
    let home = ServerId::from("home");
    a.wait(&home, 0).unwrap();
    let v = a.get(id).unwrap().unwrap();
    assert!(v.provisional);
    assert_eq!(v.object.title(), Some("mine"));
    assert_eq!(v.object.fields["place"], Value::text("theirs"));
    a.sync().unwrap();
    assert!(!a.get(id).unwrap().unwrap().provisional);
    assert!(a.get(id).unwrap().unwrap().object.conflicts.is_empty());
}

#[test]
fn a_conflict_shows_before_the_server_has_ruled() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a.create("home", "note", t("base"), None, "").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_a.set_online(false);
    a.set(id, t("mine"), "").unwrap();
    b.set(id, t("theirs"), "").unwrap();
    b.sync().unwrap();
    net_a.set_online(true);
    a.wait(&ServerId::from("home"), 0).unwrap();
    let v = a.get(id).unwrap().unwrap();
    assert!(v.provisional);
    assert_eq!(v.object.title(), Some("theirs"));
    assert_eq!(v.object.conflicts[0].value, Some("mine".into()));
}

#[test]
fn a_lost_reply_does_not_apply_twice() {
    let mut w = world();
    let alice = w.alice;
    let (mut a, net) = w.client(alice, "a");
    let id = a.create("home", "note", t("x"), None, "").unwrap();
    a.sync().unwrap();
    a.act(
        id,
        "object.add",
        fields([("field", "tags".into()), ("value", "t".into())]),
        "",
    )
    .unwrap();
    net.lose_next_replies(1);
    assert!(a.sync().is_err());
    assert_eq!(a.provisional().unwrap().len(), 1);
    a.sync().unwrap();
    assert_eq!(a.provisional().unwrap().len(), 0);
    assert_eq!(w.server.with_store(|s| s.history(id).unwrap()).len(), 2);
}

#[test]
fn an_edit_of_something_deleted_while_offline_is_kept() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a.create("home", "note", t("base"), None, "").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_a.set_online(false);
    a.set(id, t("edited offline"), "").unwrap();
    b.act(id, "object.delete", Fields::new(), "clear it out")
        .unwrap();
    b.sync().unwrap();
    net_a.set_online(true);
    a.sync().unwrap();
    let o = a.get(id).unwrap().unwrap().object;
    assert!(o.deleted);
    assert_eq!(o.conflicts[0].value, Some("edited offline".into()));
    let c = o.conflicts[0].id.clone();
    a.act(
        id,
        "object.resolve",
        fields([("conflict", c.into()), ("take", true.into())]),
        "keep it",
    )
    .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    let o = b.get(id).unwrap().unwrap().object;
    assert!(!o.deleted);
    assert_eq!(o.title(), Some("edited offline"));
}

#[test]
fn a_deleted_object_can_be_recovered_from_another_client() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let id = a
        .create("home", "note", t("precious"), Some("<p>body</p>"), "")
        .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    let del = a.act(id, "object.delete", Fields::new(), "oops").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    assert!(b.get(id).unwrap().unwrap().object.deleted);
    b.revert(del, id, "bring it back").unwrap();
    assert!(!b.get(id).unwrap().unwrap().object.deleted);
    b.sync().unwrap();
    a.sync().unwrap();
    let o = a.get(id).unwrap().unwrap().object;
    assert!(!o.deleted);
    assert_eq!(o.body.as_deref(), Some("<p>body</p>"));
}

#[test]
fn an_overwrite_can_be_recovered_from_history_a_client_never_held() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let id = a.create("home", "note", t("old"), None, "").unwrap();
    let edit = {
        a.sync().unwrap();
        let e = a.set(id, t("new"), "").unwrap();
        a.sync().unwrap();
        e
    };
    let (mut b, _) = w.client(bob, "b");
    b.sync().unwrap();
    assert_eq!(
        b.store().history(id).unwrap().len(),
        1,
        "a snapshot, not the history"
    );
    b.revert(edit, id, "undo").unwrap();
    b.sync().unwrap();
    assert_eq!(title(&b, id), "old");
}

#[test]
fn a_refused_offline_edit_is_not_lost() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let (mut b, net_b) = w.client(bob, "b");
    let id = a.create("home", "note", t("shared"), None, "").unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_b.set_online(false);
    b.set(id, t("bob's work"), "edit").unwrap();
    // Bob's write permission goes while he is offline.
    let admin = w.admin;
    w.server.with_store(|s| {
        let grants = s.objects_of_type("locus.grant").unwrap();
        for g in grants {
            let subject = g.fields["subject"].as_ref_id();
            let action = g.fields["action"].as_text().unwrap().to_string();
            if subject == Some(bob.actor) && action != "read" {
                let op = s
                    .op_on(g.id, "object.delete", Fields::new(), &admin)
                    .unwrap();
                s.execute(op, &admin).unwrap();
            }
        }
    });
    net_b.set_online(true);
    let r = b.sync().unwrap();
    assert_eq!(r.refused, 1);
    assert_eq!(title(&b, id), "shared");
    assert!(!b.get(id).unwrap().unwrap().provisional);
    let refused = b.refused().unwrap();
    assert_eq!(refused.len(), 1);
    assert!(
        serde_json::to_string(&refused[0].op)
            .unwrap()
            .contains("bob's work")
    );
    assert!(refused[0].reason.contains("no grant"));
    let audit = w.server.with_store(|s| s.audit(1).unwrap());
    assert_eq!(audit[0].decision, "refused");
    assert_eq!(audit[0].actor, bob.actor.to_string());
}

#[test]
fn a_client_cannot_write_as_someone_else() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let id = a.create("home", "note", t("x"), None, "").unwrap();
    a.sync().unwrap();
    // Bob's connection, but Alice's name on the operation.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path(), "x").unwrap();
    let mut forged = Replica::new(store, alice, "x");
    forged.connect(
        "home",
        Box::new(LocalTransport::new(
            w.server.clone(),
            bob,
            Network::default(),
        )),
        Subscription::everything(),
    );
    forged.sync().unwrap();
    forged.set(id, t("forged"), "").unwrap();
    assert_eq!(forged.sync().unwrap().refused, 1);
    assert_eq!(
        w.server.with_store(|s| s.get(id).unwrap()).unwrap().title(),
        Some("x")
    );
}

#[test]
fn a_client_only_holds_what_it_may_read() {
    let mut w = world();
    let (alice, admin) = (w.alice, w.admin);
    let carol = Id::new();
    w.server.with_store(|s| {
        let op = s.create_op(carol, "locus.member", t("Carol"), None, admin.actor);
        s.execute(op, &admin).unwrap();
    });
    let (mut a, _) = w.client(alice, "a");
    let open = a.create("home", "note", t("open"), None, "").unwrap();
    let closed = a.create("home", "note", t("closed"), None, "").unwrap();
    a.sync().unwrap();
    let g = grant(
        &w.server,
        &admin,
        carol,
        "read",
        fields([("scope_object", open.into())]),
    );
    let (mut c, _) = w.client(Principal::acting(carol), "c");
    c.sync().unwrap();
    assert!(c.get(open).unwrap().is_some());
    assert!(c.get(closed).unwrap().is_none());
    assert!(
        !c.store()
            .conn()
            .query_row(
                "SELECT count(*) FROM log WHERE op LIKE '%closed%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .map(|n| n > 0)
            .unwrap()
    );
    // New edits flow, and a revoked grant removes the copy.
    a.set(open, t("open 2"), "").unwrap();
    a.sync().unwrap();
    c.sync().unwrap();
    assert_eq!(title(&c, open), "open 2");
    w.server.with_store(|s| {
        let op = s.op_on(g, "object.delete", Fields::new(), &admin).unwrap();
        s.execute(op, &admin).unwrap();
    });
    assert_eq!(c.sync().unwrap().evicted, 1);
    assert!(c.get(open).unwrap().is_none());
    let text = serde_json::to_string(&c.store().history(open).unwrap()).unwrap();
    assert!(!text.contains("open 2"), "{text}");
}

#[test]
fn a_client_can_ask_for_part_of_what_it_may_read() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, _) = w.client(alice, "a");
    let n = a.create("home", "note", t("n"), None, "").unwrap();
    let m = a.create("home", "task", t("m"), None, "").unwrap();
    a.sync().unwrap();
    let sub = Subscription {
        types: vec!["task".to_string()],
        ..Subscription::default()
    };
    let (mut b, _) = w.client_with(bob, "b", sub);
    b.sync().unwrap();
    assert!(b.get(m).unwrap().is_some());
    assert!(b.get(n).unwrap().is_none());
    let sub = Subscription {
        objects: vec![n],
        types: vec!["task".to_string()],
        all: false,
    };
    let server = w.server.clone();
    b.connect(
        "home",
        Box::new(LocalTransport::new(server, bob, Network::default())),
        sub,
    );
    b.sync().unwrap();
    assert!(b.get(n).unwrap().is_some());
}

#[test]
fn blobs_move_on_demand_and_are_gated() {
    let mut w = world();
    let (alice, bob, admin) = (w.alice, w.bob, w.admin);
    let (mut a, _) = w.client(alice, "a");
    let (mut b, _) = w.client(bob, "b");
    let bytes: Vec<u8> = (0..3_000_000u32).map(|n| n as u8).collect();
    let h = a.put_blob("home", &bytes).unwrap();
    let id = a
        .create(
            "home",
            "file",
            fields([("title", "big".into()), ("blob", Value::Blob(h))]),
            None,
            "",
        )
        .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    assert!(!b.store().has_blob(&h).unwrap(), "a client copy is partial");
    assert_eq!(b.blob("home", &h).unwrap(), bytes);
    assert!(b.store().has_blob(&h).unwrap());
    let carol = Id::new();
    w.server.with_store(|s| {
        let op = s.create_op(carol, "locus.member", t("Carol"), None, admin.actor);
        s.execute(op, &admin).unwrap();
    });
    let (mut c, _) = w.client(Principal::acting(carol), "c");
    assert!(matches!(c.blob("home", &h), Err(SyncError::Server(_))));
    let _ = id;
}

#[test]
fn a_purge_reaches_every_client() {
    let mut w = world();
    let (alice, bob, admin) = (w.alice, w.bob, w.admin);
    let (mut a, _) = w.client(alice, "a");
    let (mut b, net_b) = w.client(bob, "b");
    let id = a
        .create(
            "home",
            "note",
            fields([("title", "wifi".into()), ("password", "hunter2".into())]),
            None,
            "",
        )
        .unwrap();
    a.sync().unwrap();
    b.sync().unwrap();
    net_b.set_online(false);
    b.set(id, fields([("hint", "it was hunter2".into())]), "")
        .unwrap();
    w.server.with_store(|s| {
        let op = s
            .op_on(
                id,
                "object.purge",
                fields([("scope", "object".into())]),
                &admin,
            )
            .unwrap();
        s.execute(op, &admin).unwrap();
    });
    net_b.set_online(true);
    a.sync().unwrap();
    b.sync().unwrap();
    for r in [&a, &b] {
        let o = r.get(id).unwrap().unwrap().object;
        assert!(o.purged);
        let text = serde_json::to_string(&(
            r.store().history(id).unwrap(),
            r.provisional().unwrap(),
            r.refused().unwrap(),
        ))
        .unwrap();
        assert!(!text.contains("hunter2"), "{text}");
    }
}

#[test]
fn an_agents_proposal_syncs_and_is_reviewed_on_another_device() {
    let mut w = world();
    let (alice, admin) = (w.alice, w.admin);
    let bot = Id::new();
    w.server.with_store(|s| {
        let op = s.create_op(bot, "locus.agent", t("bot"), None, admin.actor);
        s.execute(op, &admin).unwrap();
    });
    grant(&w.server, &admin, alice.actor, "proposal.*", Fields::new());
    grant(&w.server, &admin, bot, "read", Fields::new());
    grant(
        &w.server,
        &admin,
        bot,
        "object.set",
        fields([("mode", "propose".into())]),
    );
    let (mut laptop, _) = w.client(alice, "laptop");
    let id = laptop.create("home", "note", t("draft"), None, "").unwrap();
    laptop.sync().unwrap();
    let agent = Principal::for_invoker(bot, alice.actor);
    let (mut ag, _) = w.client(agent, "agent");
    ag.sync().unwrap();
    // The agent's plain edit becomes a proposal at the server.
    ag.set(id, t("agent's title"), "better title").unwrap();
    assert_eq!(title(&ag, id), "agent's title");
    let r = ag.sync().unwrap();
    assert_eq!(r.proposed.len(), 1);
    assert_eq!(title(&ag, id), "draft", "the speculative change goes away");
    let p = r.proposed[0];
    // Explicit branches work offline too.
    let op = ag
        .draft(
            id,
            "object.set",
            fields([("fields", Value::Map(fields([("mood", "calm".into())])))]),
            "add mood",
        )
        .unwrap();
    let p2 = ag.propose("home", "two", &[op]).unwrap();
    ag.sync().unwrap();
    laptop.sync().unwrap();
    assert!(render(&laptop.review(p).unwrap()).contains(r#""draft" -> "agent's title""#));
    laptop
        .act(p, "proposal.approve", Fields::new(), "looks good")
        .unwrap();
    laptop
        .act(
            p2,
            "proposal.reject",
            fields([("reason", "no mood".into())]),
            "",
        )
        .unwrap();
    laptop.sync().unwrap();
    ag.sync().unwrap();
    assert_eq!(title(&laptop, id), "agent's title");
    assert_eq!(title(&ag, id), "agent's title");
    assert!(
        !laptop
            .get(id)
            .unwrap()
            .unwrap()
            .object
            .fields
            .contains_key("mood")
    );
    let last = w
        .server
        .with_store(|s| s.history(id).unwrap())
        .pop()
        .unwrap()
        .op;
    assert_eq!(
        (last.actor, last.invoker, last.approved_by),
        (bot, Some(alice.actor), Some(alice.actor))
    );
}

#[test]
fn a_second_home_is_kept_separately_and_shows_its_staleness() {
    let mut w = world();
    let alice = w.alice;
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path(), "other-clan").unwrap();
    let their_admin = Principal::acting(s.genesis("Them").unwrap());
    let shared = Id::new();
    let op = s.create_op(shared, "note", t("their note"), None, their_admin.actor);
    s.execute(op, &their_admin).unwrap();
    let f = fields([("title", "Alice".into())]);
    let op = s.create_op(alice.actor, "locus.member", f, None, their_admin.actor);
    s.execute(op, &their_admin).unwrap();
    let other = HomeServer::new(s);
    grant(
        &other,
        &their_admin,
        alice.actor,
        "read",
        fields([("scope_object", shared.into())]),
    );
    let (mut a, _) = w.client(alice, "a");
    let net2 = Network::default();
    a.connect(
        "other-clan",
        Box::new(LocalTransport::new(other.clone(), alice, net2.clone())),
        Subscription::everything(),
    );
    a.sync().unwrap();
    let mine = a
        .create(
            "home",
            "note",
            fields([
                ("title", "mine".into()),
                (
                    "about",
                    Value::Ref(Reference {
                        id: shared,
                        pin: None,
                        home: Some("other-clan".into()),
                    }),
                ),
            ]),
            None,
            "",
        )
        .unwrap();
    a.sync().unwrap();
    let r = a.get(mine).unwrap().unwrap().object.fields["about"].clone();
    let Value::Ref(r) = r else { panic!() };
    let cached = a.resolve(&r).unwrap().unwrap();
    assert_eq!(cached.object.title(), Some("their note"));
    assert!(cached.as_of.is_some());
    net2.set_online(false);
    other.with_store(|s| {
        let args = fields([("fields", Value::Map(t("changed")))]);
        let op = s.op_on(shared, "object.set", args, &their_admin).unwrap();
        s.execute(op, &their_admin).unwrap();
    });
    // Their server is unreachable, so the copy stays, and says when it's from.
    assert!(a.sync_home(&ServerId::from("other-clan")).is_err());
    let again = a.resolve(&r).unwrap().unwrap();
    assert_eq!(again.object.title(), Some("their note"));
    assert_eq!(again.as_of, cached.as_of);
    net2.set_online(true);
    a.sync_home(&ServerId::from("other-clan")).unwrap();
    assert_eq!(
        a.resolve(&r).unwrap().unwrap().object.title(),
        Some("changed")
    );
    // A write to their object is refused by their server, not by a neighbour.
    a.set(shared, t("mine now"), "").unwrap();
    assert_eq!(
        a.sync_home(&ServerId::from("other-clan")).unwrap().refused,
        1
    );
}

#[test]
fn ten_rounds_of_random_edits_converge() {
    let mut w = world();
    let (alice, bob) = (w.alice, w.bob);
    let (mut a, net_a) = w.client(alice, "a");
    let (mut b, net_b) = w.client(bob, "b");
    let ids: Vec<Id> = (0..5)
        .map(|n| {
            a.create("home", "note", t(&format!("n{n}")), None, "")
                .unwrap()
        })
        .collect();
    a.sync().unwrap();
    b.sync().unwrap();
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for round in 0..10 {
        net_a.set_online(rand() % 2 == 0);
        net_b.set_online(rand() % 2 == 0);
        for step in 0..20 {
            let r = if rand() % 2 == 0 { &mut a } else { &mut b };
            let id = ids[(rand() % 5) as usize];
            let v = format!("r{round}s{step}");
            match rand() % 4 {
                0 | 1 => drop(r.set(id, t(&v), "")),
                2 => drop(r.act(
                    id,
                    "object.add",
                    fields([("field", "tags".into()), ("value", v.into())]),
                    "",
                )),
                _ => drop(r.act(id, "object.delete", Fields::new(), "")),
            }
        }
        let _ = a.sync();
        let _ = b.sync();
    }
    net_a.set_online(true);
    net_b.set_online(true);
    a.sync().unwrap();
    b.sync().unwrap();
    a.sync().unwrap();
    assert_eq!(a.provisional().unwrap().len(), 0);
    assert_eq!(b.provisional().unwrap().len(), 0);
    w.server.with_store(|s| assert_eq!(s.rebuild().unwrap(), 0));
    for id in ids {
        let server = w.server.with_store(|s| s.get(id).unwrap()).unwrap();
        assert_eq!(a.store().get(id).unwrap().unwrap(), server);
        assert_eq!(b.store().get(id).unwrap().unwrap(), server);
    }
}
