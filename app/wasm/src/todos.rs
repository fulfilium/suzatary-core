//! The todo readings a page takes beside the class's generic ones: the folds, and the list as
//! the box's route answers it, here at this application's record.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::fold;
use prodrome_core::fpl::{Candidates, Env, instant_of, iso};
use prodrome_core::policy::Policy;
use prodrome_core::registers;
use prodrome_wasm_exports::Read;
use prodrome_wasm_exports::json;
use prodrome_wasm_exports::wire::{
    json_candidates, json_env, json_marker, json_reading, json_tended, json_terms, object,
    parse_moment, parse_untrusted,
};
use serde_json::{Map, Value, json};
use wasm_bindgen::prelude::*;

use suzatary_base::chains::ChainName;
use suzatary_core::chain_set::Held;
use suzatary_core::routes;

use crate::unrendered::Unrendered;
use crate::{Chains, Event, Todos, printed, refused};

#[wasm_bindgen]
impl Todos {
    /// §6.1 to §6.5 at `at` (ISO, or `null` for the latest instant the objects stamp) under
    /// `untrusted`: the environment, the specs, each todo's function and what a
    /// `ref` links against, the state history a past knot is priced against, and each todo's
    /// events in causal order.
    pub fn fold(&self, at: Option<String>, untrusted: &str) -> Result<String, JsError> {
        let read = self.0.read().map_err(refused)?;
        let policy = parse_untrusted(untrusted).map_err(refused)?;
        let moment = read
            .moment(parse_moment("at", at).map_err(refused)?)
            .map_err(refused)?;
        let state = registers::fold(read.nodes);
        let now = instant_of(moment);
        let mut env = Env::new();
        let (mut specs, mut flat) = (BTreeMap::new(), BTreeMap::new());
        for prodrome in state.prodromes().values() {
            let one = fold::env(prodrome, now, &policy);
            env.outcomes.extend(one.outcomes);
            env.tended.extend(one.tended);
            specs.extend(fold::specs(prodrome, now, &policy));
            flat.extend(fold::flatten(prodrome, now, &policy).map_err(|e| refused(e.0))?);
        }
        let functions = Value::Object(
            fold::link_specs(&flat, state.entities().map(|(todo, _)| todo))
                .into_iter()
                .map(|(todo, term)| (todo, json::to_json(&term)))
                .collect(),
        );

        // Each todo's state register read at every instant a binding write is dated, where the
        // reading changes, and every binding tending.
        let mut bindings = Map::new();
        let mut tended = BTreeMap::new();
        for (todo, stream) in state.entities() {
            let mut instants = BTreeSet::new();
            for stamp in stream
                .iter()
                .filter(|s| policy.standing(&s.name, &*s.event).binds())
            {
                for write in fold::Write::of(&stamp.event) {
                    match write {
                        fold::Write::State(_) => {
                            instants.insert(instant_of(stamp.event.at()));
                        }
                        fold::Write::Tend(at) => {
                            tended
                                .entry(todo.as_str().to_owned())
                                .or_insert_with(BTreeSet::new)
                                .insert(at);
                        }
                        _ => {}
                    }
                }
            }
            let mut timeline = Vec::new();
            let mut before = Candidates::from([None]);
            for at in instants {
                let reading = fold::read(stream, Some(at), &policy).outcomes();
                if reading != before {
                    timeline.push(json!({ "at": iso(at), "binding": json_reading(&reading) }));
                    before = reading;
                }
            }
            if !timeline.is_empty() {
                bindings.insert(todo.as_str().to_owned(), Value::Array(timeline));
            }
        }
        let history = json!({ "bindings": bindings, "tended": json_tended(&tended) });
        let stream = Value::Object(
            streams(&read)
                .iter()
                .map(|(todo, events)| {
                    let markers = events
                        .iter()
                        .map(|(name, event)| json_marker(name, event))
                        .collect();
                    ((*todo).to_owned(), Value::Array(markers))
                })
                .collect(),
        );

        printed(&object(vec![
            ("at", Value::String(iso(now))),
            ("env", json_env(&env)),
            ("specs", json_candidates(&specs, json::to_json)),
            ("flatten", json_terms(&flat)),
            ("functions", functions),
            ("history", history),
            ("stream", stream),
        ]))
    }

    /// The box's `GET …/fold` answered from these objects: every todo as the folds see it at
    /// `at` under `untrusted`, priced through the set of these objects as the chain `chain` and
    /// every other chain `others` holds, each row composed by `view::entry` as the box composes
    /// it, its fields' keys beside no HTML.
    pub fn list(
        &self,
        at: Option<String>,
        untrusted: &str,
        chain: &str,
        others: &Chains,
    ) -> Result<String, JsError> {
        let read = self.0.read().map_err(refused)?;
        let policy = parse_untrusted(untrusted).map_err(refused)?;
        let moment = read
            .moment(parse_moment("at", at).map_err(refused)?)
            .map_err(refused)?;
        let name = ChainName::new(chain).map_err(|e| refused(e.to_string()))?;
        let own = Held {
            name: &name,
            dag: read.dag,
            nodes: read.nodes,
            untrusted: &policy,
        };
        let set = others.set(moment, Some(own));
        let fold = routes::fold(&Unrendered, &name, &policy, &set, None)
            .map_err(|e| refused(e.to_string()))?;
        crate::json(&fold)
    }
}

/// Per todo, its events in causal order with the object carrying each.
fn streams<'r>(read: &Read<'r, Event>) -> BTreeMap<&'r str, Vec<(&'r Hash, &'r Event)>> {
    let mut out: BTreeMap<&str, Vec<(&Hash, &Event)>> = BTreeMap::new();
    for node in read.nodes {
        if let Some(event) = node.event() {
            out.entry(event.todo().as_str())
                .or_default()
                .push((node.name(), event));
        }
    }
    out
}
