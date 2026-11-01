use locus_model::proposal::render;
use locus_model::*;
use locus_store::{Outcome, Store, StoreError};

struct Clan {
    _dir: tempfile::TempDir,
    s: Store,
    admin: Principal,
    alice: Principal,
    bob: Principal,
    bot: Id,
}

fn grant(s: &mut Store, admin: &Principal, who: Id, action: &str, mode: &str) {
    let f = fields([
        ("subject", who.into()),
        ("action", action.into()),
        ("mode", mode.into()),
    ]);
    let op = s.create_op(Id::new(), "locus.grant", f, None, admin.actor);
    s.execute(op, admin).unwrap();
}

fn person(s: &mut Store, admin: &Principal, name: &str) -> Principal {
    let id = Id::new();
    let op = s.create_op(
        id,
        "locus.member",
        fields([("title", name.into())]),
        None,
        admin.actor,
    );
    s.execute(op, admin).unwrap();
    Principal::acting(id)
}

fn clan() -> Clan {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(dir.path(), "home").unwrap();
    let admin = Principal::acting(s.genesis("Admin").unwrap());
    let alice = person(&mut s, &admin, "Alice");
    let bob = person(&mut s, &admin, "Bob");
    let bot = Id::new();
    let op = s.create_op(
        bot,
        "locus.agent",
        fields([("title", "bot".into())]),
        None,
        admin.actor,
    );
    s.execute(op, &admin).unwrap();
    for a in [
        "read",
        "object.*",
        "note.*",
        "body.*",
        "blob.put",
        "proposal.*",
    ] {
        grant(&mut s, &admin, alice.actor, a, "apply");
    }
    Clan {
        _dir: dir,
        s,
        admin,
        alice,
        bob,
        bot,
    }
}

fn note(c: &mut Clan, title: &str) -> Id {
    let id = Id::new();
    let op = c.s.create_op(
        id,
        "note",
        fields([("title", title.into())]),
        Some("<p>x</p>"),
        c.alice.actor,
    );
    c.s.execute(op, &c.alice.clone()).unwrap();
    id
}

fn set(c: &mut Clan, id: Id, who: &Principal, f: &str, v: Value) -> Result<Outcome, StoreError> {
    let args = fields([("fields", Value::Map(fields([(f, v)])))]);
    let op = c.s.op_on(id, "object.set", args, who)?;
    c.s.execute(op, who)
}

#[test]
fn every_action_is_logged_and_audited() {
    let mut c = clan();
    let id = note(&mut c, "a");
    let alice = c.alice;
    set(&mut c, id, &alice, "title", "b".into()).unwrap();
    assert_eq!(c.s.history(id).unwrap().len(), 2);
    let a = c.s.audit(2).unwrap();
    assert_eq!(a[0].action, "object.set");
    assert_eq!(a[0].decision, "applied");
    assert_eq!(a[0].actor, alice.actor.to_string());
}

#[test]
fn the_log_cannot_be_rewritten() {
    let mut c = clan();
    note(&mut c, "a");
    for sql in [
        "UPDATE log SET op='x'",
        "DELETE FROM log",
        "UPDATE audit SET actor='x'",
        "DELETE FROM audit",
    ] {
        assert!(c.s.conn().execute(sql, []).is_err(), "{sql}");
    }
}

#[test]
fn state_can_be_rebuilt_from_the_log() {
    let mut c = clan();
    let id = note(&mut c, "a");
    let alice = c.alice;
    set(&mut c, id, &alice, "title", "b".into()).unwrap();
    let del =
        c.s.op_on(id, "object.delete", Fields::new(), &alice)
            .unwrap();
    c.s.execute(del, &alice).unwrap();
    assert_eq!(c.s.rebuild().unwrap(), 0);
    assert!(c.s.get(id).unwrap().unwrap().deleted);
}

#[test]
fn a_retried_operation_applies_once() {
    let mut c = clan();
    let id = note(&mut c, "a");
    let alice = c.alice;
    let op =
        c.s.op_on(
            id,
            "object.add",
            fields([("field", "tags".into()), ("value", "t".into())]),
            &alice,
        )
        .unwrap();
    c.s.execute(op.clone(), &alice).unwrap();
    assert!(matches!(
        c.s.execute(op, &alice).unwrap(),
        Outcome::Duplicate { .. }
    ));
    assert_eq!(c.s.history(id).unwrap().len(), 2);
}

