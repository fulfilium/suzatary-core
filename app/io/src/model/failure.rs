//! Why a run of the `claude` CLI failed, read once off what it printed. The CLI prints a limit on
//! stdout, as a refused `rate_limit_event` and an error `result`, and often nothing on stderr, so
//! a run is read whole before its exit status is.

use suzatary_core::capability::{Limited, ModelError};
use suzatary_core::health::Instant;

use super::stream::{Event, LimitStatus};

/// What a run printed that says why it failed.
#[derive(Debug, Default)]
pub struct Heard {
    refused: Refused,
    /// The text of the last result the CLI called an error.
    words: Option<String>,
}

/// Whether a window refused one of the run's requests.
#[derive(Debug, Default, Clone, Copy)]
enum Refused {
    #[default]
    No,
    /// It did, and resets at this unix second, when the CLI said.
    Yes { resets: Option<i64> },
}

impl Heard {
    /// Everything `out` printed, line by line.
    #[must_use]
    pub fn of(out: &str) -> Self {
        let mut heard = Self::default();
        for line in out.lines() {
            heard.line(line);
        }
        heard
    }

    /// One more line of the run's stdout.
    pub fn line(&mut self, line: &str) {
        match Event::of(line) {
            Some(Event::RateLimitEvent { rate_limit_info }) => {
                if rate_limit_info.status == LimitStatus::Rejected {
                    self.refused = Refused::Yes {
                        resets: rate_limit_info.resets_at,
                    };
                }
            }
            Some(Event::Result(ended)) => {
                if ended.is_error
                    && let Some(text) = ended.result
                {
                    self.words = Some(text);
                }
            }
            Some(
                Event::System(_)
                | Event::Assistant { .. }
                | Event::StreamEvent { .. }
                | Event::Other,
            )
            | None => {}
        }
    }

    /// Why the run failed at `now`: a refused window first, then the error result's words, then
    /// `otherwise`, the box's own words for how it ended.
    #[must_use]
    pub fn failure(&self, otherwise: String, now: Instant) -> ModelError {
        match self.refused {
            Refused::Yes { resets } => {
                ModelError::Limited(Limited::usage(resets.and_then(unix), now))
            }
            Refused::No => failed(self.words.clone().unwrap_or(otherwise), now),
        }
    }
}

/// The instant of a unix second.
fn unix(seconds: i64) -> Option<Instant> {
    chrono::DateTime::from_timestamp(seconds, 0).map(|at| at.naive_utc())
}

/// The CLI's words for a failure at `now`, read once: a prompt longer than the model reads is
/// [`ModelError::TooLong`], so the round can say which answer was too big; a spent allowance or
/// too many requests is [`ModelError::Limited`], so the box stops asking until it ends; an
/// overloaded service is [`ModelError::Overloaded`]; a refused credential (a 401 or 403, an
/// invalid or expired key or token, a login asked for) is [`ModelError::Refused`], so the box
/// asks for a new one; anything else is [`ModelError::Failed`].
#[must_use]
pub fn failed(words: String, now: Instant) -> ModelError {
    let lower = words.to_lowercase();
    let status = |codes: &[&str]| {
        lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| codes.contains(&token))
    };
    let says = |phrases: &[&str]| phrases.iter().any(|said| lower.contains(said));
    if says(&TOO_LONG) {
        ModelError::TooLong(words)
    } else if says(&SPENT) {
        ModelError::Limited(Limited::usage(reset_of(&lower).and_then(unix), now))
    } else if status(&["429"]) || says(&RATE) {
        ModelError::Limited(Limited::rate(now))
    } else if status(&["529"]) || says(&["overloaded"]) {
        ModelError::Overloaded(words)
    } else if status(&["401", "403"]) || says(&REFUSALS) {
        ModelError::Refused(words)
    } else {
        ModelError::Failed(words)
    }
}

