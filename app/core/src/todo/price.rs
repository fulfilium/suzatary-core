//! What a todo is worth, as every writer authors it and a proposed act stores it.

use prodrome_core::fpl;
use prodrome_core::literal::{ProdromeError, Value};
use prodrome_core::term::{Term, TermF};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::literal::{Fields, Literal, Signature, arm, no_arm, record};
use crate::refusal::Refusal;
use crate::todo::{Cycle, Deadline};
use crate::zone::Moment;

/// One price, since a todo carrying two has no defined value. Percentages are whole; low is
/// urgent. The serde form is a verb's argument, and the inbox's acts' before they were literals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "how", rename_all = "lowercase")]
pub enum Price {
    /// A fulfillment that does not move.
    Flat {
        /// The number.
        percent: u32,
    },
    /// A fulfillment that decays toward a date.
    Deadline(Deadline),
    /// A chore whose cycle restarts at every pass (`Tended`); before the first pass it is the
    /// price it replaces, or for a new todo the cycle from its creation.
    Recur(Cycle),
    /// The cycle repeated on the calendar from the instant it is authored, whatever is done.
    Periodic(Cycle),
}

impl Price {
    /// The term this price is, for `todo`, authored at `now` over `replaced`, the todo's own term
    /// before it (`None` for a new todo).
    ///
    /// A `Recur` over a todo already recurring waits on that one's pending price rather than
    /// nesting it: with no pass yet the two read the same, and after one neither reads it.
    ///
    /// # Errors
    /// A percentage over 100, a cycle or lead-up out of range, or a term the evaluator refuses.
    pub fn term(&self, todo: &str, now: Moment, replaced: Option<&Term>) -> Result<Term, Refusal> {
        let anchor = fpl::instant_of(now.at);
        match self {
            Price::Flat { percent } => fpl::mk_flat(ratio(*percent, "priority")?).map_err(refused),
            Price::Deadline(due) => due.term(now.zone),
            Price::Recur(cycle) => {
                let body = cycle.ramp(anchor)?;
                let pending = match replaced {
                    None => body.clone(),
                    Some(term) => match term.out() {
                        TermF::Recur {
                            todo: over,
                            pending,
                            ..
                        } if over == todo => pending.clone(),
                        _ => term.clone(),
                    },
                };
                fpl::mk_recur(todo.to_owned(), anchor, body, pending).map_err(refused)
            }
            Price::Periodic(cycle) => {
                fpl::mk_periodic(cycle.span(), anchor, cycle.ramp(anchor)?).map_err(refused)
            }
        }
    }
}

fn refused(failed: impl std::fmt::Display) -> Refusal {
    Refusal::bad_request(failed.to_string())
}

/// A whole percentage as the ratio the evaluator takes.
///
/// # Errors
/// Over 100, naming `field`.
pub fn ratio(percent: u32, field: &str) -> Result<f64, Refusal> {
    if percent > 100 {
        return Err(Refusal::bad_request(format!(
            "{field} must be a whole-number percentage like '65%', got {percent}%. \
             It is a FULFILLMENT, so LOW means urgent."
        )));
    }
    Ok(f64::from(percent) / 100.0)
}

/// `FlatPrice(percent)`; the term vocabulary's `Flat` is a term, not a price.
pub const FLAT_PRICE: Signature = ("FlatPrice", &["percent"]);
/// `DeadlinePrice(end_date, start_percent, end_percent, lead_up_days, start_date)`.
pub const DEADLINE_PRICE: Signature = (
    "DeadlinePrice",
    &[
        "end_date",
        "start_percent",
        "end_percent",
        "lead_up_days",
        "start_date",
    ],
);
/// `RecurPrice(days, from_percent, to_percent)`.
pub const RECUR_PRICE: Signature = ("RecurPrice", &["days", "from_percent", "to_percent"]);
/// `PeriodicPrice(days, from_percent, to_percent)`.
pub const PERIODIC_PRICE: Signature = ("PeriodicPrice", &["days", "from_percent", "to_percent"]);

