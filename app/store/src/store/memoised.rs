use std::sync::{Mutex, PoisonError};

use prodrome_data::schema::Schema;
use prodrome_memo::{Cache, Evict, Key, Lookup, name};

use super::{Prodrome, StoreError};

/// The key a reading of `store` is kept under: `function` at the store's heads.
/// Not for a reading that also takes the clock: the heads do not name the instant.
fn key<E: Schema, S: Prodrome<E> + ?Sized>(function: &[u8], store: &S) -> Result<Key, StoreError> {
    Ok(Key {
        function: name(function, []),
        argument: name(b"heads", &store.tips()?),
    })
}

/// `read` of `store` through `cache` (`memo f = f`), keyed by `function` at the store's heads.
/// The heads are read first, so a write in between is held under the older heads and read again.
/// The store is read outside the hold.
/// A key holding two values is read again, never answered by picking one.
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
        return Ok(value);
    }
    let value = read(store)?;
    let _finding = hold().insert(key, value.clone());
    Ok(value)
}
