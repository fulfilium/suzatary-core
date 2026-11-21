//! The door's clauses over the chain that read what is held (`sync::receive`): a lifecycle event
//! on a `decision` record is refused whether the record was held before the post or rides in it;
//! a change the chain's own check would report once held is refused for good.

use prodrome_core::change::mk_change;
use prodrome_core::event::{
    Actor, Hash, canonical_envelope, mk_completed, mk_created, mk_reopened, seal_hash,
};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::Datetime;
use prodrome_core::sign::Secret;
use suzatary_api::sync::{Objects, Reason, receive};
use suzatary_record::record::{Envelope, MarkupSource, TodoEvent, mk_authored};

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 1, 9, minute, 0, 0).expect("an instant")
}

fn change(genesis: &Hash, deps: Vec<Hash>, event: TodoEvent) -> Envelope {
    Envelope::Change(mk_change(genesis.clone(), deps, event).expect("a change"))
}

/// What `held` and then a post of `posted` by ada answer of `completed`.
fn refusal_of(held: &[&Envelope], posted: &[&Envelope], completed: &Hash) -> Option<Reason> {
    let mut objects = Objects::default();
    for object in held {
        objects.insert(seal_hash(object), (*object).clone());
    }
    let posted: Vec<String> = posted.iter().map(|o| canonical_envelope(o)).collect();
    let root = Secret::new(&"ab".repeat(32)).expect("a seed").public();
    let ada = Actor::new("ada").expect("an actor");
    receive(
        &objects,
        &posted,
        &ada,
        &root,
        &prodrome_core::policy::Untrusted::none(),
    )
    .refused
    .into_iter()
    .find(|refused| refused.name.as_ref() == Some(completed))
    .map(|refused| refused.reason)
}

/// Ada authors a `decision` record and completes it, posted together and posted apart.
#[test]
fn a_decision_record_seals_its_todo_whether_held_or_posted_beside() {
    let genesis: Envelope =
        Envelope::Genesis(mk_genesis("chain", &"cd".repeat(16)).expect("a genesis"));
    let decision = change(
        &seal_hash(&genesis),
        vec![seal_hash(&genesis)],
        mk_authored(
            "settlethis",
            at(0),
            "ada",
            "decision",
            at(0),
            MarkupSource::escaped("Send the reply"),
            prodrome_core::fpl::mk_absent(),
            Vec::new(),
            "",
            "",
            MarkupSource::escaped("proposal:default/0\n\nthe body a person reads"),
            None,
            Vec::new(),
            Vec::new(),
            "",
        )
        .expect("a decision record"),
    );
    let completed = change(
        &seal_hash(&genesis),
        vec![seal_hash(&decision)],
        mk_completed("settlethis", at(1), "ada", "").expect("a completion"),
    );
    let name = seal_hash(&completed);
    let apart = refusal_of(&[&genesis, &decision], &[&completed], &name);
    let together = refusal_of(&[&genesis], &[&decision, &completed], &name);
    assert_eq!(apart, Some(Reason::SealedDecision));
    assert_eq!(together, apart);
}

/// A change resting on a write and on what that write rests on is refused for good, in the
/// check's sentence.
#[test]
fn a_change_the_chain_s_check_would_report_is_refused_for_good() {
    let genesis: Envelope =
        Envelope::Genesis(mk_genesis("chain", &"cd".repeat(16)).expect("a genesis"));
    let begun = seal_hash(&genesis);
    let created = change(
        &begun,
        Vec::new(),
        mk_created("alpha", at(0), "ada", "", "").expect("valid"),
    );
    let completed = change(
        &begun,
        vec![seal_hash(&created)],
        mk_completed("alpha", at(1), "ada", "").expect("valid"),
    );
    let reopened = change(
        &begun,
        vec![seal_hash(&created), seal_hash(&completed)],
        mk_reopened("alpha", at(2), "ada", "").expect("valid"),
    );
    // Ada's device key, registered by the root `refusal_of` names, signs the post.
    let (root, phone) = (
        Secret::new(&"ab".repeat(32)).expect("a seed"),
        Secret::new(&"cd".repeat(32)).expect("a seed"),
    );
    let added = Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
        genesis: begun,
        actor: Actor::new("ada").expect("an actor"),
        key: phone.public(),
    });
    let registered = Envelope::Signed(root.sign(&seal_hash(&added)));
    let signed = Envelope::Signed(phone.sign(&seal_hash(&reopened)));
    let why = refusal_of(
        &[&genesis, &created, &completed, &added, &registered],
        &[&reopened, &signed],
        &seal_hash(&reopened),
    );
    assert!(
        matches!(&why, Some(Reason::Reported(found)) if found.iter().any(|f| f.contains("already rests on"))),
        "{why:?}"
    );
}
