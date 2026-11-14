use std::cell::RefCell;

use prodrome_core::event::Hash;

thread_local! {
    /// The chain objects taken in on this thread while a [`recording`] runs.
    static WROTE: RefCell<Option<Vec<Hash>>> = const { RefCell::new(None) };
}

/// `work`, run on this thread, and every chain object a store took in while it ran ([`wrote`]):
/// what a device's request made the box write. A route performs on one thread from start to
/// answer, so what it wrote is what this sees; work it hands to another thread (a model's later
/// round) is that work's own.
pub fn recording<T>(work: impl FnOnce() -> T) -> (T, Vec<Hash>) {
    let outer = WROTE.with(|wrote| wrote.replace(Some(Vec::new())));
    let answer = work();
    let wrote = WROTE.with(|wrote| wrote.replace(outer)).unwrap_or_default();
    (answer, wrote)
}

/// Told by a chain store of each object it takes in, appended or accepted, so a [`recording`]
/// on this thread sees it; nothing happens outside one.
pub fn wrote(name: &Hash) {
    WROTE.with(|wrote| {
        if let Some(names) = wrote.borrow_mut().as_mut() {
            names.push(name.clone());
        }
    });
}
