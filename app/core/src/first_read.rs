//! A process's first read of the conversations, logged once, part by part: the one line that
//! says where a cold read spends its time on a box. It needs a clock, a lock and `tracing`, and
//! nothing of the stores, so every module that does a part of the read may name it.

/// The target a process's first read of the conversations is logged at, once: one line with the
/// time of each of its parts, so a cold read on a box can be split without a debugger. It names no
/// store's content: no word, no name, only parts and milliseconds.
pub const FIRST_READ: &str = "suzatary::first_read";

// The browser's module has no subscriber and no clock (`std::time::Instant::now` panics on
// `wasm32-unknown-unknown`), so there every function is a no-op the compiler removes, and the
// module ships none of this: two bodies, chosen by target here, never at a call site.
#[cfg(target_arch = "wasm32")]
pub use browser::{add_part, around, begins, done, mine, part};
#[cfg(not(target_arch = "wasm32"))]
pub use native::{add_part, around, begins, done, mine, part};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::time::Instant;

    /// Each part of a first read: its name, its time summed, and how many times it ran.
    type Parts = Vec<(&'static str, std::time::Duration, usize)>;

    /// Where this process's first read stands. Only the thread that began it sums parts into it, so a
    /// read another thread runs meanwhile (serve's compactor folds beside its start) adds nothing.
    enum First {
        /// No read has begun.
        Waiting,
        /// The thread `by` is reading, and these are its parts so far.
        Reading {
            by: std::thread::ThreadId,
            parts: Parts,
        },
        /// The first read was told; nothing is summed again.
        Told,
    }

    static FIRST: std::sync::Mutex<First> = std::sync::Mutex::new(First::Waiting);

    /// Begin this process's first read on this thread, where none has begun and a subscriber listens
    /// at [`super::FIRST_READ`]: its start, which [`done`] is given back. `None` otherwise.
    pub fn begins() -> Option<Instant> {
        if !tracing::enabled!(target: "suzatary::first_read", tracing::Level::INFO) {
            return None;
        }
        let mut first = first();
        match *first {
            First::Waiting => {
                *first = First::Reading {
                    by: std::thread::current().id(),
                    parts: Vec::new(),
                };
                Some(Instant::now())
            }
            First::Reading { .. } | First::Told => None,
        }
    }

    /// Whether this thread is reading this process's first read now.
    #[must_use]
    pub fn mine() -> bool {
        matches!(*first(), First::Reading { by, .. } if by == std::thread::current().id())
    }

    /// `run`'s answer, its time added to `part` of this process's first read while this thread is
    /// reading it. The clock is read only then. A part is a leaf of the read: none is timed inside
    /// another, so their sum is time spent once.
    pub fn part<T>(part: &'static str, run: impl FnOnce() -> T) -> T {
        if !mine() {
            return run();
        }
        let started = Instant::now();
        let answer = run();
        add_part(part, started.elapsed());
        answer
    }

    /// [`part`] of work that runs parts of its own: `run`'s answer, its time less theirs added to
    /// `part`, so the sum still counts each moment once.
    pub fn around<T>(part: &'static str, run: impl FnOnce() -> T) -> T {
        if !mine() {
            return run();
        }
        let inside = || match &*first() {
            First::Reading { parts, .. } => parts.iter().map(|(_, spent, _)| *spent).sum(),
            First::Waiting | First::Told => std::time::Duration::ZERO,
        };
        let (started, before) = (Instant::now(), inside());
        let answer = run();
        let theirs = inside().saturating_sub(before);
        add_part(part, started.elapsed().saturating_sub(theirs));
        answer
    }

    /// `spent`, measured by the caller (a wall time around work spread over threads), added to
    /// `part` of this process's first read while this thread is reading it.
    pub fn add_part(part: &'static str, spent: std::time::Duration) {
        let mut first = first();
        let First::Reading { by, parts } = &mut *first else {
            return;
        };
        if *by != std::thread::current().id() {
            return;
        }
        match parts.iter_mut().find(|(name, _, _)| *name == part) {
            Some((_, total, count)) => {
                *total += spent;
                *count += 1;
            }
            None => parts.push((part, spent, 1)),
        }
    }

    /// This process's first read is done on this thread, begun at `started`: its parts logged as one
    /// line at [`super::FIRST_READ`], in the order they first ran, each `<part> <ms>` (and `x<count>` where
    /// it ran more than once), beside their sum and the whole read's time, and nothing summed from now
    /// on. Another thread's call, or a second one, logs nothing.
    pub fn done(what: &'static str, started: Instant) {
        let parts = {
            let mut first = first();
            match &mut *first {
                First::Reading { by, parts } if *by == std::thread::current().id() => {
                    let parts = std::mem::take(parts);
                    *first = First::Told;
                    parts
                }
                First::Waiting | First::Reading { .. } | First::Told => return,
            }
        };
        let (line, ms) = line_of(&parts);
        tracing::info!(
            target: "suzatary::first_read",
            what,
            ms,
            whole = started.elapsed().as_millis(),
            "first read: {line}"
        );
    }

    /// `parts` as the first read's line says them, `<part> <ms>` each (and `x<count>` where a part
    /// ran more than once), and their sum in milliseconds.
    pub(super) fn line_of(parts: &Parts) -> (String, u128) {
        let line = parts
            .iter()
            .map(|(part, spent, count)| match count {
                1 => format!("{part} {}", spent.as_millis()),
                _ => format!("{part} {} x{count}", spent.as_millis()),
            })
            .collect::<Vec<_>>()
            .join(", ");
        (
            line,
            parts.iter().map(|(_, spent, _)| spent.as_millis()).sum(),
        )
    }

    fn first() -> std::sync::MutexGuard<'static, First> {
        FIRST
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use std::time::{Duration, Instant};

    /// No first read is ever begun in the browser.
    #[must_use]
    pub fn begins() -> Option<Instant> {
        None
    }

    /// No thread reads a first read in the browser.
    #[must_use]
    pub fn mine() -> bool {
        false
    }

    /// `run`'s answer, untimed.
    pub fn part<T>(_: &'static str, run: impl FnOnce() -> T) -> T {
        run()
    }

    /// `run`'s answer, untimed.
    pub fn around<T>(_: &'static str, run: impl FnOnce() -> T) -> T {
        run()
    }

    /// Nothing.
    pub fn add_part(_: &'static str, _: Duration) {}

    /// Nothing.
    pub fn done(_: &'static str, _: Instant) {}
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    #[test]
    fn a_first_read_s_line_names_each_part_its_ms_and_how_often_and_sums_them() {
        let parts = vec![
            ("objects", Duration::from_millis(900), 1),
            ("keys", Duration::from_millis(40), 3),
            ("open", Duration::from_millis(7), 1),
        ];
        assert_eq!(
            super::native::line_of(&parts),
            ("objects 900, keys 40 x3, open 7".to_owned(), 947)
        );
    }
}