fn cycle(signature: Signature, cycle: &Cycle) -> Value {
    record(
        signature,
        vec![
            cycle.days.to_value(),
            cycle.from_percent.to_value(),
            cycle.to_percent.to_value(),
        ],
    )
}

fn cycle_of(value: &Value, signature: Signature) -> Result<Cycle, ProdromeError> {
    let fields = Fields::of(value, signature)?;
    Ok(Cycle {
        days: fields.get("days")?,
        from_percent: fields.get("from_percent")?,
        to_percent: fields.get("to_percent")?,
    })
}

impl Literal for Price {
    fn to_value(&self) -> Value {
        match self {
            Price::Flat { percent } => record(FLAT_PRICE, vec![percent.to_value()]),
            Price::Deadline(due) => record(
                DEADLINE_PRICE,
                vec![
                    due.end_date.to_value(),
                    due.start_percent.to_value(),
                    due.end_percent.to_value(),
                    due.lead_up.to_value(),
                    due.start_date.to_value(),
                ],
            ),
            Price::Recur(recur) => cycle(RECUR_PRICE, recur),
            Price::Periodic(periodic) => cycle(PERIODIC_PRICE, periodic),
        }
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        Ok(match arm(value)? {
            "FlatPrice" => Price::Flat {
                percent: Fields::of(value, FLAT_PRICE)?.get("percent")?,
            },
            "DeadlinePrice" => {
                let fields = Fields::of(value, DEADLINE_PRICE)?;
                Price::Deadline(Deadline {
                    end_date: fields.get("end_date")?,
                    start_percent: fields.get("start_percent")?,
                    end_percent: fields.get("end_percent")?,
                    lead_up: fields.get("lead_up_days")?,
                    start_date: fields.get("start_date")?,
                })
            }
            "RecurPrice" => Price::Recur(cycle_of(value, RECUR_PRICE)?),
            "PeriodicPrice" => Price::Periodic(cycle_of(value, PERIODIC_PRICE)?),
            other => return Err(no_arm("a price", other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("a day")
    }

    /// The shapes as the inbox stores them.
    #[test]
    fn a_price_prints_as_a_stored_act_spells_it() {
        let printed = |price: &Price| serde_json::to_string(price).expect("prints");
        assert_eq!(
            printed(&Price::Flat { percent: 40 }),
            r#"{"how":"flat","percent":40}"#
        );
        assert_eq!(
            printed(&Price::Deadline(Deadline {
                end_date: day("2026-10-20"),
                start_percent: 70,
                end_percent: 10,
                lead_up: Some(chrono::Duration::weeks(3)),
                start_date: None,
            })),
            r#"{"how":"deadline","end_date":"2026-10-20","start_percent":70,"end_percent":10,"lead_up_weeks":3,"start_date":null}"#
        );
        assert_eq!(
            printed(&Price::Recur(Cycle {
                days: 60,
                from_percent: 100,
                to_percent: 30
            })),
            r#"{"how":"recur","days":60,"from_percent":100,"to_percent":30}"#
        );
    }

    #[test]
    fn a_lead_up_in_days_reads_back_as_days() {
        let due = Price::Deadline(Deadline {
            end_date: day("2026-10-20"),
            start_percent: 60,
            end_percent: 5,
            lead_up: Some(chrono::Duration::days(10)),
            start_date: None,
        });
        let printed = serde_json::to_string(&due).expect("prints");
        assert!(
            printed.contains(r#""lead_up_weeks":null,"lead_up_days":10"#),
            "{printed}"
        );
        assert_eq!(serde_json::from_str::<Price>(&printed).expect("reads"), due);
        assert!(
            serde_json::from_str::<Price>(
                r#"{"how":"deadline","end_date":"2026-10-20","start_percent":60,"end_percent":5,"lead_up_weeks":1,"lead_up_days":3,"start_date":null}"#
            )
            .is_err()
        );
    }
}
