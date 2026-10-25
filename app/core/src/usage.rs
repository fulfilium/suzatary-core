//! What this box has spent of its model subscription's allowance: a fold over parsed
//! records, deciding and reading nothing. Quota is the constraint up to the
//! allowance and credits bill past it, so the two are never blurred and no
//! price is invented. Where the allowance is unknown the report gives a
//! measured floor, never a percentage of a guess.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One request, as the transcripts record it and this module reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// When it was made.
    pub at: DateTime<Utc>,
    /// The model's own id, verbatim.
    pub model: String,
    /// Who made it, as far as the reader can honestly tell.
    pub who: Who,
    /// Its tokens.
    pub tokens: Tokens,
}

/// Who spent it, as far as a reader that takes no text can tell: triage and
/// the secretary share a transcript directory, so they are one arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Who {
    /// The interactive assistant session.
    Session,
    /// The triage timer and the secretary, together.
    Automated,
    /// A transcript directory the reader could not place.
    Other,
}

/// A request's four token counts, kept apart because each is priced differently.
/// `number` in TypeScript, not ts-rs's `bigint`: `JSON.parse` never makes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, TS)]
#[ts(rename = "UsageTokens")]
pub struct Tokens {
    /// Input the model read for the first time.
    #[ts(type = "number")]
    pub fresh: u64,
    /// Input written into the prompt cache — paid at a premium.
    #[ts(type = "number")]
    pub cache_write: u64,
    /// Input served from the prompt cache — paid at a fraction.
    #[ts(type = "number")]
    pub cache_read: u64,
    /// Output.
    #[ts(type = "number")]
    pub output: u64,
}

impl Tokens {
    /// Everything the request read, cached or not.
    #[must_use]
    pub fn input(self) -> u64 {
        self.fresh + self.cache_write + self.cache_read
    }

    /// Everything, both directions.
    #[must_use]
    pub fn total(self) -> u64 {
        self.input() + self.output
    }

    /// How much of the input was served from cache, in `[0, 1]`; `None` when
    /// there was no input at all, because a rate over nothing is not zero.
    #[must_use]
    pub fn cache_hit(self) -> Option<f64> {
        let input = self.input();
        if input == 0 {
            None
        } else {
            // Both fit a mantissa many times over at any plausible scale.
            #[allow(clippy::cast_precision_loss)]
            Some(self.cache_read as f64 / input as f64)
        }
    }

    fn add(self, other: Tokens) -> Tokens {
        Tokens {
            fresh: self.fresh + other.fresh,
            cache_write: self.cache_write + other.cache_write,
            cache_read: self.cache_read + other.cache_read,
            output: self.output + other.output,
        }
    }
}

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
    /// How long it is.
    #[must_use]
    pub fn length(self) -> Duration {
        match self {
            Window::FiveHour => Duration::hours(5),
            Window::SevenDay => Duration::days(7),
        }
    }

    /// The window the limits call `name`, by the one spelling serde derives; `None` for a
    /// limit this box does not count.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::deserialize(serde::de::value::StrDeserializer::<serde::de::value::Error>::new(name))
            .ok()
    }
}

/// A request the limit refused, with what the limit said about itself; the
/// one place the transcripts speak about the allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UsageRefusal")]
pub struct Refused {
    /// When.
    #[serde(with = "stamp")]
    #[ts(type = "string")]
    pub at: DateTime<Utc>,
    /// Which window refused; the report already files it under that window.
    #[serde(skip)]
    #[ts(skip)]
    pub window: Window,
    /// When that window resets, if the limit said.
    #[serde(with = "maybe_stamp")]
    #[ts(type = "string | null")]
    pub resets_at: Option<DateTime<Utc>>,
    /// The credit situation at that moment.
    #[serde(flatten)]
    #[ts(flatten)]
    pub overage: Overage,
}

/// Whether crossing the allowance bills, from the refusal's own metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Overage {
    /// Past the allowance, requests bill as credits rather than refuse.
    #[serde(rename = "overage_enabled")]
    #[ts(rename = "overage_enabled")]
    pub enabled: bool,
    /// This request was itself on credits.
    #[serde(rename = "overage_using")]
    #[ts(rename = "overage_using")]
    pub using: bool,
    /// The limit's own reason when it is not enabled, verbatim, or empty.
    #[serde(rename = "overage_reason")]
    #[ts(rename = "overage_reason")]
    pub disabled_reason: String,
}

