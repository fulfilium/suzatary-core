use std::sync::{Mutex, PoisonError};

use prodrome_core::memo::{Cache, Evict, Key, Lookup, name};
use prodrome_core::schema::Schema;

use super::{Prodrome, StoreError};

/// The key a reading of `store` is kept under in a [`Cache`]: the function, named by `function`
/// (its code and what it reads), at the argument the store's heads name. A store's objects are
/// named by their content and its heads name every object it holds, so a reading that is a
/// function of the objects is a function of the heads. Not for a reading that also takes the
/// clock: the heads do not name the instant.
///
/// # Errors
/// The heads unread.
fn key<E: Schema, S: Prodrome<E> + ?Sized>(function: &[u8], store: &S) -> Result<Key, StoreError> {
    Ok(Key {
        function: name(function, []),
        argument: name(b"heads", &store.tips()?),
    })
}

/// `read` of `store` through `cache`, the Prodrome's tabulation of a pure function
/// (`memo f = f`): what the cache holds under the function `function` at the store's heads,
/// or `read` of it now, then held. The heads are read first, so objects written between the two
/// are read now and held under the older heads, and the next ask reads again. The store is read
/// outside the hold. A key holding two values is a function that was not pure: it is read again,
/// never answered by picking one.
///
/// # Errors
/// The heads unread, or `read`'s own refusal; nothing is held from either.
pub fn memoised<V, Ev, E, S, F>(
    cache: &Mutex<Cache<V, Ev>>,
    function: &[u8],
    store: &S,
    read: F,
) -> Result<V, StoreError>
where
    V: Clone + Eq,
    Ev: Evict,
    E: Schema,
    S: Prodrome<E> + ?Sized,
    F: FnOnce(&S) -> Result<V, StoreError>,
{
    let key = key(function, store)?;
    let hold = || cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Lookup::Hit(value) = hold().get(&key) {
        return Ok(value.clone());
    }
    let value = read(store)?;
    // A second value under one key is the cache's finding to keep; this answer is the reading now.
    let _finding = hold().insert(key, value.clone());
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Mutex;

    use prodrome_core::event::mk_created;
    use prodrome_core::memo::{Cache, Lru};

    use super::memoised;
    use crate::record::TodoEvent;
    use crate::store::Reads as _;
    use crate::store::{InMemory, Prodrome, StoreError};

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
}
