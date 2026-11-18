//! What the compactor may spend: a cap on calls an hour and on bytes sent a day, read against
//! the attempts the digest's history records, so the budget is a fold over the digest's own
//! objects and a restart forgets nothing it spent.
//!
//! Both windows are rolling and end at the instant asked about. A call is made only when what
//! the windows spent, joined with what the call itself will spend, is within both caps, so no
//! call crosses a cap by its own size: the caps are a down-set of the [`Spent`] monoid.

use prodrome_core::fpl::{Instant, instant_of};
use prodrome_core::literal::Datetime;

use crate::secretary::digest_spent::Spent;

/// The caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Calls to the model in any hour.
    pub calls_per_hour: u32,
    /// Bytes sent to it in any twenty-four hours.
    pub bytes_per_day: u64,
}

/// Why no attempt may start now, and when one may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// The last hour's calls reached the cap.
    Calls {
        /// The calls made in it.
        spent: u32,
        /// When enough of them are an hour old.
        until: Instant,
    },
    /// The last day's bytes and the call's own would cross the cap.
    Bytes {
        /// The bytes sent in it.
        spent: u64,
        /// The bytes the call would send.
        next: u64,
        /// When enough of them are a day old.
        until: Instant,
    },
}

impl Hold {
    /// When an attempt may start again.
    #[must_use]
    pub fn until(self) -> Instant {
        match self {
            Hold::Calls { until, .. } | Hold::Bytes { until, .. } => until,
        }
    }
}

impl std::fmt::Display for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Hold::Calls { spent, until } => write!(
                f,
                "the hourly cap holds the compactor: {spent} calls in the last hour, until {}",
                until.format("%Y-%m-%dT%H:%M:%S")
            ),
            Hold::Bytes { spent, next, until } => write!(
                f,
                "the daily byte budget holds the compactor: {spent} bytes sent in the last day \
                 and {next} to send, until {}",
                until.format("%Y-%m-%dT%H:%M:%S")
            ),
        }
    }
}

impl Budget {
    /// Whether a call that spends `next` could ever be made: within both caps on its own. One
    /// that is not would wait forever, so the rule records it unfit instead of holding it.
    #[must_use]
    pub fn admits(&self, next: Spent) -> bool {
        next.calls <= self.calls_per_hour && next.sent <= self.bytes_per_day
    }

    /// The hold on a call that will spend `next`, at `now`, given every call the history
    /// records; `None` when it may be made. The hourly cap is asked first.
    #[must_use]
    pub fn hold(&self, attempts: &[(Datetime, Spent)], next: Spent, now: Instant) -> Option<Hold> {
        let hour = chrono::Duration::hours(1);
        let day = chrono::Duration::days(1);
        let calls = freed(attempts, now, hour, |spent| {
            u64::from(spent.join(next).calls) > u64::from(self.calls_per_hour)
        });
        let bytes = freed(attempts, now, day, |spent| {
            spent.join(next).sent > self.bytes_per_day
        });
        calls
            .map(|(spent, until)| Hold::Calls {
                spent: spent.calls,
                until,
            })
            .or_else(|| {
                bytes.map(|(spent, until)| Hold::Bytes {
                    spent: spent.sent,
                    next: next.sent,
                    until,
                })
            })
    }
}