#[test]
fn a_delete_can_be_recovered() {
    let mut c = clan();
    let id = note(&mut c, "keep me");
    let alice = c.alice;
    let del =
        c.s.op_on(id, "object.delete", Fields::new(), &alice)
            .unwrap();
    let del_id = del.id;
    c.s.execute(del, &alice).unwrap();
    assert_eq!(c.s.deleted(&alice).unwrap().len(), 1);
    c.s.revert(del_id, &alice, "undo delete").unwrap();
    let o = c.s.get(id).unwrap().unwrap();
    assert!(!o.deleted);
    assert_eq!(o.title(), Some("keep me"));
    assert_eq!(o.body.as_deref(), Some("<p>x</p>"));
}

#[test]
fn an_overwrite_can_be_recovered_and_old_versions_read() {
    let mut c = clan();
    let id = note(&mut c, "first");
    let alice = c.alice;
    let Outcome::Applied { seq, .. } = set(&mut c, id, &alice, "title", "second".into()).unwrap()
    else {
        panic!()
    };
    let overwrite = c.s.log_since(seq - 1, 1).unwrap().remove(0).op.id;
    assert_eq!(
        c.s.object_at(id, 1).unwrap().unwrap().title(),
        Some("first")
    );
    c.s.revert(overwrite, &alice, "undo").unwrap();
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("first"));
    assert_eq!(c.s.history(id).unwrap().len(), 3);
}

#[test]
fn a_pinned_reference_keeps_its_version() {
    let mut c = clan();
    let id = note(&mut c, "v1");
    let alice = c.alice;
    set(&mut c, id, &alice, "title", "v2".into()).unwrap();
    let pinned = Reference {
        id,
        pin: Some(1),
        home: None,
    };
    assert_eq!(c.s.resolve(&pinned).unwrap().unwrap().title(), Some("v1"));
    assert_eq!(
        c.s.resolve(&Reference::to(id)).unwrap().unwrap().title(),
        Some("v2")
    );
}

#[test]
fn a_batch_undoes_together() {
    let mut c = clan();
    let batch = Id::new();
    let alice = c.alice;
    let mut ids = Vec::new();
    for n in 0..3 {
        let id = Id::new();
        let mut op = c.s.create_op(
            id,
            "note",
            fields([("title", format!("n{n}").into())]),
            None,
            alice.actor,
        );
        op.batch = Some(batch);
        c.s.execute(op, &alice).unwrap();
        ids.push(id);
    }
    assert_eq!(c.s.revert_batch(batch, &alice, "undo import").unwrap(), 3);
    assert!(ids.iter().all(|i| c.s.get(*i).unwrap().unwrap().deleted));
}

#[test]
fn a_purge_really_removes_the_data() {
    let mut c = clan();
    let id = note(&mut c, "password is hunter2");
    let alice = c.alice;
    let admin = c.admin;
    set(&mut c, id, &alice, "title", "password is hunter3".into()).unwrap();
    let del =
        c.s.op_on(id, "object.delete", Fields::new(), &alice)
            .unwrap();
    c.s.execute(del, &alice).unwrap();
    // Alice has object.*, but a wildcard does not reach purge.
    let op =
        c.s.op_on(
            id,
            "object.purge",
            fields([("scope", "object".into())]),
            &alice,
        )
        .unwrap();
    assert!(matches!(
        c.s.execute(op, &alice),
        Err(StoreError::Refused(_))
    ));
    let op =
        c.s.op_on(
            id,
            "object.purge",
            fields([("scope", "object".into())]),
            &admin,
        )
        .unwrap()
        .intent("leaked password");
    c.s.execute(op, &admin).unwrap();
    let o = c.s.get(id).unwrap().unwrap();
    assert!(o.purged && o.fields.is_empty() && o.body.is_none());
    for e in c.s.history(id).unwrap() {
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("hunter"), "{json}");
    }
    assert!(
        c.s.audit(100)
            .unwrap()
            .iter()
            .any(|a| a.action == "object.purge" && a.decision == "applied")
    );
    let raw = std::fs::read(c.s.dir().join("locus.db")).unwrap();
    assert!(!raw.windows(6).any(|w| w == b"hunter"));
    // The purged object cannot be touched again.
    assert!(set(&mut c, id, &alice, "title", "x".into()).is_err());
}

