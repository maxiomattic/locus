use crate::op::{SEEN_BODY, scrub_entry};
use crate::*;

fn ok(h: &str) -> Result<String, String> {
    Ok(h.to_string())
}

fn create(ty: &str, f: Fields) -> (Option<Object>, Registry) {
    let reg = Registry::builtin();
    let op = Op::new(
        Id::new(),
        "object.create",
        fields([
            ("type", ty.into()),
            ("home", "home".into()),
            ("fields", Value::Map(f)),
        ]),
        Id::SYSTEM,
    );
    let mut doc = None;
    let e = plan(None, &op, &reg, ok).unwrap();
    apply(&mut doc, &e, op.object, op.at);
    (doc, reg)
}

fn set(doc: &Object, actor: Id, field: &str, v: Value) -> Op {
    Op::new(
        doc.id,
        "object.set",
        fields([("fields", Value::Map(fields([(field, v)])))]),
        actor,
    )
    .seeing(Some(doc), &Registry::builtin())
}

fn run(doc: &mut Option<Object>, op: &Op, reg: &Registry) -> Vec<Effect> {
    let e = plan(doc.as_ref(), op, reg, ok).unwrap();
    apply(doc, &e, op.object, op.at);
    e
}

#[test]
fn a_plain_set_records_what_it_replaced() {
    let (mut doc, reg) = create("note", fields([("title", "a".into())]));
    let op = set(doc.as_ref().unwrap(), Id::SYSTEM, "title", "b".into());
    let e = run(&mut doc, &op, &reg);
    assert_eq!(
        e,
        vec![Effect::Set {
            field: "title".into(),
            old: Some("a".into()),
            new: Some("b".into())
        }]
    );
    assert_eq!(doc.unwrap().rev, 2);
}

#[test]
fn a_concurrent_set_keeps_both_values() {
    let (mut doc, reg) = create("note", fields([("title", "base".into())]));
    let (a, b) = (Id::new(), Id::new());
    let op_a = set(doc.as_ref().unwrap(), a, "title", "from a".into());
    let op_b = set(doc.as_ref().unwrap(), b, "title", "from b".into());
    run(&mut doc, &op_a, &reg);
    run(&mut doc, &op_b, &reg);
    let d = doc.unwrap();
    assert_eq!(d.title(), Some("from a"));
    assert_eq!(d.conflicts.len(), 1);
    assert_eq!(d.conflicts[0].value, Some("from b".into()));
}

#[test]
fn identical_concurrent_edits_do_not_conflict() {
    let (mut doc, reg) = create("note", fields([("title", "base".into())]));
    let op_a = set(doc.as_ref().unwrap(), Id::new(), "title", "same".into());
    let op_b = set(doc.as_ref().unwrap(), Id::new(), "title", "same".into());
    run(&mut doc, &op_a, &reg);
    assert!(run(&mut doc, &op_b, &reg).is_empty());
    assert!(doc.unwrap().conflicts.is_empty());
}

#[test]
fn a_second_edit_by_the_loser_amends_its_version() {
    let (mut doc, reg) = create("note", fields([("title", "base".into())]));
    let me = Id::new();
    let mine1 = set(doc.as_ref().unwrap(), me, "title", "mine 1".into());
    let mut mine2 = mine1.clone();
    mine2.id = Id::new();
    mine2.args = fields([("fields", Value::Map(fields([("title", "mine 2".into())])))]);
    mine2.seen.insert("title".into(), Some("mine 1".into()));
    let theirs = set(doc.as_ref().unwrap(), Id::new(), "title", "theirs".into());
    run(&mut doc, &theirs, &reg);
    run(&mut doc, &mine1, &reg);
    run(&mut doc, &mine2, &reg);
    let d = doc.unwrap();
    assert_eq!(d.conflicts.len(), 1);
    assert_eq!(d.conflicts[0].value, Some("mine 2".into()));
}

