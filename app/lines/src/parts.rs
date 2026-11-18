//! Where a piece of work spends its time, part by part: the lines that split a server's start, a
//! process's first read and a skeleton piece.
//!
//! A scope is opened on a thread around the work ([`scoped`]); every [`part`] run on that thread
//! while it is open is summed into it and into every scope enclosing it. A part's time excludes
//! the parts run inside it, so a scope's parts sum to no more than its wall time. Outside every
//! scope a part reads no clock, and a part on another thread adds nothing.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Each part of a scope: its name, its time summed, and how many times it ran, in the order each
/// first ran.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts(Vec<(&'static str, Duration, usize)>);

impl Parts {
    /// `spent` more of `part`.
    pub fn add(&mut self, part: &'static str, spent: Duration) {
        match self.0.iter_mut().find(|(name, _, _)| *name == part) {
            Some((_, total, count)) => {
                *total += spent;
                *count += 1;
            }
            None => self.0.push((part, spent, 1)),
        }
    }

    #[must_use]
    pub fn sum(&self) -> Duration {
        self.0.iter().map(|(_, spent, _)| *spent).sum()
    }

    /// `<within><part> <ms>` each (and `x<count>` where a part ran more than once), joined by
    /// commas; empty for no part.
    #[must_use]
    pub fn line(&self, within: &str) -> String {
        self.0
            .iter()
            .map(|(part, spent, count)| match count {
                1 => format!("{within}{part} {}", spent.as_millis()),
                _ => format!("{within}{part} {} x{count}", spent.as_millis()),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

std::thread_local! {
    /// The scopes open on this thread, the innermost last.
    static OPEN: RefCell<Vec<Parts>> = const { RefCell::new(Vec::new()) };
}

/// Whether a scope is open on this thread.
#[must_use]
pub fn open() -> bool {
    OPEN.with_borrow(|open| !open.is_empty())
}

/// `run`'s answer and the parts run on this thread while it ran. The browser's module opens no
/// scope: `Instant::now` panics on `wasm32-unknown-unknown`, and a part outside every scope never
/// calls it.
pub fn scoped<T>(run: impl FnOnce() -> T) -> (T, Parts) {
    /// Closes the scope and every scope opened inside it, also when `run` panics, so a pooled
    /// thread keeps none open.
    struct Closing(usize);
    impl Drop for Closing {
        fn drop(&mut self) {
            OPEN.with_borrow_mut(|open| open.truncate(self.0));
        }
    }
    if cfg!(target_arch = "wasm32") {
        return (run(), Parts::default());
    }
    let closing = Closing(OPEN.with_borrow_mut(|open| {
        open.push(Parts::default());
        open.len() - 1
    }));
    let answer = run();
    // Only `closing` truncates the stack below the scope it opened, so it is still there.
    let parts = OPEN.with_borrow_mut(|open| std::mem::take(&mut open[closing.0]));
    (answer, parts)
}

/// `run`'s answer, its time less the parts run inside it added to `part` of every open scope.
pub fn part<T>(part: &'static str, run: impl FnOnce() -> T) -> T {
    if !open() {
        return run();
    }
    let inside = || OPEN.with_borrow(|open| open.last().map_or(Duration::ZERO, Parts::sum));
    let (started, before) = (Instant::now(), inside());
    let answer = run();
    let theirs = inside().saturating_sub(before);
    add_part(part, started.elapsed().saturating_sub(theirs));
    answer
}

/// `spent`, a wall time the caller measured around work spread over threads, added to `part` of
/// every open scope.
pub fn add_part(part: &'static str, spent: Duration) {
    OPEN.with_borrow_mut(|open| {
        for scope in open {
            scope.add(part, spent);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_names_each_part_its_ms_and_how_often() {
        let mut parts = Parts::default();
        parts.add("objects", Duration::from_millis(900));
        parts.add("keys", Duration::from_millis(10));
        parts.add("open", Duration::from_millis(7));
        parts.add("keys", Duration::from_millis(30));
        assert_eq!(parts.line(""), "objects 900, keys 40 x2, open 7");
        assert_eq!(
            parts.line("notice."),
            "notice.objects 900, notice.keys 40 x2, notice.open 7"
        );
        assert_eq!(parts.sum(), Duration::from_millis(947));
        assert_eq!(Parts::default().line(""), "");
    }

    /// The law the start's line rests on: nested parts count each moment once, so a scope's parts
    /// sum to no more than its wall time, an enclosing part keeps only its own time, and an inner
    /// scope's parts are its encloser's too.
    #[test]
    fn nested_parts_count_each_moment_once() {
        let nap = Duration::from_millis(20);
        let started = std::time::Instant::now();
        let ((), outer) = scoped(|| {
            part("around", || {
                std::thread::sleep(nap);
                let ((), inner) = scoped(|| part("leaf", || std::thread::sleep(nap)));
                assert_eq!(
                    inner.0.iter().map(|(name, _, _)| *name).collect::<Vec<_>>(),
                    ["leaf"]
                );
            });
        });
        let wall = started.elapsed();
        let spent = |name| {
            outer
                .0
                .iter()
                .find(|(part, _, _)| *part == name)
                .map(|(_, spent, _)| *spent)
        };
        assert!(spent("leaf").is_some_and(|leaf| leaf >= nap));
        assert!(spent("around").is_some_and(|own| own >= nap));
        assert!(outer.sum() <= wall, "{:?} > {wall:?}", outer.sum());
        assert!(!open(), "every scope closed");
        part("outside", || ());
        assert!(!open());
    }
}
