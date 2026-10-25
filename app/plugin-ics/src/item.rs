//! What the plugin emits: one item per event occurrence, an event of `ics.schema` printed by the
//! Prodrome's own printer. [`Item`] is also what the cursor keeps, as JSON private to this plugin.

use chrono::{Datelike, NaiveDateTime, Timelike};
use prodrome_core::literal::{Datetime, ProdromeError, Value, print_literal};
use serde::{Deserialize, Serialize};

/// The schema every item is in, by its content name: the hash of `ics.schema`'s text.
pub const SCHEMA: &str = "c97b894305a4977cde486b7d2db899c97a36e078e48de408373cb8a188a9a143";

/// The most characters of a title, a place or a UID kept.
pub const MAX_TEXT: usize = 500;

/// The most characters of one attendee's name kept.
pub const MAX_NAME: usize = 200;

/// The most attendees kept.
pub const MAX_ATTENDEES: usize = 50;

/// A span of UTC time, `[from, to)`.
pub type Window = (NaiveDateTime, NaiveDateTime);

/// One occurrence of one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    /// The credential the feed was read through, which names the calendar, never its URL.
    pub calendar: String,
    /// The event's `UID`.
    pub uid: String,
    /// The original start, `YYYYMMDDTHHMMSSZ` or `YYYYMMDD`, so a moved instance is still itself.
    pub occurrence: String,
    /// `SUMMARY`, or `""`.
    pub title: String,
    /// When it is.
    pub when: When,
    /// `LOCATION`, or `""`.
    pub place: String,
    /// Each attendee's `CN`, in order; an attendee with no name is left out, never its address.
    pub attendees: Vec<String>,
    /// `STATUS`.
    pub status: Status,
}

/// When an occurrence is: a span of UTC instants, or of whole days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum When {
    /// From `start` until `end`, both `YYYY-MM-DDTHH:MM:SSZ`, the end at or after the start.
    Timed {
        /// The start.
        start: String,
        /// The end.
        end: String,
    },
    /// From the day `start` until the day before `end`, both `YYYY-MM-DD`.
    AllDay {
        /// The first day.
        start: String,
        /// The day after the last.
        end: String,
    },
}

/// `STATUS`, `CONFIRMED` when the feed says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// It is happening.
    Confirmed,
    /// It may happen.
    Tentative,
    /// It is not happening: cancelled in the feed, or gone from it.
    Cancelled,
}

/// How a [`When::Timed`] spells an instant.
pub const INSTANT: &str = "%Y-%m-%dT%H:%M:%SZ";

/// How a [`When::AllDay`] spells a day.
pub const DAY: &str = "%Y-%m-%d";

impl When {
    /// Its start and end as UTC instants, a day read as its midnight.
    #[must_use]
    pub fn span(&self) -> Option<(NaiveDateTime, NaiveDateTime)> {
        let instant = |text: &str| NaiveDateTime::parse_from_str(text, INSTANT).ok();
        let day = |text: &str| instant(&format!("{text}T00:00:00Z"));
        match self {
            When::Timed { start, end } => Some((instant(start)?, instant(end)?)),
            When::AllDay { start, end } => Some((day(start)?, day(end)?)),
        }
    }
}

impl Status {
    /// Its spelling, one of the declaration's alternatives.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Confirmed => "confirmed",
            Status::Tentative => "tentative",
            Status::Cancelled => "cancelled",
        }
    }
}

/// `now` as the store spells an instant.
///
/// # Errors
/// An instant outside the years the store holds.
pub fn at(now: NaiveDateTime) -> Result<Datetime, ProdromeError> {
    let (day, micros) = (now.date(), now.and_utc().timestamp_subsec_micros());
    let (year, month, date) = (day.year(), day.month(), day.day());
    let (hour, minute, second) = (now.hour(), now.minute(), now.second());
    Datetime::new(year, month, date, hour, minute, second, micros)
}

impl Item {
    /// The event the store learns it by, learned at `at`, in the declaration's field order; its
    /// actor is the plugin, `ics` of `PLUGIN_ICS_HOME`, the only namespace the host lends it.
    #[must_use]
    pub fn event(&self, at: Datetime) -> Vec<u8> {
        let rest = self.calendar.strip_prefix("PLUGIN_").unwrap_or_default();
        let actor = rest.split_once('_').map_or("", |(name, _)| name);
        let occurrence = format!("{} {} {}", self.calendar, self.occurrence, self.uid);
        let (When::Timed { start, end } | When::AllDay { start, end }) = &self.when;
        let attendees = Value::Tuple(self.attendees.iter().map(Value::str).collect());
        let fields = [
            ("occurrence", Value::str(occurrence)),
            ("at", Value::Datetime(at)),
            ("actor", Value::str(actor.to_ascii_lowercase())),
            ("title", Value::str(&self.title)),
            ("when", Value::str(format!("{start}/{end}"))),
            ("place", Value::str(&self.place)),
            ("attendees", attendees),
            ("status", Value::str(self.status.as_str())),
        ];
        let fields = fields.map(|(name, value)| (name.to_owned(), value));
        print_literal(&Value::call("Learned", fields.into())).into_bytes()
    }

    /// Whether any of it falls in `window`.
    #[must_use]
    pub fn overlaps(&self, (from, to): Window) -> bool {
        let span = self.when.span();
        span.is_some_and(|(start, end)| {
            start < to && (end > from || (end == start && start >= from))
        })
    }
}
