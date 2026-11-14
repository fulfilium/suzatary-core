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