/// The unix second after `usage limit reached|`, as the CLI prints a spent allowance's reset.
fn reset_of(lower: &str) -> Option<i64> {
    let (_, after) = lower.split_once("limit reached|")?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// How the CLI says the account's usage allowance is spent, lowercased.
const SPENT: [&str; 5] = [
    "usage limit reached",
    "hit your limit",
    "limit reached ∙ resets",
    "limit reached · resets",
    "out of extra usage",
];

/// How the API says a request came too fast, lowercased.
const RATE: [&str; 2] = ["rate_limit_error", "too many requests"];

/// How the CLI says it refused the credential, lowercased: the API's error types it passes on
/// and its own lines.
const REFUSALS: [&str; 9] = [
    "authentication_error",
    "permission_error",
    "invalid api key",
    "invalid x-api-key",
    "invalid bearer token",
    "token has expired",
    "please run /login",
    "not logged in",
    "unauthorized",
];

/// How the CLI, the Messages API and an OpenAI-shaped API say a prompt is too long, lowercased.
const TOO_LONG: [&str; 3] = [
    "prompt is too long",
    "context_length_exceeded",
    "maximum context length",
];

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::capability::Limit;

    fn now() -> Instant {
        unix(1_761_800_000).expect("an instant")
    }

    /// Each class of failure the CLI prints reads as its own, whatever numbers its words hold:
    /// only a refused credential is [`ModelError::Refused`], a limit is never a refusal, and an
    /// overload or a timeout is neither.
    #[test]
    fn each_failure_the_cli_prints_reads_as_its_class() {
        let class = |error: &ModelError| match error {
            ModelError::Refused(_) => "refused",
            ModelError::Limited(limited) => match limited.limit {
                Limit::Usage => "usage",
                Limit::UsageUntold => "usage untold",
                Limit::Rate => "rate",
            },
            ModelError::Overloaded(_) => "overloaded",
            ModelError::TooLong(_) => "too long",
            ModelError::Failed(_) => "failed",
            ModelError::Unreachable(_)
            | ModelError::TimedOut(_)
            | ModelError::Lost(_)
            | ModelError::Halted => "other",
        };
        for (words, expected) in [
            (
                "API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\"}}",
                "refused",
            ),
            ("status 403: forbidden", "refused"),
            (
                "OAuth token has expired. Please obtain a new token",
                "refused",
            ),
            ("Invalid API key · Please run /login", "refused"),
            ("Claude AI usage limit reached|1761926400", "usage"),
            ("You've hit your limit · resets 2pm (UTC)", "usage untold"),
            ("5-hour limit reached ∙ resets 2pm", "usage untold"),
            ("API Error: 429 rate_limit_error", "rate"),
            ("API Error: 529 overloaded_error", "overloaded"),
            ("Prompt is too long", "too long"),
            ("timed out after 4010 ms", "failed"),
            ("claude exited exit status: 1: ", "failed"),
        ] {
            assert_eq!(class(&failed(words.to_owned(), now())), expected, "{words}");
        }
        assert_eq!(
            failed("Claude AI usage limit reached|1761926400".to_owned(), now()),
            ModelError::Limited(Limited {
                limit: Limit::Usage,
                until: unix(1_761_926_400).expect("an instant"),
            })
        );
    }

    /// A run that exited 1 with nothing on stderr names its limit from its stdout: the refused
    /// window and its reset win over the result's words and the exit's.
    #[test]
    fn a_refused_window_in_the_stream_is_the_limit_and_its_reset() {
        let out = [
            r#"{"type":"system","subtype":"init","tools":[]}"#,
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":1761926400,"rateLimitType":"seven_day"},"uuid":"00000000-0000-4000-8000-000000000004","session_id":"00000000-0000-4000-8000-000000000001"}"#,
            r#"{"type":"result","subtype":"success","is_error":true,"num_turns":1,"duration_ms":5,"session_id":"00000000-0000-4000-8000-000000000001","result":"You've hit your limit · resets 3pm (UTC)"}"#,
        ]
        .join("\n");
        assert_eq!(
            Heard::of(&out).failure("claude exited exit status: 1: ".to_owned(), now()),
            ModelError::Limited(Limited {
                limit: Limit::Usage,
                until: unix(1_761_926_400).expect("an instant"),
            })
        );
        let allowed = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1761926400}}"#;
        assert_eq!(
            Heard::of(allowed).failure("claude exited exit status: 1: ".to_owned(), now()),
            ModelError::Failed("claude exited exit status: 1: ".to_owned())
        );
    }
}
