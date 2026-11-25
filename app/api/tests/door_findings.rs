//! The chain's door against the check a store runs over itself: what the door refuses as the
//! check's, the store's check reports once the store holds it; what it takes, the check reports
//! nothing new of. Drawn on a keyed chain shaped as an adopted one: a merge of its tips covers the
//! device's key, and every write after rests on the merge, so the key lies beneath every dep a
//! device's change names. Posts are signed by the device's key, or not.

use std::collections::BTreeSet;

use prodrome_dag::dag::Dag;
use prodrome_dag::sign::Secret;
use prodrome_data::change::mk_change;
use prodrome_data::event::{
    Actor, Hash, canonical_envelope, mk_sealed, mk_woven, parse_envelope, seal_hash,
};
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use prodrome_data::sign::KeyAdded;
use prodrome_store::store::{MemoryStore, Replica as _};
use prodrome_todo::event::{mk_completed, mk_created};
use proptest::prelude::*;
use suzatary_api::sync::{Objects, Reason, receive};
use suzatary_record::record::{Envelope, TodoEvent, TodoVocabulary};

fn at(n: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9, n % 60, 0, 0).expect("an instant")
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

/// The chain's policy: the triage agent's writes are recorded and never bind.
fn policy() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("an actor")])
}

/// An adopted chain, its device and the device's replica of it.
struct Chain {
    held: Objects,
    root: Secret,
    phone: Secret,
    tab: MemoryStore<TodoEvent>,
    /// The root write, which every legacy write rests on.
    first: Hash,
    /// The writes after the merge, to `alpha` and to `beta`.
    alpha: Hash,
    beta: Hash,
}

/// `alpha` and `beta` written, then ada's device key registered by the root, a merge of the tips
/// and the key, and a write to each todo after it, by the triage agent, who has no key.
fn adopted() -> Chain {
    let root = Secret::new(&"ab".repeat(32)).expect("a seed");
    let phone = Secret::new(&"cd".repeat(32)).expect("a seed");
    let first = mk_sealed(
        None,
        mk_created("alpha", at(0), "ada", "", "").expect("valid"),
    );
    let beta = mk_sealed(
        Some(seal_hash(&first)),
        mk_created("beta", at(1), "ada", "", "").expect("valid"),
    );
    let added = Envelope::KeyAdded(KeyAdded {
        genesis: seal_hash(&first),
        actor: ada(),
        key: phone.public(),
    });
    let registered = Envelope::Signed(root.sign(&seal_hash(&added)));
    let mut tips = vec![seal_hash(&beta), seal_hash(&added)];
    tips.sort();
    let merge: Envelope = mk_woven(tips, None).expect("a merge");
    let alpha_after = mk_sealed(
        Some(seal_hash(&merge)),
        mk_completed("alpha", at(2), "triage", "").expect("valid"),
    );
    let beta_after = mk_sealed(
        Some(seal_hash(&alpha_after)),
        mk_completed("beta", at(3), "triage", "").expect("valid"),
    );
    let all = [
        first,
        beta,
        added,
        registered,
        merge,
        alpha_after,
        beta_after,
    ];
    let mut held = Objects::default();
    for object in &all {
        held.insert(seal_hash(object), object.clone());
    }
    let tab = MemoryStore::default();
    let prints: std::collections::BTreeMap<Hash, String> = all
        .iter()
        .map(|object| (seal_hash(object), canonical_envelope(object)))
        .collect();
    let dag: Dag<TodoEvent> = all
        .iter()
        .map(|object| (seal_hash(object), object.clone()))
        .collect();
    tab.receive(
        dag.tips(),
        &|name| prints.get(name).map(|print| print.clone().into_bytes()),
        prodrome_data::husk::Sender::Stranger,
    )
    .expect("a closed set is received");
    Chain {
        held,
        root,
        phone,
        tab,
        first: seal_hash(&all[0]),
        alpha: seal_hash(&all[5]),
        beta: seal_hash(&all[6]),
    }
}

/// One change a device posts: over its todo's heads; over them and a write beneath them; or over
/// a write to another todo. Signed by the device's key, or not.
#[derive(Debug, Clone, Copy)]
enum Write {
    Over { beta: bool, signed: bool },
    Beneath { signed: bool },
    Across { signed: bool },
}

fn write() -> impl Strategy<Value = Write> {
    prop_oneof![
        3 => (any::<bool>(), any::<bool>()).prop_map(|(beta, signed)| Write::Over { beta, signed }),
        1 => any::<bool>().prop_map(|signed| Write::Beneath { signed }),
        1 => any::<bool>().prop_map(|signed| Write::Across { signed }),
    ]
}

