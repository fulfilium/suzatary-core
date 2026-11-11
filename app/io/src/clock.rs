//! The system clock, as an interpreter of [`Clock`].
//!
//! Always UTC whatever the machine's zone, spelt naive because the Prodrome's literal has no
//! zone. A wrong clock is reported elsewhere, never clamped here.

use suzatary_base::capability::{Clock, Datetime};

/// The box's own wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Datetime {
        // `literal::Datetime` admits years 1..=9999, which a set system clock cannot leave.
        prodrome_core::fpl::datetime_of(chrono::Utc::now().naive_utc())
            .expect("the system clock reads a year in 1..=9999")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The machine's zone never enters: the clock reads UTC on a box set to any zone.
    #[test]
    fn the_system_clock_reads_utc() {
        let utc = prodrome_core::fpl::datetime_of(chrono::Utc::now().naive_utc())
            .expect("a year in range");
        let read = SystemClock.now();
        let apart = prodrome_core::fpl::instant_of(read) - prodrome_core::fpl::instant_of(utc);
        assert!(
            apart.num_seconds().abs() < 60,
            "{read:?} is not UTC's {utc:?}"
        );
    }
}
