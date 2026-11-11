//! A decay toward a date.

use chrono::{Duration, NaiveDate};
use prodrome_core::fpl;
use prodrome_core::term::Term;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::todo::price::ratio;
use suzatary_base::refusal::Refusal;
use suzatary_base::zone::Zone;

/// The inputs to a decaying price. The lead-up is a span, whichever unit its writer spoke.
#[derive(Debug, Clone, PartialEq, Eq, TS)]
#[ts(as = "StoredDeadline")]
pub struct Deadline {
    /// The owner's day it is due: reached at that day's end in the owner's zone.
    pub end_date: NaiveDate,
    /// The fulfillment at the start of the lead-up.
    pub start_percent: u32,
    /// The fulfillment at the deadline.
    pub end_percent: u32,
    /// How long the decay runs. `None` is a week.
    pub lead_up: Option<Duration>,
    /// The owner's day the decay may begin at the start of. `None` lets the lead-up place it.
    pub start_date: Option<NaiveDate>,
}

/// A deadline as it is stored and sent: the lead-up in weeks, as every stored act has it, or in
/// days where a span is not whole weeks, omitted otherwise so no stored byte moves.
#[derive(Serialize, Deserialize, TS)]
#[ts(rename = "Deadline")]
pub(crate) struct StoredDeadline {
    /// `YYYY-MM-DD`.
    #[ts(type = "string")]
    pub end_date: NaiveDate,
    /// The fulfillment at the start of the lead-up.
    pub start_percent: u32,
    /// The fulfillment at the deadline.
    pub end_percent: u32,
    /// The lead-up in whole weeks.
    pub(crate) lead_up_weeks: Option<u32>,
    /// The lead-up in days, where it is not whole weeks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub(crate) lead_up_days: Option<u32>,
    /// `YYYY-MM-DD`, the earliest the decay may begin.
    #[ts(type = "string | null")]
    pub start_date: Option<NaiveDate>,
}

impl Deadline {
    pub(crate) fn term(&self, zone: Zone) -> Result<Term, Refusal> {
        if let Some(lead) = self.lead_up
            && !(Duration::days(1)..=Duration::weeks(52)).contains(&lead)
        {
            return Err(Refusal::bad_request(
                "lead_up must be between a day and 52 weeks",
            ));
        }
        fpl::mk_decay(
            ratio(self.start_percent, "start_priority")?,
            ratio(self.end_percent, "end_priority")?,
            zone.end(self.end_date),
            self.lead_up.unwrap_or_else(|| Duration::weeks(1)),
            self.start_date.map(|day| zone.start(day)),
        )
        .map_err(|e| Refusal::bad_request(e.to_string()))
    }
}

impl Serialize for Deadline {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StoredDeadline::from(self.clone()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Deadline {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Deadline::try_from(StoredDeadline::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

impl TryFrom<StoredDeadline> for Deadline {
    type Error = &'static str;
    fn try_from(stored: StoredDeadline) -> Result<Self, &'static str> {
        let lead_up = match (stored.lead_up_weeks, stored.lead_up_days) {
            (Some(_), Some(_)) => return Err("a lead-up is lead_up_weeks or lead_up_days"),
            (Some(weeks), None) => Some(Duration::weeks(i64::from(weeks))),
            (None, Some(days)) => Some(Duration::days(i64::from(days))),
            (None, None) => None,
        };
        Ok(Deadline {
            end_date: stored.end_date,
            start_percent: stored.start_percent,
            end_percent: stored.end_percent,
            lead_up,
            start_date: stored.start_date,
        })
    }
}

impl From<Deadline> for StoredDeadline {
    fn from(due: Deadline) -> StoredDeadline {
        let whole = |span: i64| u32::try_from(span.max(0)).unwrap_or(u32::MAX);
        let (lead_up_weeks, lead_up_days) = match due.lead_up {
            None => (None, None),
            Some(span) if span == Duration::weeks(span.num_weeks()) => {
                (Some(whole(span.num_weeks())), None)
            }
            Some(span) => (None, Some(whole(span.num_days()))),
        };
        StoredDeadline {
            end_date: due.end_date,
            start_percent: due.start_percent,
            end_percent: due.end_percent,
            lead_up_weeks,
            lead_up_days,
            start_date: due.start_date,
        }
    }
}
