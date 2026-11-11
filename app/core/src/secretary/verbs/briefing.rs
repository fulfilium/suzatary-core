use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::{Refusal, shown};
use crate::secretary::briefings::BriefingError;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// One saved briefing, as Typst source.
pub struct Briefing;

#[derive(Deserialize, JsonSchema)]
pub struct BriefingArgs {
    day: String,
}

impl Verb for Briefing {
    type Surface = SecretarySurface;
    type Args = BriefingArgs;
    type Answer = Value;
    const NAME: &'static str = "briefing";
    const DESCRIPTION: &'static str = "One SAVED briefing, as typst markup source — the prose that was written \
        for that morning, by day (YYYY-MM-DD). Use it when the owner points at a \
        briefing (`briefing:2026-09-10`) or asks about one. `briefing_delta` is \
        the other one and answers a different question: what MOVED since a day.";

    fn run(call: &Call<'_>, args: BriefingArgs) -> Result<Value, Refusal> {
        let day = args.day.as_str();
        let date = chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| {
            Refusal::bad_request(format!("day must be YYYY-MM-DD, got {}", shown(day)))
        })?;
        let source = call
            .caps
            .briefings
            .read(date)
            .map_err(|failed| match failed {
                BriefingError::Absent(_) => {
                    Refusal::not_found(format!("no briefing is saved for {date}"))
                }
                other @ BriefingError::Unreadable(_) => Refusal::internal(other.to_string()),
            })?;
        Ok(json!({ "day": date.to_string(), "source": source }))
    }
}
