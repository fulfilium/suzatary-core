//! The calendar: what a `source` plugin learned of the owner's calendars, read from its store.
//!
//! A CACHE-tier store (`docs/PLUGINS-PLAN-2026-10-02.md` §4): a replica of the providers'
//! calendars, box-local, never pushed, rebuilt by syncing again. Its schema is not Rust's but the
//! plugin's own DECLARATION (`app/plugin-ics/ics.schema`), admitted by the host when it loads the
//! plugin: one `Learned` event per occurrence, keyed by its calendar, its original start and its
//! `UID`, writing five discrete registers (title, when, place, attendees, status). A later sync
//! supersedes what it saw, and two hosts syncing one calendar at once is a conflict shown whole,
//! never a winner picked.
//!
//! This module is the calendar's READER: [`Occurrence::reads`] says whether a declared store is a
//! calendar, [`Occurrence::of`] reads one write into an occurrence through every smart
//! constructor, and [`Calendar`] is the store read. Meetings are never todos: nothing here writes
//! to a chain, and a reader marks every title as other people's words, as mail is.

mod content;
pub mod effect;
mod name;
mod occurrence;
mod reading;
mod when;

pub use content::{Content, MAX_NAME, MAX_TEXT, Status};
pub(crate) use content::{MAX_ATTENDEES, NotAnOccurrence};
pub use name::{CalendarName, OccurrenceId};
pub use occurrence::Occurrence;
pub use reading::{Calendar, Day, Entry};
pub use when::When;