#[test]
fn a_field_purge_leaves_the_rest() {
    let mut c = clan();
    let id = note(&mut c, "visible");
    let alice = c.alice;
    let admin = c.admin;
    set(&mut c, id, &alice, "wifi", "s3cret".into()).unwrap();
    let op =
        c.s.op_on(
            id,
            "object.purge",
            fields([("scope", "field".into()), ("field", "wifi".into())]),
            &admin,
        )
        .unwrap();
    c.s.execute(op, &admin).unwrap();
    let o = c.s.get(id).unwrap().unwrap();
    assert_eq!(o.title(), Some("visible"));
    assert!(!o.fields.contains_key("wifi"));
    let json = serde_json::to_string(&c.s.history(id).unwrap()).unwrap();
    assert!(!json.contains("s3cret"), "{json}");
}

#[test]
fn a_purged_blob_is_gone_and_stays_gone() {
    let mut c = clan();
    let alice = c.alice;
    let admin = c.admin;
    let h = c.s.put_blob_as(&alice, b"secret scan").unwrap();
    let id = Id::new();
    let op = c.s.create_op(
        id,
        "file",
        fields([("title", "scan".into()), ("blob", Value::Blob(h))]),
        None,
        alice.actor,
    );
    c.s.execute(op, &alice).unwrap();
    assert_eq!(c.s.get_blob(&h).unwrap(), b"secret scan");
    let op =
        c.s.op_on(
            id,
            "object.purge",
            fields([("scope", "blob".into()), ("blob", Value::Blob(h))]),
            &admin,
        )
        .unwrap();
    c.s.execute(op, &admin).unwrap();
    assert!(c.s.get_blob(&h).is_err());
    assert!(c.s.put_blob(b"secret scan").is_err());
    assert_eq!(
        c.s.get(id).unwrap().unwrap().fields["blob"],
        Value::Redacted
    );
}

#[test]
fn blobs_are_verified_and_gated_by_object_reads() {
    let mut c = clan();
    let alice = c.alice;
    let h = c.s.put_blob_as(&alice, b"photo").unwrap();
    assert!(c.s.put_blob_as(&c.bob, b"nope").is_err());
    let id = Id::new();
    let op = c.s.create_op(
        id,
        "photo",
        fields([("blob", Value::Blob(h))]),
        None,
        alice.actor,
    );
    c.s.execute(op, &alice).unwrap();
    assert!(c.s.may_read_blob(&alice, &h).unwrap());
    assert!(!c.s.may_read_blob(&c.bob, &h).unwrap());
    std::fs::write(c.s.blob_path(&h), b"tampered").unwrap();
    assert!(c.s.get_blob(&h).is_err());
}

#[test]
fn refusals_are_audited_and_change_nothing() {
    let mut c = clan();
    let id = note(&mut c, "mine");
    let bob = c.bob;
    assert!(matches!(
        set(&mut c, id, &bob, "title", "x".into()),
        Err(StoreError::Refused(_))
    ));
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("mine"));
    let a = &c.s.audit(1).unwrap()[0];
    assert_eq!(a.decision, "refused");
    assert_eq!(a.actor, bob.actor.to_string());
    assert!(c.s.get_as(&bob, id).is_err());
    assert_eq!(c.s.audit(1).unwrap()[0].action, "read");
}

#[test]
fn nobody_can_act_as_someone_else() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let (alice, bob) = (c.alice, c.bob);
    let forged =
        c.s.op_on(id, "object.delete", Fields::new(), &alice)
            .unwrap();
    assert!(c.s.execute(forged, &bob).is_err());
}

#[test]
fn only_an_admin_changes_permissions() {
    let mut c = clan();
    let alice = c.alice;
    let f = fields([("subject", alice.actor.into()), ("action", "*".into())]);
    let op =
        c.s.create_op(Id::new(), "locus.grant", f, None, alice.actor);
    assert!(matches!(
        c.s.execute(op, &alice),
        Err(StoreError::Refused(_))
    ));
}