#[test]
fn sets_merge_without_conflict() {
    let (mut doc, reg) = create("note", fields([("title", "x".into())]));
    let d = doc.clone().unwrap();
    let add = |v: &str| {
        Op::new(
            d.id,
            "object.add",
            fields([("field", "tags".into()), ("value", v.into())]),
            Id::SYSTEM,
        )
    };
    run(&mut doc, &add("a"), &reg);
    run(&mut doc, &add("b"), &reg);
    let d = doc.unwrap();
    assert_eq!(d.fields["tags"], Value::List(vec!["a".into(), "b".into()]));
    assert!(d.conflicts.is_empty());
}

#[test]
fn delete_against_a_concurrent_edit_opens_a_conflict() {
    let (mut doc, reg) = create("note", fields([("title", "x".into())]));
    let del = Op::new(
        doc.as_ref().unwrap().id,
        "object.delete",
        Fields::new(),
        Id::SYSTEM,
    )
    .seeing(doc.as_ref(), &Registry::builtin());
    let edit = set(doc.as_ref().unwrap(), Id::new(), "title", "y".into());
    run(&mut doc, &edit, &reg);
    run(&mut doc, &del, &reg);
    let d = doc.as_ref().unwrap();
    assert!(d.deleted);
    assert_eq!(d.conflicts.len(), 1);
    assert_eq!(d.conflicts[0].field, None);
    let id = d.conflicts[0].id.clone();
    let fix = Op::new(
        d.id,
        "object.resolve",
        fields([("conflict", id.into()), ("take", true.into())]),
        Id::SYSTEM,
    );
    run(&mut doc, &fix, &reg);
    let d = doc.unwrap();
    assert!(!d.deleted);
    assert_eq!(d.title(), Some("y"));
}

#[test]
fn an_edit_of_a_deleted_object_is_kept() {
    let (mut doc, reg) = create("note", fields([("title", "x".into())]));
    let edit = set(doc.as_ref().unwrap(), Id::new(), "title", "y".into());
    let del = Op::new(
        doc.as_ref().unwrap().id,
        "object.delete",
        Fields::new(),
        Id::SYSTEM,
    )
    .seeing(doc.as_ref(), &Registry::builtin());
    run(&mut doc, &del, &reg);
    run(&mut doc, &edit, &reg);
    let d = doc.as_ref().unwrap();
    assert!(d.deleted);
    assert_eq!(d.conflicts[0].value, Some("y".into()));
    let fix = Op::new(
        d.id,
        "object.resolve",
        fields([
            ("conflict", d.conflicts[0].id.clone().into()),
            ("take", true.into()),
        ]),
        Id::SYSTEM,
    );
    run(&mut doc, &fix, &reg);
    let d = doc.unwrap();
    assert!(!d.deleted);
    assert_eq!(d.title(), Some("y"));
    assert!(d.conflicts.is_empty());
}

#[test]
fn bad_writes_are_refused() {
    let (doc, reg) = create("task", fields([("title", "x".into())]));
    let d = doc.as_ref().unwrap();
    let wrong_kind = set(d, Id::new(), "done", "yes".into());
    assert!(plan(doc.as_ref(), &wrong_kind, &reg, ok).is_err());
    let unknown = set(d, Id::new(), "nope", "x".into());
    assert!(plan(doc.as_ref(), &unknown, &reg, ok).is_err());
    let missing = Op::new(
        Id::new(),
        "object.create",
        fields([("type", "task".into()), ("home", "h".into())]),
        Id::SYSTEM,
    );
    assert!(plan(None, &missing, &reg, ok).is_err());
}

