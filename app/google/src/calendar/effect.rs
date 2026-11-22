//! The owner's calendars, read in the box's own clock.

use chrono::NaiveDate;

use crate::calendar::Day;
use suzatary_store::store::StoreError;

/// Read-only; its titles, places and names are other people's words.
pub trait Calendar: Sync {
    fn days(&self, first: NaiveDate, count: u32) -> Result<Vec<Day>, StoreError>;
}