#[test]
fn a_grant_on_one_object_covers_only_that_object() {
    let mut c = clan();
    let (shared, private) = (note(&mut c, "s"), note(&mut c, "p"));
    let (admin, bob) = (c.admin, c.bob);
    let f = fields([
        ("subject", bob.actor.into()),
        ("action", "read".into()),
        ("scope_object", shared.into()),
    ]);
    let op =
        c.s.create_op(Id::new(), "locus.grant", f, None, admin.actor);
    c.s.execute(op, &admin).unwrap();
    assert!(c.s.get_as(&bob, shared).is_ok());
    assert!(c.s.get_as(&bob, private).is_err());
    let seen =
        c.s.view(&bob, &ViewSpec::of(Filter::Type("note".into())))
            .unwrap();
    assert_eq!(seen.len(), 1);
}

#[test]
fn permission_changes_are_audited_and_reversible() {
    let mut c = clan();
    let (admin, bob) = (c.admin, c.bob);
    let id = note(&mut c, "x");
    let g = Id::new();
    let f = fields([("subject", bob.actor.into()), ("action", "read".into())]);
    let op = c.s.create_op(g, "locus.grant", f, None, admin.actor);
    c.s.execute(op, &admin).unwrap();
    assert!(c.s.get_as(&bob, id).is_ok());
    let del =
        c.s.op_on(g, "object.delete", Fields::new(), &admin)
            .unwrap();
    c.s.execute(del, &admin).unwrap();
    assert!(c.s.get_as(&bob, id).is_err());
}

fn agent_setup(c: &mut Clan) -> Principal {
    let (admin, bot, alice) = (c.admin, c.bot, c.alice);
    grant(&mut c.s, &admin, bot, "read", "apply");
    grant(&mut c.s, &admin, bot, "object.set", "propose");
    grant(&mut c.s, &admin, bot, "note.rename", "apply");
    Principal::for_invoker(bot, alice.actor)
}

#[test]
fn an_agents_propose_grant_branches_instead_of_applying() {
    let mut c = clan();
    let id = note(&mut c, "orig");
    let who = agent_setup(&mut c);
    let args = fields([("fields", Value::Map(fields([("title", "agent's".into())])))]);
    let op =
        c.s.op_on(id, "object.set", args, &who)
            .unwrap()
            .intent("tidy the title");
    let Outcome::Proposed { proposal, .. } = c.s.execute(op, &who).unwrap() else {
        panic!("should be proposed");
    };
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("orig"));
    let alice = c.alice;
    let review = c.s.review(proposal, &alice).unwrap();
    let text = render(&review);
    assert!(text.contains(r#"title: "orig" -> "agent's""#), "{text}");
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("orig"));
    let approve =
        c.s.op_on(proposal, "proposal.approve", Fields::new(), &alice)
            .unwrap();
    c.s.execute(approve, &alice).unwrap();
    let o = c.s.get(id).unwrap().unwrap();
    assert_eq!(o.title(), Some("agent's"));
    let last = c.s.history(id).unwrap().pop().unwrap().op;
    assert_eq!(last.actor, c.bot);
    assert_eq!(last.invoker, Some(alice.actor));
    assert_eq!(last.approved_by, Some(alice.actor));
    assert_eq!(last.proposal, Some(proposal));
}

#[test]
fn a_rejected_proposal_applies_nothing_and_stays_on_record() {
    let mut c = clan();
    let id = note(&mut c, "orig");
    let who = agent_setup(&mut c);
    let args = fields([("fields", Value::Map(fields([("title", "bad".into())])))]);
    let op = c.s.op_on(id, "object.set", args, &who).unwrap();
    let p = c.s.propose("rename", &[op], &who).unwrap();
    let alice = c.alice;
    let reject =
        c.s.op_on(
            p,
            "proposal.reject",
            fields([("reason", "no".into())]),
            &alice,
        )
        .unwrap();
    c.s.execute(reject, &alice).unwrap();
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("orig"));
    let doc = c.s.get(p).unwrap().unwrap();
    assert_eq!(doc.fields["status"], Value::text("rejected"));
    let again =
        c.s.op_on(p, "proposal.approve", Fields::new(), &alice)
            .unwrap();
    assert!(c.s.execute(again, &alice).is_err());
}