#[test]
fn declared_operations_only_touch_declared_fields() {
    let (mut doc, reg) = create("meeting", fields([("title", "m".into())]));
    let id = doc.as_ref().unwrap().id;
    let op = Op::new(
        id,
        "meeting.reschedule",
        fields([("starts_at", Value::Time(5))]),
        Id::SYSTEM,
    )
    .seeing(doc.as_ref(), &Registry::builtin());
    run(&mut doc, &op, &reg);
    assert_eq!(doc.as_ref().unwrap().fields["starts_at"], Value::Time(5));
    let sneaky = Op::new(
        id,
        "meeting.reschedule",
        fields([("title", "stolen".into())]),
        Id::SYSTEM,
    );
    assert!(plan(doc.as_ref(), &sneaky, &reg, ok).is_err());
}

#[test]
fn purge_scrubs_a_secret_from_every_entry() {
    let (mut doc, reg) = create(
        "note",
        fields([("title", "x".into()), ("secret", "hunter2".into())]),
    );
    let op = set(doc.as_ref().unwrap(), Id::new(), "secret", "hunter3".into());
    let mut effects = run(&mut doc, &op, &reg);
    let mut op = op;
    let secrets = op::field_values("secret", &effects);
    let scope = Purge::Field("secret".into());
    assert!(scrub_entry(&mut op, &mut effects, &scope, &secrets));
    let json = serde_json::to_string(&(&op, &effects)).unwrap();
    assert!(!json.contains("hunter"), "{json}");
}

#[test]
fn seen_covers_the_body() {
    assert_eq!(SEEN_BODY, "$body");
}

fn policy() -> (Policy, Id, Id, Id, Id, Registry) {
    let member = |admin: bool| {
        create(
            "locus.member",
            fields([("title", "m".into()), ("admin", admin.into())]),
        )
        .0
        .unwrap()
    };
    let (admin, alice) = (member(true), member(false));
    let bot = create(
        "locus.agent",
        fields([("title", "bot".into()), ("autonomous", false.into())]),
    )
    .0
    .unwrap();
    let auto = create(
        "locus.agent",
        fields([("title", "cron".into()), ("autonomous", true.into())]),
    )
    .0
    .unwrap();
    let grant = |who: Id, action: &str, mode: &str| {
        create(
            "locus.grant",
            fields([
                ("subject", who.into()),
                ("action", action.into()),
                ("mode", mode.into()),
            ]),
        )
        .0
        .unwrap()
    };
    let objs = vec![
        admin.clone(),
        alice.clone(),
        bot.clone(),
        auto.clone(),
        grant(alice.id, "note.*", "apply"),
        grant(alice.id, "object.*", "apply"),
        grant(bot.id, "note.rename", "apply"),
        grant(bot.id, "object.set", "propose"),
        grant(auto.id, "*", "apply"),
    ];
    (
        Policy::from_objects(&objs),
        admin.id,
        alice.id,
        bot.id,
        auto.id,
        Registry::builtin(),
    )
}

fn req<'a>(action: &'a str, ty: &'a str) -> Request<'a> {
    Request {
        action,
        type_name: ty,
        object: None,
        implementation: None,
    }
}

#[test]
fn an_agent_gets_the_intersection_with_its_invoker() {
    let (p, _, alice, bot, _, reg) = policy();
    let who = Principal::for_invoker(bot, alice);
    assert_eq!(
        p.decide(&who, &req("note.rename", "note"), &reg),
        Decision::Apply
    );
    assert_eq!(
        p.decide(&who, &req("object.set", "note"), &reg),
        Decision::Propose
    );
    let not_alice = req("object.delete", "note");
    assert!(matches!(
        p.decide(&who, &not_alice, &reg),
        Decision::Refuse(_)
    ));
}

#[test]
fn an_agent_with_no_invoker_must_be_autonomous() {
    let (p, _, _, bot, auto, reg) = policy();
    let r = req("note.rename", "note");
    assert!(matches!(
        p.decide(&Principal::acting(bot), &r, &reg),
        Decision::Refuse(_)
    ));
    assert_eq!(
        p.decide(&Principal::acting(auto), &r, &reg),
        Decision::Apply
    );
}