/// What the attempts in the window of `width` ending at `now` spent, and the instant they no
/// longer hold the cap, when `over` says they do now: the oldest are dropped until the rest are
/// under it, and the last dropped leaves the window at its instant plus `width`. A call over
/// the cap on its own is held a whole window, and asked about again then.
fn freed(
    attempts: &[(Datetime, Spent)],
    now: Instant,
    width: chrono::Duration,
    over: impl Fn(Spent) -> bool,
) -> Option<(Spent, Instant)> {
    let mut window: Vec<(Instant, Spent)> = attempts
        .iter()
        .map(|(at, spent)| (instant_of(*at), *spent))
        .filter(|(at, _)| *at > now - width && *at <= now)
        .collect();
    let spent: Spent = window.iter().map(|(_, spent)| *spent).sum();
    if !over(spent) {
        return None;
    }
    window.sort_by_key(|(at, _)| *at);
    let mut rest = spent;
    for (at, dropped) in window {
        rest = Spent {
            calls: rest.calls.saturating_sub(dropped.calls),
            sent: rest.sent.saturating_sub(dropped.sent),
        };
        if !over(rest) {
            return Some((spent, at + width));
        }
    }
    Some((spent, now + width))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn at(minute: i64) -> Instant {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 6)
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .expect("an instant")
            + chrono::Duration::minutes(minute)
    }

    fn attempt(minute: i64, calls: u32, sent: u64) -> (Datetime, Spent) {
        (
            prodrome_core::fpl::datetime_of(at(minute)).expect("a datetime"),
            Spent { calls, sent },
        )
    }

    /// The smallest call, which the caps hold exactly when the window has reached them.
    const CALL: Spent = Spent { calls: 1, sent: 1 };

    const BUDGET: Budget = Budget {
        calls_per_hour: 6,
        bytes_per_day: 1_000,
    };

    proptest! {
        /// The hold is a function of the set of attempts: their order in the history moves
        /// nothing, and at its `until` the attempts made by then hold no more on its account.
        /// A call may be made exactly when the window's spending joined with its own is within
        /// both caps.
        #[test]
        fn a_call_is_made_exactly_when_it_and_the_window_fit_the_caps(
            attempts in proptest::collection::vec((0i64..3_000, 1u32..6, 0u64..400), 0..40),
            now in 0i64..3_000, next in 0u64..600,
        ) {
            let attempts: Vec<_> = attempts.into_iter().map(|(m, c, s)| attempt(m, c, s)).collect();
            let within = |width: i64| -> Spent {
                attempts
                    .iter()
                    .filter(|(at_, _)| {
                        let at_ = instant_of(*at_);
                        at_ > at(now) - chrono::Duration::minutes(width) && at_ <= at(now)
                    })
                    .map(|(_, spent)| *spent)
                    .sum()
            };
            let next = Spent::call(next);
            let fits = within(60).join(next).calls <= BUDGET.calls_per_hour
                && within(24 * 60).join(next).sent <= BUDGET.bytes_per_day;
            prop_assert_eq!(BUDGET.hold(&attempts, next, at(now)).is_none(), fits);
        }

        #[test]
        fn the_hold_is_a_function_of_the_set_and_frees_at_its_instant(
            attempts in proptest::collection::vec((0i64..3_000, 1u32..6, 0u64..400), 0..40),
            now in 0i64..3_000, seed in any::<usize>(),
        ) {
            let attempts: Vec<_> = attempts
                .into_iter()
                .filter(|(minute, _, _)| *minute <= now)
                .map(|(m, c, s)| attempt(m, c, s))
                .collect();
            let mut shuffled = attempts.clone();
            shuffled.rotate_left(seed % attempts.len().max(1));
            shuffled.reverse();
            let hold = BUDGET.hold(&attempts, CALL, at(now));
            prop_assert_eq!(hold, BUDGET.hold(&shuffled, CALL, at(now)));
            match hold {
                Some(Hold::Calls { until, .. }) => {
                    prop_assert!(until > at(now));
                    let later = BUDGET.hold(&attempts, CALL, until);
                    prop_assert!(!matches!(later, Some(Hold::Calls { .. })), "{:?}", later);
                }
                Some(Hold::Bytes { until, .. }) => {
                    prop_assert!(until > at(now));
                    let later = BUDGET.hold(&attempts, CALL, until);
                    prop_assert!(!matches!(later, Some(Hold::Bytes { .. })), "{:?}", later);
                }
                None => {}
            }
        }
    }
}