#[test]
fn a_stale_proposal_keeps_both_versions() {
    let mut c = clan();
    let id = note(&mut c, "orig");
    let who = agent_setup(&mut c);
    let args = fields([("fields", Value::Map(fields([("title", "agent".into())])))]);
    let op = c.s.op_on(id, "object.set", args, &who).unwrap();
    let p = c.s.propose("rename", &[op], &who).unwrap();
    let alice = c.alice;
    set(&mut c, id, &alice, "title", "human".into()).unwrap();
    let text = render(&c.s.review(p, &alice).unwrap());
    assert!(text.contains("conflict"), "{text}");
    let approve =
        c.s.op_on(p, "proposal.approve", Fields::new(), &alice)
            .unwrap();
    c.s.execute(approve, &alice).unwrap();
    let o = c.s.get(id).unwrap().unwrap();
    assert_eq!(o.title(), Some("human"));
    assert_eq!(o.conflicts[0].value, Some("agent".into()));
}

#[test]
fn part_of_a_proposal_can_be_approved() {
    let mut c = clan();
    let (a, b) = (note(&mut c, "a"), note(&mut c, "b"));
    let who = agent_setup(&mut c);
    let mk = |c: &Clan, id, t: &str| {
        let args = fields([("fields", Value::Map(fields([("title", t.into())])))]);
        c.s.op_on(id, "object.set", args, &who).unwrap()
    };
    let ops = [mk(&c, a, "a2"), mk(&c, b, "b2")];
    let p = c.s.propose("two renames", &ops, &who).unwrap();
    let alice = c.alice;
    let approve =
        c.s.op_on(
            p,
            "proposal.approve",
            fields([("only", Value::List(vec![Value::Int(1)]))]),
            &alice,
        )
        .unwrap();
    c.s.execute(approve, &alice).unwrap();
    assert_eq!(c.s.get(a).unwrap().unwrap().title(), Some("a"));
    assert_eq!(c.s.get(b).unwrap().unwrap().title(), Some("b2"));
}

#[test]
fn an_agent_cannot_approve_and_needs_an_able_reviewer() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let who = agent_setup(&mut c);
    let args = fields([("fields", Value::Map(fields([("title", "y".into())])))]);
    let op = c.s.op_on(id, "object.set", args, &who).unwrap();
    let p = c.s.propose("x", &[op], &who).unwrap();
    let approve =
        c.s.op_on(p, "proposal.approve", Fields::new(), &who)
            .unwrap();
    assert!(matches!(
        c.s.execute(approve, &who),
        Err(StoreError::Refused(_))
    ));
    let bob = c.bob;
    let approve =
        c.s.op_on(p, "proposal.approve", Fields::new(), &bob)
            .unwrap();
    assert!(matches!(
        c.s.execute(approve, &bob),
        Err(StoreError::Refused(_))
    ));
    assert_eq!(c.s.get(id).unwrap().unwrap().title(), Some("x"));
}

#[test]
fn a_proposal_cannot_exceed_its_author() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let who = agent_setup(&mut c);
    let del = c.s.op_on(id, "object.delete", Fields::new(), &who).unwrap();
    assert!(c.s.propose("delete it", &[del], &who).is_err());
    // Nor hold someone else's operations.
    let alice = c.alice;
    let foreign =
        c.s.op_on(id, "object.delete", Fields::new(), &alice)
            .unwrap();
    assert!(c.s.propose("sneak", &[foreign], &who).is_err());
}

#[test]
fn proposals_change_only_through_their_own_operations() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let who = agent_setup(&mut c);
    let args = fields([("fields", Value::Map(fields([("title", "y".into())])))]);
    let op = c.s.op_on(id, "object.set", args, &who).unwrap();
    let p = c.s.propose("x", &[op], &who).unwrap();
    let alice = c.alice;
    assert!(set(&mut c, p, &alice, "status", "approved".into()).is_err());
}

#[test]
fn an_automation_needs_no_invoker_only_if_it_is_named() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let (admin, bot) = (c.admin, c.bot);
    grant(&mut c.s, &admin, bot, "note.rename", "apply");
    let who = Principal::acting(bot);
    let op =
        c.s.op_on(id, "note.rename", fields([("title", "y".into())]), &who)
            .unwrap();
    assert!(matches!(c.s.execute(op, &who), Err(StoreError::Refused(_))));
}

