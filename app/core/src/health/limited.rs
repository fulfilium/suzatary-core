//! A limit the model's account is under, and the one pause rule every caller of the model obeys:
//! while a limit recorded in the model health stands, the box asks the model nothing.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::health::Failure;
use crate::health::{Health, Healths, Instant, Reading};

/// The model health's entry for the subscription every model the box runs shares: a limit is the
/// account's, so it is recorded here and pauses every model.
pub const SUBSCRIPTION: &str = "claude-cli";

/// How long the box waits when the CLI says the usage allowance is spent and not when it resets.
pub const UNTOLD: chrono::Duration = chrono::Duration::hours(1);

/// How long the box waits after the account is rate-limited: the CLI has already retried.
pub const RATE_WAIT: chrono::Duration = chrono::Duration::minutes(15);

/// A limit, and the instant before which the box asks the model nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "ModelLimited")]
pub struct Limited {
    /// Which limit.
    pub limit: Limit,
    /// When the box asks again, UTC.
    #[ts(type = "string")]
    pub until: Instant,
}

/// Which limit the account is under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "ModelLimit")]
pub enum Limit {
    /// The usage allowance is spent until the reset the CLI said.
    Usage,
    /// The usage allowance is spent and the CLI did not say when it resets, so the box waits
    /// [`UNTOLD`].
    UsageUntold,
    /// The account sent too many requests, so the box waits [`RATE_WAIT`].
    Rate,
}

impl Limited {
    /// The usage allowance spent until `resets`, or for [`UNTOLD`] after `now` when not said.
    #[must_use]
    pub fn usage(resets: Option<Instant>, now: Instant) -> Self {
        match resets {
            Some(until) => Self {
                limit: Limit::Usage,
                until,
            },
            None => Self {
                limit: Limit::UsageUntold,
                until: now + UNTOLD,
            },
        }
    }

    /// The account rate-limited at `now`.
    #[must_use]
    pub fn rate(now: Instant) -> Self {
        Self {
            limit: Limit::Rate,
            until: now + RATE_WAIT,
        }
    }

    /// The limit that stands at `now` in `calls`, the model health: the latest limit recorded
    /// under [`SUBSCRIPTION`], while it has not ended.
    #[must_use]
    pub fn standing(calls: &Healths<String>, now: Instant) -> Option<Self> {
        match calls.reading(SUBSCRIPTION)? {
            Reading::Failing {
                class: Failure::Limited(limited),
                ..
            } => (limited.until > now).then_some(limited),
            Reading::Failing { .. } | Reading::Ok { .. } => None,
        }
    }
}

/// One model's calls as the owner reads them: its own results joined with its subscription's
/// limits, so a limit another job met shows on every model's line until a later call answers.
#[must_use]
pub fn reading_of(calls: &Healths<String>, model: &str) -> Option<Reading> {
    [model, SUBSCRIPTION]
        .into_iter()
        .filter_map(|entry| calls.health(entry))
        .fold(None, |sum: Option<Health>, health| {
            Some(sum.map_or_else(|| health.clone(), |sum| sum.join(health)))
        })
        .map(|health| health.reading())
}

/// The limit in the owner's words, with when the box asks again.
impl std::fmt::Display for Limited {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let until = self.until.format("%Y-%m-%d %H:%M UTC");
        match self.limit {
            Limit::Usage => write!(f, "the model's usage limit is reached until {until}"),
            Limit::UsageUntold => write!(
                f,
                "the model's usage limit is reached, and the CLI did not say until when, so the \
                 box asks it again after {until}"
            ),
            Limit::Rate => write!(
                f,
                "the model's account is rate-limited, so the box asks it again after {until}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Outcome;

    fn at(minute: i64) -> Instant {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 30)
            .and_then(|day| day.and_hms_opt(12, 0, 0))
            .expect("a real instant")
            + chrono::Duration::minutes(minute)
    }

    /// A limit pauses until it ends, and the latest one recorded stands; a model's line shows it
    /// until a call of that model answers after it.
    #[test]
    fn a_limit_stands_until_it_ends_and_shows_until_a_later_answer() {
        let limited = |until| {
            Outcome::Failed(Failure::Limited(Limited {
                limit: Limit::Usage,
                until: at(until),
            }))
        };
        let calls = |results: &[(&str, i64, Outcome)]| {
            results
                .iter()
                .map(|(entry, minute, outcome)| Healths::one(*entry, at(*minute), *outcome, "said"))
                .fold(Healths::default(), |sum, one| sum.join(&one))
        };
        let s = SUBSCRIPTION;
        for (results, now, standing) in [
            (vec![(s, 0, limited(60))], 30, Some(60)),
            (vec![(s, 0, limited(60))], 60, None),
            (
                vec![(s, 0, limited(60)), (s, 10, limited(20))],
                15,
                Some(20),
            ),
            (
                vec![(s, 0, limited(60)), ("a", 10, Outcome::Succeeded)],
                30,
                Some(60),
            ),
            (
                vec![("a", 0, Outcome::Failed(Failure::KeyRefused))],
                30,
                None,
            ),
        ] {
            assert_eq!(
                Limited::standing(&calls(&results), at(now)).map(|limited| limited.until),
                standing.map(at),
                "{results:?} at {now}"
            );
        }
        let failing = |reading: Option<Reading>| matches!(reading, Some(Reading::Failing { .. }));
        let before = calls(&[("a", 0, Outcome::Succeeded), (s, 5, limited(60))]);
        assert!(
            failing(reading_of(&before, "a")),
            "the limit shows on a's line"
        );
        assert!(!failing(before.reading("a")), "and is not a's own result");
        let after = before.join(&Healths::one("a", at(61), Outcome::Succeeded, ""));
        assert!(
            !failing(reading_of(&after, "a")),
            "a later answer clears it"
        );
    }
}
