//! `store::memoised`'s laws. They hold their store in [`suzatary_core::stores::InMemory`], which reads
//! its hold from `stores::Store`, above the store's cut.

use std::cell::Cell;
use std::sync::Mutex;

use prodrome_core::event::mk_created;
use prodrome_core::memo::{Cache, Lru};

use suzatary_core::stores::InMemory;
use suzatary_record::record::TodoEvent;
use suzatary_store::store::Reads as _;
use suzatary_store::store::memoised;
use suzatary_store::store::{Prodrome, StoreError};

fn created(todo: &str) -> TodoEvent {
    mk_created(
        todo,
        prodrome_core::literal::Datetime::new(2026, 10, 13, 9, 0, 0, 0).expect("an instant"),
        "ada",
        "A todo",
        "",
    )
    .expect("valid")
}

// A count of the objects, read through the cache or straight: the cache never answers what a
// read of the store now would not.
#[test]
fn a_held_reading_is_the_stores_reading_now_and_read_once_per_heads() {
    let store: InMemory<TodoEvent> = InMemory::default();
    let cache = Mutex::new(Cache::new(Lru::new(4)));
    let reads = Cell::new(0);
    let counted = |s: &InMemory<TodoEvent>| {
        reads.set(reads.get() + 1);
        s.objects().map(|objects| objects.len())
    };
    for (i, todo) in ["one", "two", "three"].into_iter().enumerate() {
        store.append(created(todo)).expect("appends");
        for _ in 0..3 {
            let held = memoised(&cache, b"count", &store, counted).expect("reads");
            assert_eq!(held, store.objects().expect("reads").len());
        }
        assert_eq!(reads.get(), i + 1, "read once while the heads stood");
    }
}

#[test]
fn two_functions_over_one_store_are_two_entries_and_a_refusal_holds_nothing() {
    let store: InMemory<TodoEvent> = InMemory::default();
    store.append(created("one")).expect("appends");
    let cache = Mutex::new(Cache::new(Lru::new(4)));
    assert!(
        memoised(&cache, b"seven", &store, |_| Err(StoreError::unreadable(
            "memory", "refused"
        )))
        .is_err()
    );
    assert_eq!(
        memoised(&cache, b"seven", &store, |_| Ok(7)).expect("reads"),
        7
    );
    assert_eq!(
        memoised(&cache, b"eight", &store, |_| Ok(8)).expect("reads"),
        8
    );
    assert_eq!(
        memoised(&cache, b"seven", &store, |_| Ok(0)).expect("reads"),
        7
    );
}