#[test]
fn an_agent_cannot_send_a_message_without_approval() {
    let mut c = clan();
    let (admin, alice) = (c.admin, c.alice);
    let cron = Id::new();
    let op = c.s.create_op(
        cron,
        "locus.agent",
        fields([("title", "cron".into()), ("autonomous", true.into())]),
        None,
        admin.actor,
    );
    c.s.execute(op, &admin).unwrap();
    grant(&mut c.s, &admin, cron, "message.send", "apply");
    grant(&mut c.s, &admin, cron, "read", "apply");
    let m = Id::new();
    let op = c.s.create_op(
        m,
        "message",
        fields([("status", "draft".into())]),
        Some("<p>hi</p>"),
        admin.actor,
    );
    c.s.execute(op, &admin).unwrap();
    let who = Principal::acting(cron);
    let op =
        c.s.op_on(m, "message.send", fields([("status", "sent".into())]), &who)
            .unwrap();
    assert!(matches!(
        c.s.execute(op, &who).unwrap(),
        Outcome::Proposed { .. }
    ));
    assert_eq!(
        c.s.get(m).unwrap().unwrap().fields["status"],
        Value::text("draft")
    );
    let _ = alice;
}

#[test]
fn the_implementation_used_is_policy_checked_and_recorded() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let admin = c.admin;
    let cron = Id::new();
    let op = c.s.create_op(
        cron,
        "locus.agent",
        fields([("title", "cron".into()), ("autonomous", true.into())]),
        None,
        admin.actor,
    );
    c.s.execute(op, &admin).unwrap();
    let f = fields([
        ("subject", cron.into()),
        ("action", "note.rename".into()),
        ("models", Value::List(vec!["local".into()])),
    ]);
    let op =
        c.s.create_op(Id::new(), "locus.grant", f, None, admin.actor);
    c.s.execute(op, &admin).unwrap();
    let who = Principal::acting(cron);
    let mut op =
        c.s.op_on(id, "note.rename", fields([("title", "y".into())]), &who)
            .unwrap();
    op.implementation = Some(ImplUse {
        model: "cloud".into(),
        effort: "high".into(),
    });
    assert!(matches!(
        c.s.execute(op.clone(), &who),
        Err(StoreError::Refused(_))
    ));
    op.implementation = Some(ImplUse {
        model: "local".into(),
        effort: "high".into(),
    });
    op.id = Id::new();
    c.s.execute(op, &who).unwrap();
    let last = c.s.history(id).unwrap().pop().unwrap().op;
    assert_eq!(last.implementation.unwrap().model, "local");
}

#[test]
fn custom_types_are_objects_and_cannot_replace_builtins() {
    let mut c = clan();
    let admin = c.admin;
    let def = TypeDef {
        name: "recipe".into(),
        traits: vec!["titled".into()],
        fields: [(
            "title".to_string(),
            FieldDef {
                kind: Kind::Text,
                required: true,
                merge: Merge::KeepBoth,
            },
        )]
        .into(),
        ..TypeDef::default()
    };
    let f = fields([
        ("name", "recipe".into()),
        ("def", serde_json::to_string(&def).unwrap().into()),
    ]);
    let op = c.s.create_op(Id::new(), "locus.type", f, None, admin.actor);
    c.s.execute(op, &admin).unwrap();
    let op = c.s.create_op(
        Id::new(),
        "recipe",
        fields([("title", "pie".into())]),
        None,
        admin.actor,
    );
    c.s.execute(op, &admin).unwrap();
    let bad = TypeDef {
        name: "locus.grant".into(),
        ..TypeDef::default()
    };
    let f = fields([
        ("name", "x".into()),
        ("def", serde_json::to_string(&bad).unwrap().into()),
    ]);
    let op = c.s.create_op(Id::new(), "locus.type", f, None, admin.actor);
    assert!(c.s.execute(op, &admin).is_err());
}

