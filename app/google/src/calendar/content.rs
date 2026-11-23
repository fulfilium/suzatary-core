//! What an occurrence says: the content register's value.

use super::{Attendees, When};

/// The most characters of a title or a place.
pub const MAX_TEXT: usize = 500;

/// Whether it is happening, as its feed says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    Confirmed,
    Tentative,
    /// It is not: cancelled in the feed, or gone from it while in the window.
    Cancelled,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::Confirmed => "confirmed",
            Status::Tentative => "tentative",
            Status::Cancelled => "cancelled",
        }
    }

    pub fn parse(text: &str) -> Result<Self, NotAnOccurrence> {
        match text {
            "confirmed" => Ok(Status::Confirmed),
            "tentative" => Ok(Status::Tentative),
            "cancelled" => Ok(Status::Cancelled),
            other => Err(NotAnOccurrence::Status(other.chars().take(40).collect())),
        }
    }
}

/// Titles, places and names are other people's words: a reader marks them so, as it marks mail.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Content {
    title: String,
    when: When,
    place: String,
    attendees: Attendees,
    status: Status,
}

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
    #[error("not a span: a known kind, two well-spelt times, the end not before the start")]
    Span,
    #[error("{0:?} is not a status")]
    Status(String),
    #[error("{0} is longer than this store keeps")]
    TooLong(&'static str),
    #[error("{0} is not a calendar of the plugin that reported it")]
    NotItsCalendar(String),
    /// A field the calendar reads is not in the schema, or holds a value of another type.
    #[error("the schema gives no {0} a calendar reads")]
    Field(&'static str),
    #[error("the occurrence does not print at its schema: {0}")]
    Unprinted(String),
}

impl Content {
    /// Empty text means absent; the plugin is held to the same limits.
    pub fn new(
        title: String,
        when: When,
        place: String,
        attendees: Attendees,
        status: Status,
    ) -> Result<Self, NotAnOccurrence> {
        if title.chars().count() > MAX_TEXT {
            return Err(NotAnOccurrence::TooLong("a title"));
        }
        if place.chars().count() > MAX_TEXT {
            return Err(NotAnOccurrence::TooLong("a place"));
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

    #[must_use]
    pub fn when(&self) -> &When {
        &self.when
    }

    /// The place, `""` for none.
    #[must_use]
    pub fn place(&self) -> &str {
        &self.place
    }

    #[must_use]
    pub fn attendees(&self) -> &Attendees {
        &self.attendees
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }
}
