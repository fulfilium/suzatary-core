//! A todo chain as the tab's worker holds it: a store of objects at the todo schema, under the
//! policy the box serves it with, and the readings of it a page asks for.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_data::event::{Hash, parse_envelope};
use prodrome_data::instant::instant_of;
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::objects::Object;
use suzatary_store::replica::writes::{Answer, Refusal};
use wasm_bindgen::prelude::*;

use crate::line_rows::hashes;
use crate::{Event, Todos, json, refused};

crate::object_rows::objects_store!(Chain = Event, chain);

/// Where a todo's act the box has not taken stands. Of two acts on one todo the one further from
/// going through is what the todo says, so the order is the join's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
enum Unsent {
    Sending,
    AwaitingTrust,
    Refused,
}

#[wasm_bindgen]
impl Chain {
    /// The reading of what stands, the read objects of the landed row and the tab's writes.
    pub fn todos(&self) -> Result<Todos, JsError> {
        Todos::new(&self.0.standing().map_err(refused)?)
    }

    /// `{todos, acts, refused, blocked, stranded, losses, waiting}`: per todo where its unsent
    /// acts stand; each unsent act's todo, object name and instant (naive UTC to the microsecond); the box's sentences for what it refused for good; how many writes those refusals
    /// hold back; how many rest in a prodrome the box no longer serves; every write a reset would
    /// lose, by name; and how many wait for this device to be trusted. A signature is counted
    /// with what it signs, never beside it.
    pub fn unsent_reading(&self) -> Result<String, JsError> {
        let replica = self.0.replica();
        let untaken: Vec<&Object> = replica
            .untaken()
            .into_iter()
            .map(|own| &own.node)
            .filter(|o| !o.is_signature())
            .collect();
        let held_back: BTreeSet<&Hash> =
            replica.refusals().iter().map(|(o, ..)| o.name()).collect();
        let trusting = resting_on(
            &untaken,
            replica
                .untaken()
                .into_iter()
                .filter_map(|own| match own.answer {
                    Answer::Refused {
                        kind: Refusal::Trust,
                        ..
                    } => Some(own.node.name()),
                    Answer::Unsent | Answer::Refused { .. } | Answer::Taken | Answer::LetGo => None,
                }),
        );
        let schema = <Event as prodrome_data::schema::Schema>::Vocabulary::default();
        let mut todos: BTreeMap<String, Unsent> = BTreeMap::new();
        let mut acts = Vec::new();
        for o in &untaken {
            let Some((todo, at)) =
                parse_envelope::<Event>(&schema, o.literal())
                    .ok()
                    .and_then(|e| {
                        e.event()
                            .map(|event| (event.todo().as_str().to_owned(), instant_of(event.at())))
                    })
            else {
                continue;
            };
            acts.push(serde_json::json!({
                "todo": todo,
                "name": o.name().as_str(),
                "at": at.format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
            }));
            let now = if held_back.contains(o.name()) {
                Unsent::Refused
            } else if trusting.contains(o.name()) {
                Unsent::AwaitingTrust
            } else {
                Unsent::Sending
            };
            let was = todos.entry(todo).or_insert(now);
            *was = (*was).max(now);
        }
        // Each refusal for good and each write it or a let-go holds back, derived as they stand now, one sentence per
        // write and none for a signature.
        let refusals: Vec<String> = replica
            .refusals()
            .into_iter()
            .filter(|(o, ..)| !o.is_signature())
            .map(|(_, _, reason)| reason)
            .collect();
        json(&serde_json::json!({
            "todos": todos,
            "acts": acts,
            "refused": refusals,
            "blocked": untaken.iter().filter(|o| held_back.contains(o.name())).count(),
            "stranded": replica.stranded().into_iter().filter(|o| !o.is_signature()).count(),
            "losses": untaken.iter().map(|o| o.name().as_str()).collect::<Vec<_>>(),
            "waiting": untaken.iter().filter(|o| trusting.contains(o.name())).count(),
        }))
    }
}

/// How two sets of heads stand in the causal order over what stands.
#[derive(serde::Serialize)]
enum Order {
    Same,
    Below,
    Above,
    Apart,
    Unknown,
}

#[wasm_bindgen]
impl Chain {
    /// `"Same"`, `"Below"`, `"Above"`, `"Apart"` or `"Unknown"`: how the heads `a` stand to `b`
    /// (JSON arrays of names), read off what stands; a set naming an object not held is unknown
    /// to every set but itself.
    pub fn order(&self, a: &str, b: &str) -> Result<String, JsError> {
        let (a, b) = (hashes(a, "a")?, hashes(b, "b")?);
        let standing = self.0.replica().standing();
        let by: BTreeMap<&Hash, &Object> = standing.iter().map(|o| (o.name(), *o)).collect();
        let under = |names: &BTreeSet<Hash>| -> Option<BTreeSet<Hash>> {
            let mut seen = BTreeSet::new();
            let mut left: Vec<Hash> = names.iter().cloned().collect();
            while let Some(name) = left.pop() {
                if seen.contains(&name) {
                    continue;
                }
                left.extend(by.get(&name)?.rests_on().iter().cloned());
                seen.insert(name);
            }
            Some(seen)
        };
        let order = if a == b {
            Order::Same
        } else {
            match (under(&a), under(&b)) {
                (Some(_), Some(y)) if a.iter().all(|n| y.contains(n)) => Order::Below,
                (Some(x), Some(_)) if b.iter().all(|n| x.contains(n)) => Order::Above,
                (Some(_), Some(_)) => Order::Apart,
                (None, _) | (_, None) => Order::Unknown,
            }
        };
        json(&order)
    }
}