#[test]
fn the_graph_is_an_index_of_references() {
    let mut c = clan();
    let alice = c.alice;
    let lauren = note(&mut c, "Lauren");
    let m = Id::new();
    let f = fields([
        ("title", "Prep".into()),
        ("attendees", Value::List(vec![lauren.into()])),
    ]);
    let op = c.s.create_op(m, "meeting", f, None, alice.actor);
    let mut alice_can = c.admin;
    alice_can.invoker = None;
    let op2 = Op {
        actor: alice_can.actor,
        ..op
    };
    c.s.execute(op2, &alice_can).unwrap();
    let n = Id::new();
    let body = format!(r#"<p id="b1"><a href="locus:{lauren}">L</a></p>"#);
    let op = c.s.create_op(
        n,
        "note",
        fields([("title", "n".into())]),
        Some(&body),
        alice.actor,
    );
    c.s.execute(op, &alice).unwrap();
    let mut got = c.s.refs_to(lauren).unwrap();
    got.sort();
    let mut want = vec![m, n];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(c.s.rebuild().unwrap(), 0);
    assert_eq!(c.s.refs_to(lauren).unwrap().len(), 2);
}

#[test]
fn saved_views_respect_permissions() {
    let mut c = clan();
    let (admin, alice, bob) = (c.admin, c.alice, c.bob);
    note(&mut c, "visible to alice");
    let spec = serde_json::to_string(&ViewSpec::of(Filter::Type("note".into()))).unwrap();
    let v = Id::new();
    let op = c.s.create_op(
        v,
        "locus.view",
        fields([("title", "all".into()), ("spec", spec.into())]),
        None,
        admin.actor,
    );
    c.s.execute(op, &admin).unwrap();
    assert_eq!(c.s.saved_view(&alice, v).unwrap().len(), 1);
    assert!(c.s.saved_view(&bob, v).is_err());
}

#[test]
fn a_reaction_proposes_by_default_and_can_apply_by_policy() {
    for auto in [false, true] {
        let mut c = clan();
        let (admin, alice) = (c.admin, c.alice);
        let cron = Id::new();
        let op = c.s.create_op(
            cron,
            "locus.agent",
            fields([("title", "cron".into()), ("autonomous", true.into())]),
            None,
            admin.actor,
        );
        c.s.execute(op, &admin).unwrap();
        grant(&mut c.s, &admin, cron, "read", "apply");
        grant(&mut c.s, &admin, cron, "object.add", "apply");
        let spec = ViewSpec::of(Filter::Contains("title".into(), "urgent".into()));
        let v = Id::new();
        let f = fields([
            ("title", "urgent".into()),
            ("spec", serde_json::to_string(&spec).unwrap().into()),
        ]);
        let op = c.s.create_op(v, "locus.view", f, None, admin.actor);
        c.s.execute(op, &admin).unwrap();
        let f = fields([
            ("view", v.into()),
            ("op", "object.add".into()),
            (
                "args",
                Value::Map(fields([
                    ("field", "tags".into()),
                    ("value", "flagged".into()),
                ])),
            ),
            ("agent", cron.into()),
            ("auto", auto.into()),
        ]);
        let op =
            c.s.create_op(Id::new(), "locus.reaction", f, None, admin.actor);
        c.s.execute(op, &admin).unwrap();
        let id = note(&mut c, "urgent thing");
        let tags = c.s.get(id).unwrap().unwrap().fields["tags"].clone();
        let flagged = tags == Value::List(vec!["flagged".into()]);
        assert_eq!(flagged, auto);
        assert_eq!(c.s.proposals(&admin).unwrap().len(), usize::from(!auto));
        // No new match, no new firing.
        set(&mut c, id, &alice, "extra", "x".into()).unwrap();
        assert_eq!(c.s.proposals(&admin).unwrap().len(), usize::from(!auto));
    }
}

#[test]
fn a_disabled_member_can_do_nothing() {
    let mut c = clan();
    let id = note(&mut c, "x");
    let (admin, alice) = (c.admin, c.alice);
    let op =
        c.s.op_on(
            alice.actor,
            "object.set",
            fields([("fields", Value::Map(fields([("disabled", true.into())])))]),
            &admin,
        )
        .unwrap();
    c.s.execute(op, &admin).unwrap();
    assert!(set(&mut c, id, &alice, "title", "y".into()).is_err());
}

#[test]
fn a_store_refuses_objects_homed_elsewhere() {
    let mut c = clan();
    let admin = c.admin;
    let args = fields([
        ("type", "note".into()),
        ("home", "other".into()),
        ("fields", Value::Map(Fields::new())),
    ]);
    let op = Op::new(Id::new(), "object.create", args, admin.actor);
    assert!(matches!(
        c.s.execute(op, &admin),
        Err(StoreError::Refused(_))
    ));
}
