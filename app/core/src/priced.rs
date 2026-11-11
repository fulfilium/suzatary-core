//! A todo's price as a call spells it: one of four shapes, over ten arguments.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::todo::{Cycle, Deadline, Price};
use crate::verb::Caught;
use suzatary_base::refusal::{Refusal, shown};

/// The price arguments of a call, each read only once the others make it matter, so a fault in
/// one the call does not use is never raised.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default)]
pub(crate) struct Priced {
    pub priority: Caught<Option<String>>,
    pub(crate) deadline_end_date: Caught<Option<String>>,
    /// Absent is 60%.
    pub(crate) deadline_start_priority: Caught<Option<String>>,
    /// Absent is 5%.
    pub(crate) deadline_end_priority: Caught<Option<String>>,
    pub(crate) deadline_start_date: Caught<Option<String>>,
    pub(crate) deadline_lead_up_weeks: Caught<Option<usize>>,
    pub recur_every_days: Caught<Option<usize>>,
    pub periodic_every_days: Caught<Option<usize>>,
    pub cycle_from_priority: Caught<Option<String>>,
    pub cycle_to_priority: Caught<Option<String>>,
}

/// What a call that spells two prices is told, and one that needs a price and spells none.
pub(crate) const PRICE_SPELLINGS: &str = "a price is ONE of `priority`, `deadline_end_date`, \
     `recur_every_days` or `periodic_every_days`: they are different prices and a todo \
     holds one";

impl Priced {
    /// The one price the call names, if any; two are refused, since a todo holds one.
    ///
    /// # Errors
    /// A part of the wrong type, a bad percentage or day, a cycle missing an end, or two prices.
    pub(crate) fn price(&self) -> Result<Option<Price>, Refusal> {
        let flat = match self.priority.read()? {
            None => None,
            Some(text) => Some(Price::Flat {
                percent: percent(text, "priority")?,
            }),
        };
        let recur = self
            .cycle(&self.recur_every_days, "recur_every_days")?
            .map(Price::Recur);
        let periodic = self
            .cycle(&self.periodic_every_days, "periodic_every_days")?
            .map(Price::Periodic);
        if recur.is_none()
            && periodic.is_none()
            && (self.cycle_from_priority.read()?.is_some()
                || self.cycle_to_priority.read()?.is_some())
        {
            return Err(Refusal::bad_request(
                "`cycle_from_priority` and `cycle_to_priority` shape a cycle: give \
                 `recur_every_days` or `periodic_every_days` with them",
            ));
        }
        let named: Vec<Price> = [flat, self.deadline()?.map(Price::Deadline), recur, periodic]
            .into_iter()
            .flatten()
            .collect();
        match <[Price; 1]>::try_from(named) {
            Ok([one]) => Ok(Some(one)),
            Err(named) if named.is_empty() => Ok(None),
            Err(_) => Err(Refusal::bad_request(PRICE_SPELLINGS)),
        }
    }

    /// The `deadline_*` arguments as a [`Deadline`]; weeks are this surface's unit.
    fn deadline(&self) -> Result<Option<Deadline>, Refusal> {
        let Some(end) = self.deadline_end_date.read()? else {
            return Ok(None);
        };
        let weeks = *self.deadline_lead_up_weeks.read()?;
        let or = |part: &Caught<Option<String>>, absent: &'static str| {
            part.read()
                .map(|given| given.clone().unwrap_or_else(|| absent.to_owned()))
        };
        Ok(Some(Deadline {
            end_date: day_of(end, "end_date")?,
            start_percent: percent(&or(&self.deadline_start_priority, "60%")?, "start_priority")?,
            end_percent: percent(&or(&self.deadline_end_priority, "5%")?, "end_priority")?,
            lead_up: match weeks {
                None => None,
                Some(weeks) => Some(chrono::Duration::weeks(
                    i64::try_from(weeks)
                        .map_err(|_| wrong("deadline_lead_up_weeks", "a few weeks"))?,
                )),
            },
            start_date: match self.deadline_start_date.read()? {
                None => None,
                Some(day) => Some(day_of(day, "start_date")?),
            },
        }))
    }

    /// A cycle when `days` is given, which then needs both ends.
    fn cycle(&self, days: &Caught<Option<usize>>, name: &str) -> Result<Option<Cycle>, Refusal> {
        let Some(days) = *days.read()? else {
            return Ok(None);
        };
        let end = |part: &Caught<Option<String>>, field: &str| match part.read()? {
            Some(text) => percent(text, field),
            None => Err(Refusal::bad_request(format!(
                "`{name}` needs both ends of its cycle: `cycle_from_priority` and \
                 `cycle_to_priority`"
            ))),
        };
        Ok(Some(Cycle {
            days: u32::try_from(days).map_err(|_| wrong(name, "a number of days"))?,
            from_percent: end(&self.cycle_from_priority, "cycle_from_priority")?,
            to_percent: end(&self.cycle_to_priority, "cycle_to_priority")?,
        }))
    }
}

fn wrong(name: &str, wanted: &str) -> Refusal {
    Refusal::bad_request(format!("{name} must be {wanted}"))
}

/// `YYYY-MM-DD`, refused naming `field`.
pub(crate) fn day_of(text: &str, field: &str) -> Result<chrono::NaiveDate, Refusal> {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| {
        Refusal::bad_request(format!("{field} must be YYYY-MM-DD, got {}", shown(text)))
    })
}

/// `'65%'` as a whole number of percent.
pub(crate) fn percent(text: &str, field: &str) -> Result<u32, Refusal> {
    let refuse = || {
        Refusal::bad_request(format!(
            "{field} must be a whole-number percentage like '65%', got {}. \
             It is a FULFILLMENT, so LOW means urgent.",
            shown(text)
        ))
    };
    let digits = text.strip_suffix('%').ok_or_else(refuse)?;
    // `065%` is a typo, not a guess to make about a fulfillment.
    if !(digits == "100"
        || (matches!(digits.len(), 1 | 2) && digits.bytes().all(|b| b.is_ascii_digit())))
    {
        return Err(refuse());
    }
    digits.parse().map_err(|_| refuse())
}