#[test]
fn agents_never_touch_policy_or_purge() {
    let (p, admin, _, _, auto, reg) = policy();
    let who = Principal::acting(auto);
    assert!(matches!(
        p.decide(&who, &req("object.set", "locus.grant"), &reg),
        Decision::Refuse(_)
    ));
    assert!(matches!(
        p.decide(&who, &req("object.purge", "note"), &reg),
        Decision::Refuse(_)
    ));
    assert!(matches!(
        p.decide(&who, &req("proposal.approve", "locus.proposal"), &reg),
        Decision::Refuse(_)
    ));
    assert_eq!(
        p.decide(
            &Principal::acting(admin),
            &req("object.purge", "note"),
            &reg
        ),
        Decision::Apply
    );
}

#[test]
fn wildcards_do_not_reach_purge() {
    let (p, _, alice, ..) = policy();
    let reg = Registry::builtin();
    let who = Principal::acting(alice);
    assert!(matches!(
        p.decide(&who, &req("object.purge", "note"), &reg),
        Decision::Refuse(_)
    ));
}

#[test]
fn sending_a_message_is_always_a_proposal_for_an_agent() {
    let (p, _, _, _, auto, reg) = policy();
    assert_eq!(
        p.decide(
            &Principal::acting(auto),
            &req("message.send", "message"),
            &reg
        ),
        Decision::Propose
    );
}

#[test]
fn a_person_cannot_claim_an_invoker() {
    let (p, admin, alice, ..) = policy();
    let reg = Registry::builtin();
    let who = Principal::for_invoker(alice, admin);
    assert!(matches!(
        p.decide(&who, &req("note.rename", "note"), &reg),
        Decision::Refuse(_)
    ));
}

#[test]
fn implementations_are_limited_by_model_and_effort() {
    let (admin_id, reg) = (Id::new(), Registry::builtin());
    let grant = create(
        "locus.grant",
        fields([
            ("subject", admin_id.into()),
            ("action", "note.summarize".into()),
            ("models", Value::List(vec!["local-llama".into()])),
            ("efforts", Value::List(vec!["low".into()])),
        ]),
    )
    .0
    .unwrap();
    let mut me = create("locus.member", fields([("title", "m".into())]))
        .0
        .unwrap();
    me.id = admin_id;
    let p = Policy::from_objects([&me, &grant]);
    let local = ImplUse {
        model: "local-llama".into(),
        effort: "low".into(),
    };
    let cloud = ImplUse {
        model: "cloud".into(),
        effort: "low".into(),
    };
    let who = Principal::acting(admin_id);
    let mut r = req("note.summarize", "note");
    r.implementation = Some(&local);
    assert_eq!(p.decide(&who, &r, &reg), Decision::Apply);
    r.implementation = Some(&cloud);
    assert!(matches!(p.decide(&who, &r, &reg), Decision::Refuse(_)));
}

#[test]
fn views_select_by_trait_and_reference() {
    let reg = Registry::builtin();
    let lauren = Id::new();
    let (m, _) = create(
        "meeting",
        fields([
            ("title", "m".into()),
            ("attendees", Value::List(vec![lauren.into()])),
        ]),
    );
    let (n, _) = create("note", fields([("title", "n".into())]));
    let objs = vec![m.unwrap(), n.unwrap()];
    let timed = evaluate(
        &ViewSpec::of(Filter::Trait("timed".into())),
        objs.clone(),
        &reg,
    );
    assert_eq!(timed.len(), 1);
    let linked = evaluate(&ViewSpec::of(Filter::Refers(lauren)), objs, &reg);
    assert_eq!(linked.len(), 1);
}

#[test]
fn body_links_are_found() {
    let id = Id::new();
    let html = format!(r#"<p id="b1">see <a href="locus:{id}">x</a></p>"#);
    assert_eq!(body_refs(&html), vec![id]);
}
