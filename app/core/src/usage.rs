//! What the model subscription's allowance stands at, per window, as the endpoint reports
//! it: what the compactor reads before it spends any of it. Only [`Allowance::Reported`] carries
//! a fraction, so nothing reads a guess as the allowance.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The two windows the limits are measured in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    /// The rolling five-hour block.
    FiveHour,
    /// The rolling seven days.
    SevenDay,
}

impl Window {
    /// The window the limits call `name`, by the one spelling serde derives; `None` for a
    /// limit this box does not count.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name))
            .ok()
    }
}

/// One reading of a window's allowance. The source answers about hourly.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reported {
    /// How much of it is used, in `[0, 1]`.
    pub utilization: f64,
    /// When it resets, if reported.
    #[serde(with = "maybe_stamp")]
    pub resets_at: Option<DateTime<Utc>>,
    /// When this reading was taken.
    #[serde(with = "stamp")]
    pub as_of: DateTime<Utc>,
}

/// What this box knows about one window's allowance. Only
/// [`Allowance::Reported`] carries a fraction.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Allowance {
    /// The endpoint answered.
    Reported(Reported),
    /// The endpoint answers 429; the last good reading rides along.
    RateLimited {
        /// The first 429 after a success, or the first read.
        #[serde(with = "stamp")]
        since: DateTime<Utc>,
        /// The endpoint's own `retry-after`, in seconds.
        retry_after_seconds: u64,
        /// The last successful reading, if this box ever got one.
        last_reported: Option<Reported>,
    },
    /// The endpoint refuses this box's token, which waiting will not change: a 403
    /// `oauth_scope_insufficient` says the setup-token lacks `user:profile`.
    Refused {
        /// The last refusal.
        #[serde(with = "stamp")]
        at: DateTime<Utc>,
        /// Its HTTP status.
        status: u16,
        /// Its error's own words, verbatim and capped: the endpoint's, never mail's.
        words: String,
    },
    /// Never read: no attempt has answered either way yet.
    Unknown,
}

mod stamp {
    use chrono::{DateTime, SecondsFormat, Utc};
    use serde::Serializer;

    pub fn serialize<S: Serializer>(at: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&at.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

mod maybe_stamp {
    use chrono::{DateTime, Utc};
    use serde::Serializer;

    #[expect(clippy::ref_option, reason = "serde's `with` signature")]
    pub fn serialize<S: Serializer>(
        at: &Option<DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => super::stamp::serialize(at, serializer),
            None => serializer.serialize_none(),
        }
    }
}