/// One reading of a window's allowance. The source answers about hourly, so
/// the page says how old it is.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "UsageReported")]
pub struct Reported {
    /// How much of it is used, in `[0, 1]`.
    pub utilization: f64,
    /// When it resets, if reported.
    #[serde(with = "maybe_stamp")]
    #[ts(type = "string | null")]
    pub resets_at: Option<DateTime<Utc>>,
    /// When this reading was taken.
    #[serde(with = "stamp")]
    #[ts(type = "string")]
    pub as_of: DateTime<Utc>,
}

/// What this box knows about one window's allowance. Only
/// [`Allowance::Reported`] carries a fraction, so a page cannot print a
/// percentage of the measured floor as if it were the allowance.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "UsageAllowance")]
pub enum Allowance {
    /// The endpoint answered.
    Reported(Reported),
    /// The endpoint answers 429; the last good reading rides along.
    RateLimited {
        /// The first 429 after a success, or the first read.
        #[serde(with = "stamp")]
        #[ts(type = "string")]
        since: DateTime<Utc>,
        /// The endpoint's own `retry-after`, in seconds.
        #[ts(type = "number")]
        retry_after_seconds: u64,
        /// The last successful reading, if this box ever got one.
        last_reported: Option<Reported>,
    },
    /// The endpoint refuses this box's token, which waiting will not change: a 403
    /// `oauth_scope_insufficient` says the setup-token lacks `user:profile`.
    Refused {
        /// The last refusal.
        #[serde(with = "stamp")]
        #[ts(type = "string")]
        at: DateTime<Utc>,
        /// Its HTTP status.
        status: u16,
        /// Its error's own words, verbatim and capped: the endpoint's, never mail's.
        words: String,
    },
    /// Never read: no attempt has answered either way yet.
    Unknown,
}

/// Which project directories this report read and which it found but did
/// not: the allowance is the whole account's, the totals only this box's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, TS)]
#[ts(rename = "UsageScope")]
pub struct Scope {
    /// Directory slugs that contributed a request or a refusal.
    pub counted: Vec<String>,
    /// Directory slugs found but older than the reader's lookback.
    pub not_counted: Vec<String>,
}

/// One model's share of a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UsageModelRow")]
pub struct ModelShare {
    /// The model id, verbatim.
    pub model: String,
    /// Its tokens.
    pub tokens: Tokens,
}

/// One author's share of a window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UsageWhoRow")]
pub struct WhoShare {
    /// Who.
    #[ts(type = "string")]
    pub who: Who,
    /// Their tokens.
    pub tokens: Tokens,
}

/// One window's report.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "UsageWindow")]
pub struct WindowReport {
    /// Which window this is: `five_hour` or `seven_day`, the limits' own names.
    #[ts(type = "string")]
    pub window: Window,
    /// The window's start, `now - length`.
    #[serde(with = "stamp")]
    #[ts(type = "string")]
    pub since: DateTime<Utc>,
    /// Requests inside it.
    pub requests: usize,
    /// Their tokens, summed.
    pub tokens: Tokens,
    /// By model, largest total first.
    pub by_model: Vec<ModelShare>,
    /// By who, in `Who`'s order.
    pub by_who: Vec<WhoShare>,
    /// Reads over all input, `[0, 1]`; `None` over no input.
    pub cache_hit: Option<f64>,
    /// The last refusal this window produced, ever, not only inside it.
    pub last_refusal: Option<Refused>,
    /// What credits are charged on: spent after the last refusal while overage
    /// was enabled. `None`, not zero, without such a refusal.
    pub on_credits: Option<Tokens>,
    /// Spent in one window-length before the last refusal: a floor of the
    /// limit, never divided into.
    pub most_seen_before_refusal: Option<Tokens>,
    /// What this box knows about the allowance.
    pub allowance: Allowance,
    /// [`Who::Other`]'s share, named beside the allowance bar; zero, not
    /// absent, when nothing else spent.
    pub elsewhere: Tokens,
}

