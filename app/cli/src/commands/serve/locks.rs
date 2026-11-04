//! The server's one policy for a poisoned lock: read through it.
//!
//! Every section the server holds a lock for either replaces the value whole or makes one call
//! on a map, so a panic leaves a value some write left whole. Refusing it would turn one
//! panicked request into every later request panicking.

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub fn held<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn read<T: ?Sized>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

pub fn write<T: ?Sized>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_reads_the_last_whole_write() {
        let mutex = Mutex::new(1);
        let lock = RwLock::new(1);
        std::thread::scope(|scope| {
            let panicked = scope
                .spawn(|| {
                    *held(&mutex) = 2;
                    *write(&lock) = 2;
                    let _mutex = held(&mutex);
                    let _lock = write(&lock);
                    panic!("a request panics while it holds both");
                })
                .join();
            assert!(panicked.is_err());
        });
        assert!(mutex.is_poisoned() && lock.is_poisoned());
        assert_eq!(*held(&mutex), 2);
        assert_eq!(*read(&lock), 2);
        *write(&lock) = 3;
        assert_eq!(*read(&lock), 3);
    }
}
