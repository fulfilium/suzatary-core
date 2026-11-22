//! The calendar: what a `source` plugin learned of the owner's calendars, read from its store.
//!
//! A CACHE-tier store: box-local, never pushed, rebuilt by syncing again. Its schema is the
//! plugin's own (`app/plugin-ics/ics.schema`): one `Learned` event per occurrence, keyed by its
//! calendar, its original start and its `UID`; two hosts syncing at once are a conflict shown
//! whole.
//!
//! Meetings are never todos: nothing here writes to a chain, and a reader marks every title as
//! other people's words, as mail is.

mod content;
pub mod effect;
mod name;
mod occurrence;
pub mod page;
mod reading;
mod when;

pub use content::{Content, MAX_NAME, MAX_TEXT, Status};
pub(crate) use content::{MAX_ATTENDEES, NotAnOccurrence};
pub use name::{CalendarName, OccurrenceId};
pub use occurrence::Occurrence;
pub use reading::{Calendar, Day, Entry};
pub use when::When;
