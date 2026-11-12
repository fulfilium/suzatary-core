//! A cycle: `from_percent` at its start, a straight line to `to_percent` over `days`, held there
//! after.

use chrono::Duration;
use prodrome_core::fpl;
use prodrome_core::term::{CurvePoint, Term};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::todo::price::ratio;
use suzatary_base::refusal::Refusal;

/// The ramp both recurrences repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Cycle {
    /// How long the ramp runs, and for a `Periodic` how often it repeats.
    pub days: u32,
    /// The fulfillment just after a pass.
    pub from_percent: u32,
    /// The fulfillment it reaches, and stays at, after `days`.
    pub to_percent: u32,
}

impl Cycle {
    /// A two-point `Curve` from `anchor`.
    ///
    /// A curve and not a decay, though a decay is the same line: every reader that finds a
    /// `Decay` takes it for a deadline, and a `Decay` starts at 98% at most where a chore just
    /// done is 100%.
    pub(crate) fn ramp(self, anchor: fpl::Instant) -> Result<Term, Refusal> {
        if !(1..=3650).contains(&self.days) {
            return Err(Refusal::bad_request(format!(
                "a cycle runs between a day and ten years, got {} days",
                self.days
            )));
        }
        fpl::mk_curve(vec![
            CurvePoint {
                at: anchor,
                value: ratio(self.from_percent, "from")?,
                label: String::new(),
            },
            CurvePoint {
                at: anchor + self.span(),
                value: ratio(self.to_percent, "to")?,
                label: String::new(),
            },
        ])
        .map_err(|e| Refusal::bad_request(e.to_string()))
    }

    pub(crate) fn span(self) -> Duration {
        Duration::days(i64::from(self.days))
    }
}
