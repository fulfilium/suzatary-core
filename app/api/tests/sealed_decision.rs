//! The door's one clause over the chain that reads what is held (`sync::receive`'s sealed
//! decision) answers the same however a tab splits the same objects across posts: a lifecycle
//! event on a `decision` record is refused whether the record was held before the post or rides
//! in it.

use prodrome_core::change::mk_change;
use prodrome_core::event::{Actor, Hash, canonical_envelope, mk_completed, seal_hash};
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
    receive(&objects, &posted, &ada, &root)
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
