//! The owner's calendars, read in the box's own clock.

use chrono::NaiveDate;

use crate::calendar::Day;
use crate::store::StoreError;

/// Reading the calendar a `source` plugin fills: the days asked for, each occurrence placed in
/// the box's clock. Read-only; its titles, places and names are other people's words.
pub trait Calendar: Sync {
    /// `count` days from `first`.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn days(&self, first: NaiveDate, count: u32) -> Result<Vec<Day>, StoreError>;
}
