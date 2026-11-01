//! `loc`: the new Locus core from a shell. Every state change is an operation.
//!
//! A directory holds one machine's store. `loc --dir D init` makes it a home
//! server. `loc --dir C join ...` makes it a client of one. Commands then work
//! the same either way: on a client they apply at once and stay provisional
//! until `loc sync`.

use locus_model::proposal::render;
use locus_model::*;
use locus_store::{Store, StoreError};
use locus_sync::{HomeServer, LocalTransport, Network, Replica, Subscription};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: loc --dir DIR [--as ID] [--for ID] [--intent TEXT] COMMAND

setup:
  init HOME ADMIN-NAME                 make DIR a home server; prints the admin ID
  join SERVER-DIR HOME NAME            make DIR a client of a server on this machine
  whoami
members and permissions:
  member NAME [--admin]                add a member
  agent NAME [--autonomous]            add an agent
  grant SUBJECT ACTION [--type T] [--object ID] [--mode apply|propose]
        [--models a,b] [--efforts a,b]
objects:
  create TYPE [k=v ...] [--body HTML]  values: 12, true, @ID (reference),
                                       t:MS (time), blob:HASH, text
  get ID  |  list [TYPE]  |  refs ID  |  run ID OP [k=v ...]
  set ID k=v ...  |  unset ID FIELD  |  add ID FIELD VALUE  |  remove ID FIELD VALUE
  body ID HTML  |  delete ID  |  restore ID
recovery:
  history ID  |  log  |  audit  |  deleted  |  revert OP-ID ID  |  undo-batch BATCH
  purge ID [--field F | --blob HASH]   irreversible
blobs:
  blob-put FILE  |  blob-get HASH OUT
conflicts:
  conflicts  |  resolve ID CONFLICT-ID [--take]
proposals:
  propose ID TITLE k=v ...  |  proposals  |  review ID  |  approve ID [--only 0,2]
  reject ID [REASON]
views and reactions:
  view TITLE FILTER-JSON  |  show-view ID
  react VIEW-ID OP AGENT-ID [--auto] [k=v ...]
sync (client):
  sync  |  pending  |  refused";

#[derive(Serialize, Deserialize)]
struct ClientConfig {
    server_dir: Option<PathBuf>,
    home: String,
    name: String,
}

#[derive(Default)]
struct Global {
    dir: Option<PathBuf>,
    actor: Option<Id>,
    invoker: Option<Id>,
    intent: String,
}

/// Flags pulled out of a command's words.
struct Words {
    list: Vec<String>,
}

impl Words {
    fn flag(&mut self, name: &str) -> bool {
        match self.list.iter().position(|w| w == name) {
            Some(i) => {
                self.list.remove(i);
                true
            }
            None => false,
        }
    }

    fn option(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(i) = self.list.iter().position(|w| w == name) else {
            return Ok(None);
        };
        if i + 1 >= self.list.len() {
            return Err(format!("{name} needs a value"));
        }
        let v = self.list.remove(i + 1);
        self.list.remove(i);
        Ok(Some(v))
    }

    fn next(&mut self, what: &str) -> Result<String, String> {
        if self.list.is_empty() {
            return Err(format!("missing {what}\n\n{USAGE}"));
        }
        Ok(self.list.remove(0))
    }

    fn id(&mut self, what: &str) -> Result<Id, String> {
        self.next(what)?.parse().map_err(|e| format!("{what}: {e}"))
    }

    fn pairs(&mut self) -> Result<Fields, String> {
        std::mem::take(&mut self.list)
            .iter()
            .map(|w| {
                let (k, v) = w.split_once('=').ok_or(format!("{w} is not key=value"))?;
                Ok((k.to_string(), parse_value(v)?))
            })
            .collect()
    }
}