/// The names of `writes` that are `from` or rest on one, read in one pass after another until
/// none is added.
fn resting_on<'o>(
    writes: &[&'o Object],
    from: impl Iterator<Item = &'o Hash>,
) -> BTreeSet<&'o Hash> {
    let mut found: BTreeSet<&Hash> = from.collect();
    loop {
        let more: Vec<&Hash> = writes
            .iter()
            .filter(|o| !found.contains(o.name()) && o.rests_on().iter().any(|p| found.contains(p)))
            .map(|o| o.name())
            .collect();
        if more.is_empty() {
            return found;
        }
        found.extend(more);
    }
}

#[cfg(test)]
mod tests {
    use super::Chain;
    use crate::Event;
    use crate::lifecycle::lifecycle;
    use suzatary_store::replica::held::Node as _;
    use suzatary_store::replica::objects::Object;

    /// The chain's genesis landed, as a pull of it leaves it.
    fn begun() -> Chain {
        let genesis = format!("Genesis(label='life', nonce='{}')", "0".repeat(32));
        let schema = <Event as prodrome_data::schema::Schema>::Vocabulary::default();
        let hash = Object::read::<Event>(&schema, genesis.clone())
            .name()
            .as_str()
            .to_owned();
        let answer = serde_json::json!({
            "objects": [{ "hash": hash, "literal": genesis }], "heads": [hash], "untrusted": [],
            "root": null, "geneses": [hash], "epoch": 1, "since": "Known",
            "frontier": { "digest": "9".repeat(64), "count": 1 },
        });
        let mut chain = Chain::new();
        chain.piece(&answer.to_string(), 1.0).expect("a piece");
        chain.land().expect("landed");
        chain
    }

    fn act(chain: &mut Chain, todo: &str, note: &str) {
        let event =
            lifecycle("Completed", todo, "2026-11-20T09:00:00", "owner", note).expect("an event");
        let changes: Vec<serde_json::Value> = serde_json::from_str(
            &chain
                .append(&serde_json::json!([event]).to_string())
                .expect("sealed"),
        )
        .expect("json");
        assert_eq!(changes.len(), 1, "no twin");
        let literals = serde_json::json!([changes[0]["literal"]]).to_string();
        chain
            .apply(&chain.writing(&literals).expect("a frame"))
            .expect("kept");
    }

    fn reading(chain: &Chain) -> serde_json::Value {
        serde_json::from_str(&chain.unsent_reading().expect("read")).expect("json")
    }

    /// The name of the unsent act on `todo` sealed with `note`, read off the reading's acts.
    fn named(chain: &Chain, todo: &str) -> Vec<String> {
        reading(chain)["acts"]
            .as_array()
            .expect("acts")
            .iter()
            .filter(|a| a["todo"] == todo)
            .map(|a| a["name"].as_str().expect("a name").to_owned())
            .collect()
    }

    /// Each todo says where its acts stand, the worst of them: one on its way, one waiting for
    /// this device's key, one refused for good with the act resting on it held back and named; a
    /// write let go is no loss and names no todo; every act is listed with its instant.
    #[test]
    fn the_unsent_reading_says_where_each_act_stands() {
        let mut chain = begun();
        act(&mut chain, "alpha", "a");
        act(&mut chain, "beta", "b");
        act(&mut chain, "gamma", "c1");
        let first = named(&chain, "gamma");
        act(&mut chain, "gamma", "c2");
        act(&mut chain, "delta", "d");
        let beta = named(&chain, "beta");
        let receipt = serde_json::json!({
            "heads": [], "adopted": [], "already": [],
            "refused": [{ "hash": beta[0], "reason": "a key", "kind": "trust" }],
            "since": "Known", "frontier": { "digest": "9".repeat(64), "count": 1 },
        });
        chain.answered(&receipt.to_string()).expect("answered");
        let frame = chain
            .let_go(&serde_json::json!(named(&chain, "delta")).to_string())
            .expect("planned");
        chain.apply(&frame).expect("kept");
        chain
            .refuse(&serde_json::json!(first).to_string(), "no")
            .expect("refused");
        let read = reading(&chain);
        assert_eq!(
            read["todos"],
            serde_json::json!({ "alpha": "Sending", "beta": "AwaitingTrust", "gamma": "Refused" })
        );
        let refused: Vec<&str> = read["refused"]
            .as_array()
            .expect("refused")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert_eq!(
            refused.len(),
            2,
            "the refused act and the one resting on it, each named"
        );
        assert!(refused.contains(&"no"));
        assert!(
            refused
                .iter()
                .any(|r| r.starts_with("it rests on ") && r.ends_with("which the box refused: no"))
        );
        assert_eq!(
            read["blocked"], 2,
            "the second of gamma's acts rests on the first"
        );
        assert_eq!(read["waiting"], 1);
        assert_eq!(read["stranded"], 0);
        let losses: std::collections::BTreeSet<String> = read["losses"]
            .as_array()
            .expect("losses")
            .iter()
            .map(|l| l.as_str().expect("a name").to_owned())
            .collect();
        let expected: std::collections::BTreeSet<String> = ["alpha", "beta", "gamma"]
            .iter()
            .flat_map(|t| named(&chain, t))
            .collect();
        assert_eq!(losses.len(), 4);
        assert_eq!(losses, expected, "a write let go is no loss");
        assert!(
            read["acts"]
                .as_array()
                .expect("acts")
                .iter()
                .all(|a| { a["at"] == "2026-11-20T09:00:00.000000" && a["todo"] != "delta" })
        );
    }
}
