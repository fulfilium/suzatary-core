use std::cell::RefCell;

use prodrome_core::event::Hash;

thread_local! {
    /// The chain objects taken in on this thread while a [`recording`] runs.
    static WROTE: RefCell<Option<Vec<Hash>>> = const { RefCell::new(None) };
}

/// `work`, run on this thread, and every chain object a store took in while it ran ([`wrote`]).
/// Work handed to another thread is not recorded here.
pub fn recording<T>(work: impl FnOnce() -> T) -> (T, Vec<Hash>) {
    let outer = WROTE.with(|wrote| wrote.replace(Some(Vec::new())));
    let answer = work();
    let wrote = WROTE.with(|wrote| wrote.replace(outer)).unwrap_or_default();
    (answer, wrote)
}

/// Called by a chain store for each object it takes in; does nothing outside a [`recording`].
pub fn wrote(name: &Hash) {
    WROTE.with(|wrote| {
        if let Some(names) = wrote.borrow_mut().as_mut() {
            names.push(name.clone());
        }
    });
}
