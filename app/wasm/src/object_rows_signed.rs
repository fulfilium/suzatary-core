//! A signature asserts no event, so the kept reading leaves signatures out (`read_objects`). The
//! law that this changes nothing a reading answers, over generated topic stores, the only stores
//! a tab signs. Their `tips` and `heads` differ, since a signature rests on what it signs; no
//! page reads those, so only what is not a signature among the tips with them is a tip without
//! them.

use prodrome_core::event::{Actor, Envelope, Hash, canonical, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::sign::Secret;
use suzatary_topics::secretary::topics::provenance::Owner;
use suzatary_topics::secretary::topics::reading::{self, AliasEvent, MembershipEvent};
use suzatary_topics::secretary::topics::topic::Topic;

use super::*;

fn replica<E: Schema<Vocabulary: Default>>(objects: impl Iterator<Item = Object>) -> Replica<E> {
    let objects: Vec<_> = objects
        .map(|o| serde_json::json!({ "hash": o.name().as_str(), "text": o.literal() }))
        .collect();
    Replica::of(
        &serde_json::Value::Array(objects).to_string(),
        prodrome_wasm_exports::Sender::Stranger,
    )
    .expect("a reading")
}

fn tips<E: Schema>(r: &Replica<E>) -> serde_json::Value {
    serde_json::from_str(&prodrome_wasm_exports::tips(r).expect("tips")).expect("JSON")
}

/// The law at one moment of `s`, `next` the event a write would append now.
fn law<E: Schema<Vocabulary: Default> + prodrome_wasm_exports::Json>(
    s: &Store<E>,
    next: Option<&str>,
    at: &str,
) -> usize {
    let standing: Vec<Object> = s.replica.standing().into_iter().cloned().collect();
    let signatures: Vec<&Object> = standing.iter().filter(|o| o.is_signature()).collect();
    let mut with = replica::<E>(standing.iter().cloned());
    let mut without = replica::<E>(standing.iter().filter(|o| !o.is_signature()).cloned());
    let (a, b) = (tips(&with), tips(&without));
    assert_eq!(
        a["excluded"], b["excluded"],
        "{at}: what the reading leaves out"
    );
    let read = |r: &Replica<E>| prodrome_wasm_exports::readings(r, None, "[]").expect("readings");
    assert_eq!(read(&with), read(&without), "{at}: the registers");
    let signed = |name: &serde_json::Value| signatures.iter().any(|o| o.name().as_str() == name);
    let changes = a["tips"]
        .as_array()
        .expect("tips")
        .iter()
        .filter(|t| !signed(t));
    for tip in changes {
        assert!(
            b["tips"].as_array().expect("tips").contains(tip),
            "{at}: tip {tip}"
        );
    }
    if let Some(event) = next {
        assert_eq!(
            prodrome_wasm_exports::append(&mut with, event, None),
            prodrome_wasm_exports::append(&mut without, event, None),
            "{at}: an append"
        );
    }
    signatures.len()
}

struct Rng(u64);

impl Rng {
    fn int(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

fn begun<E: Schema<Vocabulary: Default>>(label: &str) -> Store<E> {
    let mut s = Store::<E>::default();
    let genesis = format!("Genesis(label='{label}', nonce='{}')", "0".repeat(32));
    s.replica
        .write(vec![Object::read::<E>(&E::Vocabulary::default(), genesis)]);
    s
}

/// Every unsigned change of `s` signed by `key`.
fn sign<E: Schema<Vocabulary: Default>>(s: &mut Store<E>, key: &Secret) {
    let schema = E::Vocabulary::default();
    let unsigned: Vec<Hash> = s
        .replica
        .unsigned::<E>(&schema)
        .into_iter()
        .map(|o| o.name().clone())
        .collect();
    for name in unsigned {
        let print = canonical_envelope::<E>(&Envelope::Signed(key.sign(&name)));
        s.replica.write(vec![Object::read::<E>(&schema, print)]);
    }
    s.read = OnceCell::new();
}

/// One of the changes and signatures `s` wrote refused for good; its genesis stands for the box's.
fn refuse<E: Schema<Vocabulary: Default>>(s: &mut Store<E>, r: &mut Rng) -> usize {
    let own: Vec<Hash> = s
        .replica
        .unsent()
        .into_iter()
        .filter(|o| !o.literal().starts_with("Genesis("))
        .map(|o| o.name().clone())
        .collect();
    if let Some(name) = own.get(usize::try_from(r.int(own.len().max(1) as u64)).unwrap_or(0)) {
        let refused = [(
            name.clone(),
            Answer::Refused {
                kind: Refusal::Final,
                reason: "refused".to_owned(),
            },
        )];
        s.replica.answered(refused);
        s.read = OnceCell::new();
        return 1;
    }
    0
}

#[test]
fn a_signature_changes_no_reading() {
    let key = Secret::new(&"07".repeat(32)).expect("a seed");
    let owner = Owner::new(Actor::new("ada").expect("an actor")).expect("an owner");
    let (mut signed, mut refused) = (0, 0);
    for seed in 1..=40u64 {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut members = begun::<MembershipEvent>("topics");
        let mut aliases = begun::<AliasEvent>("topic-aliases");
        for n in 0..16u32 {
            let (m, a) = (
                members.fresh().expect("a reading"),
                aliases.fresh().expect("a reading"),
            );
            let (m, a) = (m.read().expect("nodes"), a.read().expect("nodes"));
            let reading = reading::of(m.nodes, a.nodes).expect("one prodrome each");
            let at = Datetime::new(2026, 11, 14, 8, n / 60, n % 60, 0).expect("an instant");
            let target = format!("round:c{}/{}", r.int(3), r.int(3))
                .parse()
                .expect("a target");
            let topic =
                Topic::new(["travel", "work"][usize::from(r.int(2) == 1)]).expect("a topic");
            let membership: Vec<String> = if r.int(2) == 0 {
                reading
                    .add(&target, &topic, at, &owner)
                    .iter()
                    .map(canonical)
                    .collect()
            } else {
                reading
                    .remove(&target, &topic, at, &owner)
                    .iter()
                    .map(canonical)
                    .collect()
            };
            let to = Topic::new("trips").expect("a topic");
            let alias: Vec<String> = reading
                .rename(&topic, &to, at, &owner)
                .iter()
                .map(canonical)
                .collect();
            let moment = format!("seed {seed}, move {n}");
            signed += law(&members, membership.first().map(String::as_str), &moment);
            signed += law(&aliases, alias.first().map(String::as_str), &moment);
            match r.int(4) {
                0 => {
                    if let Some(e) = membership.first() {
                        let f = members.append(e).expect("appended");
                        if let Some(f) = f {
                            members.apply(&f).expect("kept");
                        }
                    }
                }
                1 => {
                    if let Some(e) = alias.first() {
                        let f = aliases.append(e).expect("appended");
                        if let Some(f) = f {
                            aliases.apply(&f).expect("kept");
                        }
                    }
                }
                2 => {
                    sign(&mut members, &key);
                    sign(&mut aliases, &key);
                }
                _ => {
                    refused += refuse(&mut members, &mut r) + refuse(&mut aliases, &mut r);
                }
            }
        }
    }
    assert!(
        signed > 0 && refused > 0,
        "the histories sign and refuse: {signed}, {refused}"
    );
}
