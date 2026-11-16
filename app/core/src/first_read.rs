//! A process's first read of the conversations, logged once, part by part ([`crate::parts`]): the
//! one line that says where a cold read spends its time on a box.

/// The target a process's first read of the conversations is logged at, once: one line with the
/// time of each of its parts, so a cold read on a box can be split without a debugger. It names no
/// store's content: no word, no name, only parts and milliseconds.
pub const FIRST_READ: &str = "suzatary::first_read";

/// `run`'s answer, its parts logged at [`FIRST_READ`] where it is this process's first read.
#[cfg(not(target_arch = "wasm32"))]
pub fn first<T>(what: &'static str, run: impl FnOnce() -> T) -> T {
    static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !tracing::enabled!(target: "suzatary::first_read", tracing::Level::INFO)
        || TOLD.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        return run();
    }
    let started = std::time::Instant::now();
    let (answer, parts) = crate::parts::scoped(run);
    tracing::info!(
        target: "suzatary::first_read",
        what,
        ms = parts.sum().as_millis(),
        whole = started.elapsed().as_millis(),
        "first read: {}",
        parts.line("")
    );
    answer
}

#[cfg(target_arch = "wasm32")]
pub fn first<T>(_: &'static str, run: impl FnOnce() -> T) -> T {
    run()
}
