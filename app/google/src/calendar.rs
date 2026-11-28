//! The calendar: what a `source` plugin learned of the owner's calendars, read from its store.
//!
//! A CACHE-tier store: box-local, never pushed, rebuilt by syncing again. Its schema is the
//! plugin's own (`app/plugin-ics/ics.schema`): one `Learned` event per occurrence, keyed by its
//! calendar, its original start and its `UID`; two hosts syncing at once are a conflict shown
//! whole.
//!
//! Meetings are never todos: nothing here writes to a chain, and a reader marks every title as
//! other people's words, as mail is.

mod attendees;
pub(crate) mod bounds;
mod content;
pub mod effect;
mod event;
mod kept;
mod listing;
mod name;
mod occurrence;
pub mod page;
mod point;
mod provider;
mod reading;
mod when;

pub use attendees::{Attendees, MAX_ATTENDEES};
pub use bounds::{CALENDARS, EVENTS, MAX_ANSWER, Unread, window};
pub(crate) use content::NotAnOccurrence;
pub use content::{Content, MAX_TEXT, Status};
pub use event::{EventTitle, NewEvent};
pub use kept::{Kept, learn_kept};
pub use listing::{Calendars, Listed};
pub use name::{CalendarName, OccurrenceId, account_folder};
pub use occurrence::{Occurrence, Said};
pub use point::Point;
pub use provider::Provider;
pub use reading::{Calendar, Day, Entry};
pub use suzatary_address::{AttendeeName, MAX_NAME, NotAName};
pub use when::When;

/// The text the calendar plugin declares (`app/plugin-ics/ics.schema`, which a test holds this
/// to), so the calendar reads what this source writes as it reads the plugin's.
pub const SCHEMA: &str = "Schema(key='occurrence', events=(Event(name='Learned', fields=(Field(name='occurrence', type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), Field(name='title', type=Text()), Field(name='when', type=Text()), Field(name='place', type=Text()), Field(name='attendees', type=List(item=Text())), Field(name='unnamed', type=Integer()), Field(name='status', type=Enum(alternatives=('cancelled', 'confirmed', 'tentative'))))),), registers=(Register(name='attendees', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='attendees'),)), Register(name='place', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='place'),)), Register(name='status', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='status'),)), Register(name='title', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='title'),)), Register(name='unnamed', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='unnamed'),)), Register(name='when', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='when'),))))";
