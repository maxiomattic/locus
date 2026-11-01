//! Reactions: a view whose result changed invokes an operation.
//!
//! A reaction runs as its named agent, with that agent's own grant. It
//! proposes by default. Policy can let one apply automatically, when both the
//! reaction says `auto` and the agent's grant allows applying.

use crate::{Result, Store, StoreError};
use locus_model::{Fields, Object, ObjectId, Op, Principal, Value, ViewSpec};
use rusqlite::{OptionalExtension, params};
use std::collections::BTreeSet;

impl Store {
    pub(crate) fn run_reactions(&mut self) -> Result<()> {
        let reactions = self.objects_of_type("locus.reaction")?;
        if reactions.is_empty() {
            return Ok(());
        }
        self.in_reaction = true;
        let result = reactions
            .iter()
            .filter(|r| !r.deleted)
            .try_for_each(|r| self.run_reaction(r));
        self.in_reaction = false;
        result
    }

    fn run_reaction(&mut self, r: &Object) -> Result<()> {
        let bad = |what: &str| StoreError::Invalid(format!("reaction {}: {what}", r.id));
        let view = r
            .fields
            .get("view")
            .and_then(Value::as_ref_id)
            .ok_or_else(|| bad("no view"))?;
        let agent = r
            .fields
            .get("agent")
            .and_then(Value::as_ref_id)
            .ok_or_else(|| bad("no agent"))?;
        let name = r
            .fields
            .get("op")
            .and_then(Value::as_text)
            .ok_or_else(|| bad("no op"))?
            .to_string();
        let args = match r.fields.get("args") {
            Some(Value::Map(m)) => m.clone(),
            _ => Fields::new(),
        };
        let auto = r
            .fields
            .get("auto")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let who = Principal::acting(agent);
        let Some(v) = self.get(view)? else {
            return Ok(());
        };
        let spec = v
            .fields
            .get("spec")
            .and_then(Value::as_text)
            .ok_or_else(|| bad("no spec"))?;
        let spec: ViewSpec = serde_json::from_str(spec)?;
        // The reaction sees what its agent may read, and no more.
        let now: BTreeSet<ObjectId> = self.view(&who, &spec)?.iter().map(|o| o.id).collect();
        let key = r.id.to_string();
        let old: Option<String> = self
            .conn()
            .query_row(
                "SELECT ids FROM reaction_state WHERE reaction=?1",
                [&key],
                |x| x.get(0),
            )
            .optional()?;
        let json = serde_json::to_string(&now)?;
        self.conn().execute(
            "INSERT OR REPLACE INTO reaction_state (reaction, ids) VALUES (?1, ?2)",
            params![key, json],
        )?;
        let Some(old) = old else {
            return Ok(()); // The first look sets the baseline.
        };
        let old: BTreeSet<ObjectId> = serde_json::from_str(&old)?;
        for id in now.difference(&old) {
            let doc = self.get(*id)?;
            let op = Op::new(*id, &name, args.clone(), agent)
                .seeing(doc.as_ref(), self.registry())
                .intent(&format!("reaction {key}"));
            let result = if auto {
                self.execute(op, &who).map(|_| ())
            } else {
                self.propose(&format!("reaction {key} on {id}"), &[op], &who)
                    .map(|_| ())
            };
            // A refusal is audited by execute, and must not undo the change
            // that triggered the reaction.
            if let Err(e) = result
                && !matches!(e, StoreError::Refused(_) | StoreError::Plan(_))
            {
                return Err(e);
            }
        }
        Ok(())
    }
}