fn parse_value(s: &str) -> Result<Value, String> {
    if let Some(id) = s.strip_prefix('@') {
        return Ok(Value::Ref(Reference::to(
            id.parse().map_err(|e| format!("{e}"))?,
        )));
    }
    if let Some(ms) = s.strip_prefix("t:") {
        return Ok(Value::Time(ms.parse().map_err(|_| "bad time".to_string())?));
    }
    if let Some(h) = s.strip_prefix("blob:") {
        return Ok(Value::Blob(h.parse().map_err(|e| format!("{e}"))?));
    }
    Ok(match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => s
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or_else(|_| Value::text(s)),
    })
}

enum Machine {
    Home(Box<Store>),
    Client(Box<Replica>),
}

struct Ctx {
    machine: Machine,
    who: Principal,
    intent: String,
    home: String,
}

fn meta(store: &Store, key: &str) -> Result<Option<String>, String> {
    use locus_store::rusqlite::OptionalExtension;
    store
        .conn()
        .query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())
}

fn set_meta(store: &Store, key: &str, value: &str) -> Result<(), String> {
    store
        .conn()
        .execute("INSERT OR REPLACE INTO meta VALUES (?1, ?2)", [key, value])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn open(g: &Global) -> Result<Ctx, String> {
    let dir = g.dir.as_ref().ok_or("--dir is required")?;
    if !dir.join("locus.db").exists() {
        return Err(format!(
            "{} is not a Locus store: run init or join",
            dir.display()
        ));
    }
    let store = Store::open(dir, "unused").map_err(err)?;
    let default_actor = meta(&store, "cli_actor")?
        .map(|s| s.parse::<Id>().map_err(err))
        .transpose()?;
    let actor = g
        .actor
        .or(default_actor)
        .ok_or("say who you are with --as")?;
    let who = Principal {
        actor,
        invoker: g.invoker,
    };
    let home = store.home().0.clone();
    let config = dir.join("client.json");
    if config.exists() {
        let cfg: ClientConfig =
            serde_json::from_slice(&std::fs::read(&config).map_err(err)?).map_err(err)?;
        let mut replica = Replica::new(store, who, &cfg.name);
        if let Some(server_dir) = &cfg.server_dir {
            // A server on this machine, reached in-process. The transport
            // stands in for `locus-net`, and only opens the store if used.
            let server = Store::open(server_dir, "unused").map_err(err)?;
            replica.connect(
                &cfg.home,
                Box::new(LocalTransport::new(
                    HomeServer::new(server),
                    who,
                    Network::default(),
                )),
                Subscription::everything(),
            );
        }
        return Ok(Ctx {
            machine: Machine::Client(Box::new(replica)),
            who,
            intent: g.intent.clone(),
            home: cfg.home,
        });
    }
    Ok(Ctx {
        machine: Machine::Home(Box::new(store)),
        who,
        intent: g.intent.clone(),
        home,
    })
}

impl Ctx {
    fn store(&self) -> &Store {
        match &self.machine {
            Machine::Home(s) => s,
            Machine::Client(r) => r.store(),
        }
    }

    fn get(&self, id: Id) -> Result<Object, String> {
        match &self.machine {
            Machine::Home(s) => s.get_as(&self.who, id).map_err(err),
            Machine::Client(r) => r
                .get(id)
                .map_err(err)?
                .map(|v| v.object)
                .ok_or(format!("{id} is not held here")),
        }
    }

    fn create(&mut self, ty: &str, f: Fields, body: Option<&str>) -> Result<Id, String> {
        let (who, intent) = (self.who, self.intent.clone());
        match &mut self.machine {
            Machine::Home(s) => {
                let id = Id::new();
                let mut op = s.create_op(id, ty, f, body, who.actor).intent(&intent);
                op.invoker = who.invoker;
                s.execute(op, &who).map_err(err)?;
                Ok(id)
            }
            Machine::Client(r) => {
                let home = self.home.clone();
                r.create(&home, ty, f, body, &intent).map_err(err)
            }
        }
    }

    fn act(&mut self, id: Id, name: &str, args: Fields) -> Result<(), String> {
        let (who, intent) = (self.who, self.intent.clone());
        match &mut self.machine {
            Machine::Home(s) => {
                let op = s.op_on(id, name, args, &who).map_err(err)?.intent(&intent);
                match s.execute(op, &who).map_err(err)? {
                    locus_store::Outcome::Proposed { proposal, .. } => {
                        println!("proposed {proposal}: you may only propose this");
                    }
                    locus_store::Outcome::Applied { effects, .. } => {
                        for e in effects {
                            if let Effect::ConflictOpened(c) = e {
                                println!("conflict {}: both versions kept", c.id);
                            }
                        }
                    }
                    locus_store::Outcome::Duplicate { .. } => {}
                }
                Ok(())
            }
            Machine::Client(r) => r.act(id, name, args, &intent).map(|_| ()).map_err(err),
        }
    }

    fn review(&self, id: Id) -> Result<String, String> {
        let entries = match &self.machine {
            Machine::Home(s) => s.review(id, &self.who).map_err(err)?,
            Machine::Client(r) => r.review(id).map_err(err)?,
        };
        Ok(render(&entries))
    }

    fn list(&self, spec: &ViewSpec) -> Result<Vec<Object>, String> {
        match &self.machine {
            Machine::Home(s) => s.view(&self.who, spec).map_err(err),
            Machine::Client(r) => r.view(spec).map_err(err),
        }
    }

    fn propose(&mut self, target: Id, title: &str, f: Fields) -> Result<Id, String> {
        let who = self.who;
        let intent = self.intent.clone();
        let args = fields([("fields", Value::Map(f))]);
        match &mut self.machine {
            Machine::Home(s) => {
                let op = s
                    .op_on(target, "object.set", args, &who)
                    .map_err(err)?
                    .intent(&intent);
                s.propose(title, &[op], &who).map_err(err)
            }
            Machine::Client(r) => {
                let op = r.draft(target, "object.set", args, &intent).map_err(err)?;
                let home = self.home.clone();
                r.propose(&home, title, &[op]).map_err(err)
            }
        }
    }
}

fn line(o: &Object) -> String {
    let mut s = format!("{}  {}", o.id, o.type_name);
    if let Some(t) = o.title() {
        s += &format!("  {t:?}");
    }
    if o.deleted {
        s += "  [deleted]";
    }
    if !o.conflicts.is_empty() {
        s += &format!("  [{} conflict(s)]", o.conflicts.len());
    }
    s
}

fn pretty<T: Serialize>(v: &T) -> Result<String, String> {
    serde_json::to_string_pretty(v).map_err(err)
}

fn csv(s: Option<String>) -> Option<Value> {
    s.map(|s| Value::List(s.split(',').map(Value::text).collect()))
}

fn run(g: Global, cmd: &str, mut w: Words) -> Result<(), String> {
    let dir = g.dir.clone().ok_or("--dir is required")?;
    match cmd {
        "init" => {
            let home = w.next("home name")?;
            let name = w.next("admin name")?;
            let mut s = Store::open(&dir, &home).map_err(err)?;
            let id = s.genesis(&name).map_err(err)?;
            set_meta(&s, "cli_actor", &id.to_string())?;
            println!("{id}");
            return Ok(());
        }
        "join" => {
            let server = PathBuf::from(w.next("server dir")?);
            let home = w.next("home name")?;
            let name = w.next("client name")?;
            let s = Store::open(&dir, &name).map_err(err)?;
            if let Some(a) = g.actor {
                set_meta(&s, "cli_actor", &a.to_string())?;
            }
            let cfg = ClientConfig {
                server_dir: Some(std::path::absolute(server).map_err(err)?),
                home,
                name,
            };
            std::fs::write(
                dir.join("client.json"),
                serde_json::to_vec(&cfg).map_err(err)?,
            )
            .map_err(err)?;
            return Ok(());
        }
        _ => {}
    }
    let mut c = open(&g)?;
    match cmd {
        "whoami" => println!("{}", c.who.actor),
        "member" | "agent" => {
            let flag = w.flag(if cmd == "member" {
                "--admin"
            } else {
                "--autonomous"
            });
            let key = if cmd == "member" {
                "admin"
            } else {
                "autonomous"
            };
            let name = w.next("name")?;
            let f = fields([("title", name.as_str().into()), (key, flag.into())]);
            let ty = format!("locus.{cmd}");
            println!("{}", c.create(&ty, f, None)?);
        }
        "grant" => {
            let ty = w.option("--type")?;
            let object = w.option("--object")?;
            let mode = w.option("--mode")?.unwrap_or_else(|| "apply".to_string());
            let models = w.option("--models")?;
            let efforts = w.option("--efforts")?;
            let subject = w.id("subject")?;
            let action = w.next("action")?;
            let mut f = fields([
                ("subject", subject.into()),
                ("action", action.as_str().into()),
                ("mode", mode.as_str().into()),
            ]);
            if let Some(t) = ty {
                f.insert("scope_type".into(), t.into());
            }
            if let Some(o) = object {
                f.insert("scope_object".into(), parse_value(&format!("@{o}"))?);
            }
            if let Some(v) = csv(models) {
                f.insert("models".into(), v);
            }
            if let Some(v) = csv(efforts) {
                f.insert("efforts".into(), v);
            }
            println!("{}", c.create("locus.grant", f, None)?);
        }
        "create" => {
            let body = w.option("--body")?;
            let ty = w.next("type")?;
            let f = w.pairs()?;
            println!("{}", c.create(&ty, f, body.as_deref())?);
        }
        "get" => println!("{}", pretty(&c.get(w.id("id")?)?)?),
        "list" => {
            let filter = match w.list.first() {
                Some(t) => Filter::Type(t.clone()),
                None => Filter::All,
            };
            for o in c.list(&ViewSpec::of(filter))? {
                println!("{}", line(&o));
            }
        }
        "refs" => {
            let id = w.id("id")?;
            for o in c.list(&ViewSpec::of(Filter::Refers(id)))? {
                println!("{}", line(&o));
            }
        }
        "run" => {
            let id = w.id("id")?;
            let op = w.next("operation")?;
            let args = w.pairs()?;
            c.act(id, &op, args)?;
        }
        "set" => {
            let id = w.id("id")?;
            let f = w.pairs()?;
            c.act(id, "object.set", fields([("fields", Value::Map(f))]))?;
        }
        "unset" => {
            let id = w.id("id")?;
            let f = w.next("field")?;
            let list = Value::List(vec![f.as_str().into()]);
            c.act(id, "object.set", fields([("unset", list)]))?;
        }
        "add" | "remove" => {
            let id = w.id("id")?;
            let f = w.next("field")?;
            let v = parse_value(&w.next("value")?)?;
            let args = fields([("field", f.as_str().into()), ("value", v)]);
            c.act(id, &format!("object.{cmd}"), args)?;
        }
        "body" => {
            let id = w.id("id")?;
            let h = w.next("html")?;
            c.act(id, "body.set", fields([("html", h.as_str().into())]))?;
        }
        "delete" | "restore" => c.act(w.id("id")?, &format!("object.{cmd}"), Fields::new())?,
        "history" => {
            let id = w.id("id")?;
            c.get(id)?;
            let entries = match &c.machine {
                Machine::Home(s) => s.history(id).map_err(err)?,
                Machine::Client(r) => r.history(id).map_err(err)?,
            };
            for e in entries {
                println!(
                    "{} rev {} {} by {} {:?}: {}",
                    e.op.id,
                    e.rev,
                    e.op.name,
                    e.op.actor,
                    e.op.intent,
                    serde_json::to_string(&e.effects).map_err(err)?
                );
            }
        }
        "log" => {
            for e in c.store().log_since(0, 10_000).map_err(err)? {
                println!("{:>6} {} {} {}", e.seq, e.op.id, e.op.name, e.op.object);
            }
        }
        "audit" => {
            for a in c.store().audit(10_000).map_err(err)?.iter().rev() {
                println!(
                    "{} {} {} {} {} {}",
                    a.at,
                    a.decision,
                    a.action,
                    a.actor,
                    a.object.as_deref().unwrap_or("-"),
                    a.detail
                );
            }
        }
        "deleted" => {
            let spec = ViewSpec::of(Filter::And(vec![Filter::IncludeDeleted]));
            for o in c.list(&spec)?.iter().filter(|o| o.deleted) {
                println!("{}", line(o));
            }
        }
        "revert" => {
            let op = w.id("operation")?;
            let id = w.id("object")?;
            let (who, intent) = (c.who, c.intent.clone());
            match &mut c.machine {
                Machine::Home(s) => drop(s.revert(op, &who, &intent).map_err(err)?),
                Machine::Client(r) => r.revert(op, id, &intent).map_err(err)?,
            }
        }
        "undo-batch" => {
            let b = w.id("batch")?;
            let (who, intent) = (c.who, c.intent.clone());
            let Machine::Home(s) = &mut c.machine else {
                return Err("undo-batch runs on the home server".to_string());
            };
            println!(
                "{} entries undone",
                s.revert_batch(b, &who, &intent).map_err(err)?
            );
        }
        "purge" => {
            let field = w.option("--field")?;
            let blob = w.option("--blob")?;
            let id = w.id("id")?;
            let args = match (field, blob) {
                (Some(f), _) => fields([("scope", "field".into()), ("field", f.into())]),
                (_, Some(b)) => fields([
                    ("scope", "blob".into()),
                    ("blob", Value::Blob(b.parse().map_err(err)?)),
                ]),
                _ => fields([("scope", "object".into())]),
            };
            c.act(id, "object.purge", args)?;
        }
        "blob-put" => {
            let path = w.next("file")?;
            let bytes = std::fs::read(&path).map_err(err)?;
            let home = c.home.clone();
            let who = c.who;
            let h = match &mut c.machine {
                Machine::Home(s) => s.put_blob_as(&who, &bytes).map_err(err)?,
                Machine::Client(r) => r.put_blob(&home, &bytes).map_err(err)?,
            };
            println!("{h}");
        }
        "blob-get" => {
            let h: BlobHash = w.next("hash")?.parse().map_err(err)?;
            let out = w.next("output file")?;
            let home = c.home.clone();
            let who = c.who;
            let bytes = match &mut c.machine {
                Machine::Home(s) => {
                    if !s.may_read_blob(&who, &h).map_err(err)? {
                        return Err(StoreError::Refused("no read grant".to_string()).to_string());
                    }
                    s.get_blob(&h).map_err(err)?
                }
                Machine::Client(r) => r.blob(&home, &h).map_err(err)?,
            };
            std::fs::write(Path::new(&out), bytes).map_err(err)?;
        }
        "conflicts" => {
            let spec = ViewSpec::of(Filter::And(vec![Filter::IncludeDeleted]));
            for o in c.list(&spec)? {
                for x in &o.conflicts {
                    let field = x.field.as_deref().unwrap_or("(delete)");
                    let current = x.field.as_ref().and_then(|f| o.fields.get(f));
                    println!(
                        "{} {} {field}: current {current:?}, other {:?} by {}, conflict {}",
                        o.id, o.type_name, x.value, x.actor, x.id
                    );
                }
            }
        }
        "resolve" => {
            let take = w.flag("--take");
            let id = w.id("id")?;
            let conflict = w.next("conflict id")?;
            let args = fields([
                ("conflict", conflict.as_str().into()),
                ("take", take.into()),
            ]);
            c.act(id, "object.resolve", args)?;
        }
        "propose" => {
            let target = w.id("object")?;
            let title = w.next("title")?;
            let f = w.pairs()?;
            println!("{}", c.propose(target, &title, f)?);
        }
        "proposals" => {
            let spec = ViewSpec::of(Filter::And(vec![
                Filter::Type("locus.proposal".to_string()),
                Filter::Eq("status".to_string(), "open".into()),
            ]));
            for o in c.list(&spec)? {
                println!("{}", line(&o));
            }
        }
        "review" => print!("{}", c.review(w.id("proposal")?)?),
        "approve" => {
            let only = w.option("--only")?;
            let id = w.id("proposal")?;
            let mut args = Fields::new();
            if let Some(o) = only {
                let list: Result<Vec<Value>, String> = o
                    .split(',')
                    .map(|n| n.parse::<i64>().map(Value::Int).map_err(err))
                    .collect();
                args.insert("only".to_string(), Value::List(list?));
            }
            c.act(id, "proposal.approve", args)?;
        }
        "reject" => {
            let id = w.id("proposal")?;
            let mut args = Fields::new();
            if let Ok(r) = w.next("reason") {
                args.insert("reason".to_string(), r.into());
            }
            c.act(id, "proposal.reject", args)?;
        }
        "view" => {
            let title = w.next("title")?;
            let filter: Filter = serde_json::from_str(&w.next("filter")?).map_err(err)?;
            let spec = serde_json::to_string(&ViewSpec::of(filter)).map_err(err)?;
            let f = fields([("title", title.as_str().into()), ("spec", spec.into())]);
            println!("{}", c.create("locus.view", f, None)?);
        }
        "show-view" => {
            let v = c.get(w.id("view")?)?;
            let spec = v
                .fields
                .get("spec")
                .and_then(Value::as_text)
                .unwrap_or("{}");
            let spec: ViewSpec = serde_json::from_str(spec).map_err(err)?;
            for o in c.list(&spec)? {
                println!("{}", line(&o));
            }
        }
        "react" => {
            let auto = w.flag("--auto");
            let view = w.id("view")?;
            let op = w.next("operation")?;
            let agent = w.id("agent")?;
            let args = w.pairs()?;
            let f = fields([
                ("view", view.into()),
                ("op", op.as_str().into()),
                ("agent", agent.into()),
                ("auto", auto.into()),
                ("args", Value::Map(args)),
            ]);
            println!("{}", c.create("locus.reaction", f, None)?);
        }
        "sync" => {
            let Machine::Client(r) = &mut c.machine else {
                return Err("sync is for clients".to_string());
            };
            let report = r.sync().map_err(err)?;
            println!(
                "sent {}, accepted {}, refused {}, proposed {}, received {}",
                report.submitted,
                report.accepted,
                report.refused,
                report.proposed.len(),
                report.received
            );
        }
        "pending" | "refused" => {
            let Machine::Client(r) = &c.machine else {
                return Err(format!("{cmd} is for clients"));
            };
            if cmd == "pending" {
                for o in r.provisional().map_err(err)? {
                    println!("{} {} {}", o.id, o.name, o.object);
                }
            } else {
                for x in r.refused().map_err(err)? {
                    println!("{} {} {}: {}", x.op.id, x.op.name, x.op.object, x.reason);
                }
            }
        }
        other => return Err(format!("unknown command {other}\n\n{USAGE}")),
    }
    Ok(())
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut g = Global::default();
    let mut take = |name: &str| -> Option<String> {
        let i = args.iter().position(|w| w == name)?;
        if i + 1 >= args.len() {
            return None;
        }
        let v = args.remove(i + 1);
        args.remove(i);
        Some(v)
    };
    g.dir = take("--dir").map(PathBuf::from);
    let parse = |s: Option<String>| -> Option<Id> {
        s.map(|s| {
            s.parse().unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(2);
            })
        })
    };
    g.actor = parse(take("--as"));
    g.invoker = parse(take("--for"));
    g.intent = take("--intent").unwrap_or_default();
    if args.is_empty() || args[0] == "help" {
        println!("{USAGE}");
        return;
    }
    let cmd = args.remove(0);
    if let Err(e) = run(g, &cmd, Words { list: args }) {
        eprintln!("loc: {e}");
        std::process::exit(1);
    }
}