impl Chain {
    /// `write`, as the device posts it: its change and, if signed, its signature.
    fn wrote(&self, write: Write, n: u32) -> (Hash, Vec<String>) {
        let event = |todo: &str| mk_completed(todo, at(10 + n), "ada", "").expect("valid");
        let change: Envelope = match write {
            Write::Over { beta, .. } => {
                let name = self
                    .tab
                    .append(event(if beta { "beta" } else { "alpha" }))
                    .expect("the replica appends");
                let print = self.tab.print(&name).expect("held");
                parse_envelope(
                    &TodoVocabulary::default(),
                    std::str::from_utf8(&print).expect("utf8"),
                )
                .expect("a print")
            }
            Write::Beneath { .. } => Envelope::Change(
                mk_change(
                    self.first.clone(),
                    vec![self.alpha.clone(), self.first.clone()],
                    event("alpha"),
                )
                .expect("a change"),
            ),
            Write::Across { .. } => Envelope::Change(
                mk_change(self.first.clone(), vec![self.beta.clone()], event("alpha"))
                    .expect("a change"),
            ),
        };
        let name = seal_hash(&change);
        let signed = match write {
            Write::Over { signed, .. } | Write::Beneath { signed } | Write::Across { signed } => {
                signed
            }
        };
        let mut posted = vec![canonical_envelope(&change)];
        if signed {
            posted.push(canonical_envelope(&Envelope::Signed(
                self.phone.sign(&name),
            )));
        }
        (name, posted)
    }
}

/// What the store's check reports of `held` beside `more`, under the chain's policy.
fn findings(held: &Objects, more: &[(Hash, Envelope)]) -> BTreeSet<String> {
    let dag: Dag<TodoEvent> = held
        .names()
        .into_iter()
        .map(|name| {
            let object = held.get(&name).expect("held").clone();
            (name, object)
        })
        .chain(more.iter().cloned())
        .collect();
    suzatary_signatures::signatures::findings_of(&dag, &policy())
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn parsed(print: &str) -> (Hash, Envelope) {
    let object: Envelope = parse_envelope(&TodoVocabulary::default(), print).expect("a print");
    (seal_hash(&object), object)
}

/// A device's change over its todo's heads, signed by its key, is taken with its signature,
/// though the key it is signed under lies beneath its dep: the check of what a change rests on
/// holds no signature of it, and judges none.
#[test]
fn a_signed_change_over_an_adopted_chain_is_taken() {
    let chain = adopted();
    let (name, posted) = chain.wrote(
        Write::Over {
            beta: false,
            signed: true,
        },
        0,
    );
    let outcome = receive(
        &chain.held,
        &posted,
        &ada(),
        &chain.root.public(),
        &policy(),
    );
    assert!(outcome.refused.is_empty(), "{outcome:?}");
    assert!(outcome.taken.iter().any(|(taken, _)| *taken == name));
    assert_eq!(outcome.taken.len(), 2, "the change and its signature");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Over generated posts to an adopted chain, signed and unsigned: the store's check of the
    /// chain with what the door takes reports nothing it did not report before; each change the
    /// door refuses as the check's, the check reports once the chain holds it; and no change the
    /// device signed over its todo's heads is refused as the check's.
    #[test]
    fn the_chain_s_door_refuses_as_the_check_s_what_the_check_reports(
        posts in prop::collection::vec(prop::collection::vec(write(), 1..4), 1..4),
    ) {
        let mut chain = adopted();
        let mut n = 0u32;
        for post in posts {
            let mut written = Vec::new();
            let mut prints = Vec::new();
            for write in post {
                let (name, posted) = chain.wrote(write, n);
                n += 1;
                written.push((name, write, posted.clone()));
                prints.extend(posted);
            }
            let before = findings(&chain.held, &[]);
            let outcome = receive(&chain.held, &prints, &ada(), &chain.root.public(), &policy());
            for (name, print) in &outcome.taken {
                let (parsed_name, object) = parsed(print);
                prop_assert_eq!(&parsed_name, name);
                chain.held.insert(parsed_name, object);
            }
            let after = findings(&chain.held, &[]);
            let new: Vec<&String> = after.difference(&before).collect();
            prop_assert!(new.is_empty(), "taken, and reported: {:?}", new);
            for refused in &outcome.refused {
                let (Some(name), Reason::Reported(said)) = (&refused.name, &refused.reason) else {
                    continue;
                };
                let (_, write, posted) = written.iter().find(|(w, _, _)| w == name).expect("posted");
                prop_assert!(
                    !matches!(write, Write::Over { signed: true, .. }),
                    "a signed change over its heads, refused as {:?}", said
                );
                let more: Vec<(Hash, Envelope)> = posted.iter().map(|print| parsed(print)).collect();
                let held = findings(&chain.held, &more);
                prop_assert!(
                    held.difference(&after).any(|finding| finding.contains(name.as_str())),
                    "refused as {:?}, and the check reports nothing of it once held", said
                );
            }
        }
    }
}
