//! The inert calendar: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_store::store::StoreError;

use crate::calendar::Day;
use crate::calendar::effect::Calendar;

/// No calendar: every day asked for is empty.
impl Calendar for Nothing {
    fn days(&self, first: chrono::NaiveDate, count: u32) -> Result<Vec<Day>, StoreError> {
        Ok(first
            .iter_days()
            .take(count as usize)
            .map(|date| Day {
                date,
                entries: Vec::new(),
            })
            .collect())
    }
}
