use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::ThreadId;

use super::WriteLock;

/// A hold this process's threads wait on, reentrant within one thread, since a batch holds it
/// while each write inside takes its own.
#[derive(Debug, Default)]
pub struct Reentrant {
    writing: Mutex<()>,
    depth: Mutex<Option<(ThreadId, u32)>>,
}

impl Reentrant {
    /// Hold it until the returned guard is dropped.
    pub fn hold(&self) -> Box<dyn WriteLock + '_> {
        let me = std::thread::current().id();
        {
            let mut depth = self.depth.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((owner, level)) = depth.as_mut()
                && *owner == me
            {
                *level += 1;
                return Box::new(Held {
                    by: self,
                    _guard: None,
                });
            }
        }
        // Waited on without the depth mutex, or the holder could never record letting go.
        let guard = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        *self.depth.lock().unwrap_or_else(PoisonError::into_inner) = Some((me, 1));
        Box::new(Held {
            by: self,
            _guard: Some(guard),
        })
    }
}

/// Only the outermost hold owns the mutex.
struct Held<'a> {
    by: &'a Reentrant,
    _guard: Option<MutexGuard<'a, ()>>,
}

impl WriteLock for Held<'_> {}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        let mut depth = self.by.depth.lock().unwrap_or_else(PoisonError::into_inner);
        match depth.as_mut() {
            Some((_, level)) if *level > 1 => *level -= 1,
            _ => *depth = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// Reentrant, or the first two-intent batch would deadlock.
    #[test]
    fn one_thread_may_hold_it_again() {
        let hold = Reentrant::default();
        let outer = hold.hold();
        {
            let _inner = hold.hold();
            let _deeper = hold.hold();
        }
        drop(outer);
        let _after = hold.hold();
    }

    /// A second thread waits, so two acts never decide from the same chain.
    #[test]
    fn a_second_thread_waits() {
        let hold = Arc::new(Reentrant::default());
        let entered = Arc::new(AtomicBool::new(false));
        let first = hold.hold();
        let other = {
            let hold = Arc::clone(&hold);
            let entered = Arc::clone(&entered);
            std::thread::spawn(move || {
                let _wait = hold.hold();
                entered.store(true, Ordering::SeqCst);
            })
        };
        assert!(!entered.load(Ordering::SeqCst));
        drop(first);
        other.join().expect("the other thread finishes");
        assert!(entered.load(Ordering::SeqCst));
    }
}
