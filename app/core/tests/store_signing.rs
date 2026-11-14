//! `store::signing`'s laws. They hold their store in [`suzatary_core::stores::InMemory`], which reads
//! its hold from `stores::Store`, above the store's cut.

use prodrome_core::event::{Envelope, mk_completed};
use prodrome_core::literal::Datetime;
use prodrome_core::sign::Secret;

use suzatary_core::stores::InMemory;
use suzatary_record::record::TodoEvent;
use suzatary_store::store::*;

fn done(todo: &str) -> TodoEvent {
    let at = Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant");
    mk_completed(todo, at, "ada", "").expect("an event")
}

/// Every append is joined by its key's signature, which verifies, and a replay is a twin
/// whose signature is the one already held.
#[test]
fn every_append_is_signed_once() {
    let key = Secret::new(&"1".repeat(64)).expect("a seed");
    let public = key.public();
    let store = Signing::new(InMemory::<TodoEvent>::default(), Signer::Key(key.into()));
    let written = store.append(done("alpha")).expect("appends");
    let signatures = |store: &Signing<InMemory<TodoEvent>>| -> Vec<_> {
        store
            .objects()
            .expect("reads")
            .into_iter()
            .filter_map(|(_, object)| match object {
                Envelope::Signed(signed) => Some(signed),
                _ => None,
            })
            .collect()
    };
    let held = signatures(&store);
    assert_eq!(held.len(), 1);
    assert!(held[0].object == written && held[0].key == public && held[0].verifies());
    assert_eq!(store.append(done("alpha")).expect("a twin"), written);
    assert_eq!(signatures(&store).len(), 1);
}

/// A signer that cannot sign refuses with its reason and appends nothing.
#[test]
fn a_missing_key_refuses_and_writes_nothing() {
    let store = Signing::new(
        InMemory::<TodoEvent>::default(),
        Signer::Refusing("SUZATARY_BOX_KEY does not resolve".to_owned()),
    );
    let refused = store.append(done("alpha")).expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("SUZATARY_BOX_KEY does not resolve")
    );
    assert!(store.objects().expect("reads").is_empty());
}
