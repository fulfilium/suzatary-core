//! A process's first whole read of a store, timed once, and split into its parts when asked.

/// The target a process's first read is logged at, once, part by part; it names no store's
/// content, only parts and milliseconds.
pub const FIRST_READ: &str = "suzatary::first_read";

/// Told what a process's first read was and how long it took; unset, nobody is.
#[cfg(not(target_arch = "wasm32"))]
static TOLD: std::sync::OnceLock<Box<dyn Fn(&'static str, std::time::Duration) + Send + Sync>> =
    std::sync::OnceLock::new();

/// Tell `teller` of this process's first read when it ends. The first teller set is the one told.
#[cfg(not(target_arch = "wasm32"))]
pub fn tell(teller: impl Fn(&'static str, std::time::Duration) + Send + Sync + 'static) {
    let _ = TOLD.set(Box::new(teller));
}

/// `run`'s answer; where it is this process's first read, told and logged at [`FIRST_READ`].
#[cfg(not(target_arch = "wasm32"))]
pub fn first<T>(what: &'static str, run: impl FnOnce() -> T) -> T {
    static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if FIRST.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return run();
    }
    let started = std::time::Instant::now();
    let answer = if tracing::enabled!(target: "suzatary::first_read", tracing::Level::INFO) {
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
    } else {
        run()
    };
    if let Some(teller) = TOLD.get() {
        teller(what, started.elapsed());
    }
    answer
}

#[cfg(target_arch = "wasm32")]
pub fn first<T>(_: &'static str, run: impl FnOnce() -> T) -> T {
    run()
}