/// The whole report: `GET /api/usage`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "UsagePayload")]
pub struct Report {
    /// The instant it was folded at.
    #[serde(with = "stamp")]
    #[ts(type = "string")]
    pub at: DateTime<Utc>,
    /// The five-hour window.
    pub five_hour: WindowReport,
    /// The seven-day window.
    pub seven_day: WindowReport,
    /// Everything the reader saw, summed.
    pub seen: Tokens,
    /// How many requests the reader saw.
    pub seen_requests: usize,
    /// The reader's horizon, so "seen" is never read as all time.
    #[serde(with = "maybe_stamp")]
    #[ts(type = "string | null")]
    pub seen_since: Option<DateTime<Utc>>,
    /// Which directories it read.
    pub scope: Scope,
}

/// Fold the records into the report. Records come in any order; of two
/// allowances for one window the later wins, and none reads
/// [`Allowance::Unknown`].
#[must_use]
pub fn report(
    at: DateTime<Utc>,
    requests: &[Request],
    refusals: &[Refused],
    allowances: &[(Window, Allowance)],
    horizon: Option<DateTime<Utc>>,
    scope: Scope,
) -> Report {
    let seen = requests
        .iter()
        .fold(Tokens::default(), |sum, request| sum.add(request.tokens));
    Report {
        at,
        five_hour: window(Window::FiveHour, at, requests, refusals, allowances),
        seven_day: window(Window::SevenDay, at, requests, refusals, allowances),
        seen,
        seen_requests: requests.len(),
        seen_since: horizon,
        scope,
    }
}

fn window(
    which: Window,
    at: DateTime<Utc>,
    requests: &[Request],
    refusals: &[Refused],
    allowances: &[(Window, Allowance)],
) -> WindowReport {
    let since = at - which.length();
    let inside: Vec<&Request> = requests
        .iter()
        .filter(|request| request.at > since && request.at <= at)
        .collect();

    let tokens = inside
        .iter()
        .fold(Tokens::default(), |sum, request| sum.add(request.tokens));

    let mut by_model: BTreeMap<&str, Tokens> = BTreeMap::new();
    let mut by_who: BTreeMap<Who, Tokens> = BTreeMap::new();
    for request in &inside {
        let model = by_model.entry(request.model.as_str()).or_default();
        *model = model.add(request.tokens);
        let who = by_who.entry(request.who).or_default();
        *who = who.add(request.tokens);
    }
    let mut by_model: Vec<ModelShare> = by_model
        .into_iter()
        .map(|(model, tokens)| ModelShare {
            model: model.to_owned(),
            tokens,
        })
        .collect();
    // Largest first, then by name so equal totals are stable.
    by_model.sort_by(|left, right| {
        right
            .tokens
            .total()
            .cmp(&left.tokens.total())
            .then_with(|| left.model.cmp(&right.model))
    });
    let elsewhere = by_who.get(&Who::Other).copied().unwrap_or_default();
    let by_who: Vec<WhoShare> = by_who
        .into_iter()
        .map(|(who, tokens)| WhoShare { who, tokens })
        .collect();

    let last_refusal = refusals
        .iter()
        .filter(|refused| refused.window == which)
        .max_by_key(|refused| refused.at)
        .cloned();

    let on_credits = last_refusal.as_ref().and_then(|refused| {
        if !refused.overage.enabled {
            return None;
        }
        Some(
            requests
                .iter()
                .filter(|request| request.at > refused.at && request.at <= at)
                .fold(Tokens::default(), |sum, request| sum.add(request.tokens)),
        )
    });

    let most_seen_before_refusal = last_refusal.as_ref().map(|refused| {
        let from = refused.at - which.length();
        requests
            .iter()
            .filter(|request| request.at > from && request.at <= refused.at)
            .fold(Tokens::default(), |sum, request| sum.add(request.tokens))
    });

    let allowance = allowances
        .iter()
        .rev()
        .find(|(window, _)| *window == which)
        .map_or(Allowance::Unknown, |(_, allowance)| allowance.clone());

    WindowReport {
        window: which,
        since,
        requests: inside.len(),
        tokens,
        by_model,
        by_who,
        cache_hit: tokens.cache_hit(),
        last_refusal,
        on_credits,
        most_seen_before_refusal,
        allowance,
        elsewhere,
    }
}

