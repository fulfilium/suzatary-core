use std::sync::{Condvar, Mutex, PoisonError};
use std::thread::ThreadId;

use super::{Reentrant, StoreError, WriteLock};

/// What serialises a prodrome's writers above the database's own lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// This process's threads, reentrant, since a batch holds it while each write inside takes
    /// its own.
    Reentrant,
    /// An `flock` on this file beside the objects, opened afresh for each hold, so threads wait
    /// on each other as processes do; never taken twice by one thread.
    File(&'static str),
}

/// A hold in this process that refuses a second take by the thread holding it, as the box's
/// `flock` would hang on one, naming the store.
#[derive(Debug, Default)]
pub struct Once {
    holder: Mutex<Option<ThreadId>>,
    released: Condvar,
}

impl Once {
    /// Hold it until the returned guard is dropped; another thread waits.
    ///
    /// # Errors
    /// [`StoreError`] when this thread already holds it.
    pub fn hold(&self, store: &'static str) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        let me = std::thread::current().id();
        let mut holder = self.holder.lock().unwrap_or_else(PoisonError::into_inner);
        if *holder == Some(me) {
            return Err(StoreError::not_appended(
                store,
                "this thread already holds the store's lock; on the box that is a file lock one \
                 process cannot take twice, so this would never return",
            ));
        }
        while holder.is_some() {
            holder = self
                .released
                .wait(holder)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *holder = Some(me);
        Ok(Box::new(OnceHeld { by: self }))
    }
}

struct OnceHeld<'a> {
    by: &'a Once,
}

impl WriteLock for OnceHeld<'_> {}

impl Drop for OnceHeld<'_> {
    fn drop(&mut self) {
        *self
            .by
            .holder
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        self.by.released.notify_one();
    }
}

/// A store in memory's hold, as its schema's [`Hold`] says: a file lock on the box is one a
/// thread cannot take twice, so in memory it is a [`Once`], and a test that would hang on the box
/// fails here.
#[derive(Debug)]
pub enum InMemoryHold {
    /// [`Hold::Reentrant`].
    Reentrant(Reentrant),
    /// [`Hold::File`].
    Once(Once),
}

impl InMemoryHold {
    /// The hold for `hold`.
    #[must_use]
    pub fn of(hold: Hold) -> Self {
        match hold {
            Hold::Reentrant => Self::Reentrant(Reentrant::default()),
            Hold::File(_) => Self::Once(Once::default()),
        }
    }

    /// Hold it.
    ///
    /// # Errors
    /// As [`Once::hold`].
    pub fn hold(&self, store: &'static str) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        match self {
            Self::Reentrant(hold) => Ok(hold.hold()),
            Self::Once(hold) => hold.hold(store),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn a_second_take_by_its_holder_is_refused() {
        let hold = Once::default();
        let first = hold.hold("inbox").expect("free");
        assert!(hold.hold("inbox").is_err());
        drop(first);
        hold.hold("inbox").expect("free again");
    }

    #[test]
    fn another_thread_waits() {
        let hold = Arc::new(Once::default());
        let entered = Arc::new(AtomicBool::new(false));
        let first = hold.hold("inbox").expect("free");
        let other = {
            let hold = Arc::clone(&hold);
            let entered = Arc::clone(&entered);
            std::thread::spawn(move || {
                let _wait = hold.hold("inbox").expect("after the first");
                entered.store(true, Ordering::SeqCst);
            })
        };
        assert!(!entered.load(Ordering::SeqCst));
        drop(first);
        other.join().expect("the other thread finishes");
        assert!(entered.load(Ordering::SeqCst));
    }
}
