//! SPEC law 42 in the tab: a device key's signature sealed as the Prodrome's `Signed`, and the
//! readings of who wrote what, by the core's own `Trust`. The key never crosses: the tab signs
//! with `WebCrypto` and hands over the signature, and this module checks and prints it.

use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::sign::{PublicKey, Registrar, Signature, Signed};
use suzatary_api::wire::{
    Condition, Found, ObjectVerdict, ProvenanceRow, SignedObject, Verified, VerifiedPayload, Walk,
};
use suzatary_core::signatures::{Enrolment, Trust};
use wasm_bindgen::prelude::*;

use crate::{Event, Todos, json as printed_json, refused};

/// `{hash, text}`: the `Signed` of `object` by `key` with `signature` (the bytes `WebCrypto`
/// answered, as hex), printed canonically, refused unless it verifies, so a tab never seals a
/// signature the box would call forged.
#[wasm_bindgen]
pub fn signed(object: &str, key: &str, signature: &str) -> Result<String, JsError> {
    let signed = Signed {
        object: Hash::new(object).map_err(|e| refused(format!("object: {e}")))?,
        key: PublicKey::new(key).map_err(|e| refused(format!("key: {e}")))?,
        signature: Signature::new(signature).map_err(|e| refused(format!("signature: {e}")))?,
    };
    if !signed.verifies() {
        return Err(refused("the signature does not verify over the object"));
    }
    let envelope = Envelope::<Event>::Signed(signed);
    printed_json(&SignedObject {
        hash: seal_hash(&envelope).as_str().to_owned(),
        text: canonical_envelope(&envelope),
    })
}

fn root_of(root: Option<String>) -> Result<Registrar, JsError> {
    Ok(match root {
        None => Registrar::Anyone,
        Some(root) => Registrar::Roots(
            [PublicKey::new(root).map_err(|e| refused(format!("root: {e}")))?]
                .into_iter()
                .collect(),
        ),
    })
}

#[wasm_bindgen]
impl Todos {
    /// The name of the `KeyAdded` a device card asks for, in these objects' first prodrome:
    /// what a trusted device signs to vouch for `key` writing as `actor`.
    pub fn key_added(&self, actor: &str, key: &str) -> Result<String, JsError> {
        let read = self.0.read().map_err(refused)?;
        let enrolment = Enrolment {
            actor: Actor::new(actor).map_err(|e| refused(format!("actor: {e}")))?,
            key: PublicKey::new(key).map_err(|e| refused(format!("key: {e}")))?,
        };
        enrolment
            .added(read.dag)
            .map(|added| seal_hash(&added).as_str().to_owned())
            .ok_or_else(|| refused("these objects begin no prodrome yet"))
    }

    /// `trusted`, `untrusted` or `revoked`: what these objects say of `key` for `actor` in every
    /// prodrome they hold, under `root` (`null`: every registration counted).
    pub fn standing(
        &self,
        root: Option<String>,
        actor: &str,
        key: &str,
    ) -> Result<String, JsError> {
        let read = self.0.read().map_err(refused)?;
        let trust = Trust::of(read.dag, &root_of(root)?);
        let actor = Actor::new(actor).map_err(|e| refused(format!("actor: {e}")))?;
        let key = PublicKey::new(key).map_err(|e| refused(format!("key: {e}")))?;
        let standings: Vec<_> = read
            .dag
            .geneses()
            .iter()
            .map(|genesis| trust.standing(&read.dag.key(genesis), &actor, &key))
            .collect();
        let worst = standings
            .iter()
            .copied()
            .find(|standing| !standing.signs())
            .or_else(|| standings.first().copied())
            .unwrap_or(suzatary_core::signatures::KeyStanding::Untrusted);
        Ok(worst.as_str().to_owned())
    }

