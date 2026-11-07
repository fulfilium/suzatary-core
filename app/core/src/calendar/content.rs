//! What an occurrence says: the content register's value.

use super::When;

/// The most characters of a title or a place.
pub const MAX_TEXT: usize = 500;

/// The most characters of one attendee's name.
pub const MAX_NAME: usize = 200;

/// The most attendees.
pub(crate) const MAX_ATTENDEES: usize = 50;

/// Whether it is happening, as its feed says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// It is happening.
    Confirmed,
    /// It may happen.
    Tentative,
    /// It is not: cancelled in the feed, or gone from it while in the window.
    Cancelled,
}

impl Status {
    /// Its one spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::Confirmed => "confirmed",
            Status::Tentative => "tentative",
            Status::Cancelled => "cancelled",
        }
    }

    /// Read that spelling.
    ///
    /// # Errors
    /// `NotAnOccurrence::Status` for any other.
    pub fn parse(text: &str) -> Result<Self, NotAnOccurrence> {
        match text {
            "confirmed" => Ok(Status::Confirmed),
            "tentative" => Ok(Status::Tentative),
            "cancelled" => Ok(Status::Cancelled),
            other => Err(NotAnOccurrence::Status(other.chars().take(40).collect())),
        }
    }
}

/// An occurrence's content: its title, its span, its place, its attendees' names and its status.
/// Titles, places and names are other people's words: a reader marks them so, as it marks mail.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Content {
    title: String,
    when: When,
    place: String,
    attendees: Vec<String>,
    status: Status,
}

/// Why something is not an occurrence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAnOccurrence {
    /// Not a plugin credential's name.
    #[error("{0:?} is not a calendar's name")]
    Calendar(String),
    /// An empty or overlong `UID`, or one with a control character.
    #[error("{0:?} is not a UID this store keeps")]
    Uid(String),
    /// Neither `YYYYMMDD` nor `YYYYMMDDTHHMMSSZ`.
    #[error("{0:?} is not an occurrence's original start")]
    Key(String),
    /// A misspelt or inverted span.
    #[error("not a span: a known kind, two well-spelt times, the end not before the start")]
    Span,
    /// Not one of the three.
    #[error("{0:?} is not a status")]
    Status(String),
    /// A title, place or name past its length, or more attendees than kept.
    #[error("{0} is longer than this store keeps")]
    TooLong(&'static str),
    /// A calendar outside the namespace of the plugin that reported it.
    #[error("{0} is not a calendar of the plugin that reported it")]
    NotItsCalendar(String),
    /// A field the calendar reads is not in the schema, or holds a value of another type.
    #[error("the schema gives no {0} a calendar reads")]
    Field(&'static str),
}

impl Content {
    /// Check and build one; empty text means absent.
    ///
    /// # Errors
    /// `NotAnOccurrence::TooLong` past a limit the plugin is held to as well.
    pub fn new(
        title: String,
        when: When,
        place: String,
        attendees: Vec<String>,
        status: Status,
    ) -> Result<Self, NotAnOccurrence> {
        if title.chars().count() > MAX_TEXT {
            return Err(NotAnOccurrence::TooLong("a title"));
        }
        if place.chars().count() > MAX_TEXT {
            return Err(NotAnOccurrence::TooLong("a place"));
        }
        if attendees.len() > MAX_ATTENDEES {
            return Err(NotAnOccurrence::TooLong("the attendee list"));
        }
        if attendees.iter().any(|name| name.chars().count() > MAX_NAME) {
            return Err(NotAnOccurrence::TooLong("an attendee's name"));
        }
        Ok(Self {
            title,
            when,
            place,
            attendees,
            status,
        })
    }

    /// The title, `""` for none.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// When.
    #[must_use]
    pub fn when(&self) -> &When {
        &self.when
    }

    /// The place, `""` for none.
    #[must_use]
    pub fn place(&self) -> &str {
        &self.place
    }

    /// The attendees' names; never an address.
    #[must_use]
    pub fn attendees(&self) -> &[String] {
        &self.attendees
    }

    /// The status.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }
}