/// RFC 3339 to the second in UTC with a `Z`.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours_ago: i64) -> DateTime<Utc> {
        now() - Duration::hours(hours_ago)
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-12T12:00:00Z")
            .expect("a real instant")
            .with_timezone(&Utc)
    }

    fn request(hours_ago: i64, model: &str, who: Who, tokens: Tokens) -> Request {
        Request {
            at: at(hours_ago),
            model: model.to_owned(),
            who,
            tokens,
        }
    }

    fn some(fresh: u64, cache_write: u64, cache_read: u64, output: u64) -> Tokens {
        Tokens {
            fresh,
            cache_write,
            cache_read,
            output,
        }
    }

    /// The report is `/api/usage`'s body: instants to the second with a `Z`,
    /// shares as rows, a refusal's overage flattened beside it.
    #[test]
    fn the_report_serialises_as_the_usage_payload() {
        let requests = [
            request(1, "opus", Who::Session, some(10, 0, 30, 5)),
            request(30, "opus", Who::Automated, some(1, 0, 0, 1)),
        ];
        let refusals = [Refused {
            at: at(2) + Duration::milliseconds(250),
            window: Window::FiveHour,
            resets_at: Some(at(-3)),
            overage: Overage {
                enabled: true,
                using: false,
                disabled_reason: String::new(),
            },
        }];
        let reported = Reported {
            utilization: 0.5,
            resets_at: None,
            as_of: at(1),
        };
        let allowances = [
            (Window::FiveHour, Allowance::Reported(reported.clone())),
            (
                Window::SevenDay,
                Allowance::RateLimited {
                    since: at(4),
                    retry_after_seconds: 60,
                    last_reported: Some(reported),
                },
            ),
        ];
        let report = report(
            now(),
            &requests,
            &refusals,
            &allowances,
            Some(at(48)),
            Scope::default(),
        );
        let json = serde_json::to_value(&report).expect("serialises");
        let tokens = |fresh: u64, cache_read: u64, output: u64| serde_json::json!({"fresh": fresh, "cache_write": 0, "cache_read": cache_read, "output": output});
        let refusal = serde_json::json!({
            "at": "2026-09-12T10:00:00Z", "resets_at": "2026-09-12T15:00:00Z",
            "overage_enabled": true, "overage_using": false, "overage_reason": "",
        });
        let reported = serde_json::json!({"utilization": 0.5, "resets_at": null, "as_of": "2026-09-12T11:00:00Z"});
        assert_eq!(
            json,
            serde_json::json!({
                "at": "2026-09-12T12:00:00Z",
                "five_hour": {
                    "window": "five_hour", "since": "2026-09-12T07:00:00Z", "requests": 1,
                    "tokens": tokens(10, 30, 5),
                    "by_model": [{"model": "opus", "tokens": tokens(10, 30, 5)}],
                    "by_who": [{"who": "session", "tokens": tokens(10, 30, 5)}],
                    "cache_hit": 0.75, "last_refusal": refusal,
                    "on_credits": tokens(10, 30, 5),
                    "most_seen_before_refusal": tokens(0, 0, 0),
                    "allowance": {"kind": "reported", "utilization": 0.5, "resets_at": null, "as_of": "2026-09-12T11:00:00Z"},
                    "elsewhere": tokens(0, 0, 0),
                },
                "seven_day": {
                    "window": "seven_day", "since": "2026-09-05T12:00:00Z", "requests": 2,
                    "tokens": tokens(11, 30, 6),
                    "by_model": [{"model": "opus", "tokens": tokens(11, 30, 6)}],
                    "by_who": [
                        {"who": "session", "tokens": tokens(10, 30, 5)},
                        {"who": "automated", "tokens": tokens(1, 0, 1)},
                    ],
                    "cache_hit": 30.0 / 41.0, "last_refusal": null, "on_credits": null,
                    "most_seen_before_refusal": null,
                    "allowance": {"kind": "rate_limited", "since": "2026-09-12T08:00:00Z", "retry_after_seconds": 60, "last_reported": reported},
                    "elsewhere": tokens(0, 0, 0),
                },
                "seen": tokens(11, 30, 6),
                "seen_requests": 2,
                "seen_since": "2026-09-10T12:00:00Z",
                "scope": {"counted": [], "not_counted": []},
            })
        );
    }
}