    /// The class's `verify`, with each object beneath an epoch that no key proves read as
    /// history written before signatures rather than as a finding, as the box's own verify
    /// reads it (`suzatary_core::signatures::since_epoch`).
    pub fn verify_history(&self) -> Result<String, JsError> {
        // Objects with no honest reading have no epoch to read either: their findings stand.
        let history: std::collections::BTreeSet<String> = self
            .0
            .read()
            .map(|read| {
                Trust::of(read.dag, &Registrar::Anyone)
                    .predating()
                    .iter()
                    .filter_map(|name| {
                        read.dag
                            .get(name)
                            .and_then(|object| object.event())
                            .map(|event| {
                                prodrome_core::dag::Finding::Unsigned {
                                    object: name.clone(),
                                    actor: event.actor().clone(),
                                }
                                .to_string()
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let answered: Answered =
            serde_json::from_str(&prodrome_wasm_exports::verify(&self.0).map_err(refused)?)
                .map_err(|e| refused(format!("the Prodrome's verify: {e}")))?;
        printed_json(&answered.verified(&history).map_err(refused)?)
    }

    /// Who wrote each object carrying an event, under `root`: proven by a key, written before
    /// signatures, or not proven and why.
    pub fn provenance(&self, root: Option<String>) -> Result<String, JsError> {
        let read = self.0.read().map_err(refused)?;
        let trust = Trust::of(read.dag, &root_of(root)?);
        let rows: Vec<ProvenanceRow> = read
            .nodes
            .iter()
            .filter_map(|node| {
                trust
                    .provenance(node.name())
                    .map(|provenance| ProvenanceRow {
                        hash: node.name().as_str().to_owned(),
                        proof: (&provenance).into(),
                        said: provenance.said(),
                    })
            })
            .collect();
        printed_json(&rows)
    }
}

/// The Prodrome's `verify` answer as it prints it, read once: where the walk put each object is
/// a flag and a depth, and what it found a flag and a list, each pair read here into the sum it
/// means.
#[derive(serde::Deserialize)]
struct Answered {
    tips: Vec<String>,
    objects: Vec<Row>,
    walked: u32,
    verified: u32,
    genesis: Vec<String>,
    linearisation: Vec<String>,
    problems: Vec<String>,
}

/// One object of [`Answered`].
#[derive(serde::Deserialize)]
struct Row {
    hash: String,
    computed: String,
    #[serde(rename = "hashOk")]
    hash_ok: bool,
    reachable: bool,
    depth: Option<u32>,
    kind: String,
    todo: String,
    actor: String,
    at: String,
    problem: String,
}

impl Answered {
    /// The answer with every finding `history` names left out, as the page reads it; a row whose
    /// flag and depth disagree is refused, naming it.
    fn verified(
        self,
        history: &std::collections::BTreeSet<String>,
    ) -> Result<VerifiedPayload, String> {
        let objects = self
            .objects
            .into_iter()
            .map(|row| {
                let walk = match (row.reachable, row.depth) {
                    (true, Some(depth)) => Walk::Reached { depth },
                    (false, None) => Walk::Unreached,
                    (reachable, depth) => {
                        return Err(format!(
                            "object {} is reachable: {reachable} at depth {depth:?}",
                            row.hash
                        ));
                    }
                };
                // The Prodrome prints an envelope's todo, writer and instant as "".
                let holds = if row.todo.is_empty() {
                    Verified::Envelope { kind: row.kind }
                } else {
                    Verified::Event {
                        kind: row.kind,
                        todo: row.todo,
                        actor: row.actor,
                        at: row.at,
                    }
                };
                Ok(ObjectVerdict {
                    hash: row.hash,
                    computed: row.computed,
                    hash_ok: row.hash_ok,
                    walk,
                    condition: Condition::of(row.problem),
                    holds,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(VerifiedPayload {
            tips: self.tips,
            objects,
            walked: self.walked,
            verified: self.verified,
            genesis: self.genesis,
            linearisation: self.linearisation,
            found: Found::of(
                self.problems
                    .into_iter()
                    .filter(|problem| !history.contains(problem))
                    .collect(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(objects: &serde_json::Value, problems: &[&str]) -> Answered {
        serde_json::from_value(serde_json::json!({
            "ok": problems.is_empty(), "tips": ["a"], "objects": objects, "walked": 1,
            "verified": 1, "genesis": ["a"], "linearisation": ["a"], "problems": problems,
        }))
        .expect("the Prodrome's shape")
    }

    fn row(reachable: bool, depth: Option<u32>) -> serde_json::Value {
        serde_json::json!({
            "hash": "a", "computed": "a", "hashOk": true, "reachable": reachable,
            "depth": depth, "kind": "Created", "todo": "t", "actor": "ada",
            "at": "2026-09-01T00:00:00", "problem": "",
        })
    }

    /// A finding that only says an object predates signatures is history, and leaves none.
    #[test]
    fn history_is_not_a_finding() {
        let history = ["object a predates".to_owned()].into_iter().collect();
        let read = answered(
            &serde_json::json!([row(true, Some(0))]),
            &["object a predates"],
        )
        .verified(&history)
        .expect("reads");
        assert_eq!(read.found, Found::Reached);
        assert_eq!(read.objects[0].walk, Walk::Reached { depth: 0 });
        assert_eq!(read.objects[0].condition, Condition::Clean);
        let kept = answered(
            &serde_json::json!([row(false, None)]),
            &["object a was sent twice"],
        )
        .verified(&history)
        .expect("reads");
        assert_eq!(
            kept.found,
            Found::of(vec!["object a was sent twice".to_owned()])
        );
        assert_eq!(kept.objects[0].walk, Walk::Unreached);
    }

    /// The Prodrome's empty sentence is a clean object, and any other is the object's flaw.
    #[test]
    fn a_row_says_clean_or_its_flaw() {
        let mut flawed = row(true, Some(0));
        flawed["problem"] = serde_json::json!("does not hash to its own name");
        let read = answered(&serde_json::json!([row(true, Some(0)), flawed]), &[])
            .verified(&std::collections::BTreeSet::new())
            .expect("reads");
        assert_eq!(read.objects[0].condition, Condition::Clean);
        assert_eq!(
            read.objects[1].condition,
            Condition::Flawed {
                problem: "does not hash to its own name".to_owned()
            }
        );
    }

    /// A row the Prodrome prints with no todo is an envelope, which claims no writer or instant.
    #[test]
    fn a_row_with_no_todo_is_an_envelope() {
        let mut merge = row(true, Some(0));
        merge["kind"] = serde_json::json!("Woven");
        merge["todo"] = serde_json::json!("");
        merge["actor"] = serde_json::json!("");
        merge["at"] = serde_json::json!("");
        let read = answered(&serde_json::json!([merge, row(true, Some(1))]), &[])
            .verified(&std::collections::BTreeSet::new())
            .expect("reads");
        assert_eq!(
            read.objects[0].holds,
            Verified::Envelope {
                kind: "Woven".to_owned()
            }
        );
        assert!(matches!(read.objects[1].holds, Verified::Event { .. }));
    }

    /// A row the walk reached with no depth, or one it did not reach at a depth, is refused.
    #[test]
    fn a_walk_that_contradicts_itself_is_refused() {
        for (reachable, depth) in [(true, None), (false, Some(2))] {
            assert!(
                answered(&serde_json::json!([row(reachable, depth)]), &[])
                    .verified(&std::collections::BTreeSet::new())
                    .is_err()
            );
        }
    }
}
