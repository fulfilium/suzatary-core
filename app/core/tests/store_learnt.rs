//! `store::learnt`'s laws. They hold their store in [`suzatary_core::stores::InMemory`], which reads
//! its hold from `stores::Store`, above the store's cut.

use prodrome_core::declared::Declaration;

use prodrome_core::declared::Declared;
use suzatary_core::stores::InMemory;
use suzatary_store::store::Reads as _;
use suzatary_store::store::doors::{SCHEMA, seen};
use suzatary_store::store::learn;

#[test]
fn learning_twice_is_learning_once_whenever_it_was_seen() {
    let schema = Declaration::admit(SCHEMA).expect("lawful");
    let store = InMemory::<Declared>::at(schema.clone());
    let first = learn(&store, vec![seen(&schema, "front", 0, "open")]).expect("learns");
    assert_eq!((first.written, first.known), (1, 0));
    let again = learn(&store, vec![seen(&schema, "front", 5, "open")]).expect("learns");
    assert_eq!((again.written, again.known), (0, 1), "the instant is data");
    let shut = learn(
        &store,
        vec![
            seen(&schema, "front", 6, "shut"),
            seen(&schema, "front", 7, "shut"),
        ],
    )
    .expect("learns");
    assert_eq!((shut.written, shut.known), (1, 1));
    assert_eq!(
        store.objects().expect("reads").len(),
        3,
        "the genesis the first write began, and two changes"
    );
}
