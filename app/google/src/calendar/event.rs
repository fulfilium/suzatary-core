//! An event the owner's tap writes to an account's own calendar, whichever provider keeps it.

use super::When;

/// An event's title, as a card shows it and the tap writes it: one line, at most 200
/// characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTitle(String);

impl EventTitle {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("an event needs a title".to_owned());
        }
        if text.chars().count() > 200 || text.chars().any(char::is_control) {
            return Err("an event's title is one line of at most 200 characters".to_owned());
        }
        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An event to write to the account's own calendar, on a tap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub title: EventTitle,
    pub when: When,
    /// Where, `""` for nowhere in particular.
    pub place: String,
}

impl NewEvent {
    /// An event, its place held to one line of at most 200 characters.
    pub fn new(title: &str, when: When, place: &str) -> Result<Self, String> {
        let place = place.trim();
        if place.chars().count() > 200 || place.chars().any(char::is_control) {
            return Err("an event's place is one line of at most 200 characters".to_owned());
        }
        Ok(Self {
            title: EventTitle::parse(title)?,
            when,
            place: place.to_owned(),
        })
    }

    #[must_use]
    pub fn sentence(&self) -> String {
        let when = match self.when {
            When::Timed { start, end } => format!(
                "{} to {} UTC",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%H:%M")
            ),
            When::AllDay { start, end } if end == start.succ_opt().unwrap_or(end) => {
                format!("all day on {start}")
            }
            When::AllDay { start, end } => {
                format!("all day from {start} to {}", end.pred_opt().unwrap_or(end))
            }
        };
        if self.place.is_empty() {
            format!("“{}”, {when}", self.title.as_str())
        } else {
            format!("“{}”, {when}, at {}", self.title.as_str(), self.place)
        }
    }
}

/// The text form a page and a stored act read, `{title, when, place}`, `when` as the calendar's
/// store spells a span.
#[derive(serde::Serialize, serde::Deserialize)]
struct Spelled {
    title: String,
    when: String,
    place: String,
}

impl serde::Serialize for NewEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Spelled {
            title: self.title.as_str().to_owned(),
            when: self.when.print(),
            place: self.place.clone(),
        }
        .serialize(serializer)
    }
}

// Read through `new`, so a stored act names only an event the constructor admits.
impl<'de> serde::Deserialize<'de> for NewEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Spelled::deserialize(deserializer)?;
        let when = When::parse(&spelled.when).map_err(serde::de::Error::custom)?;
        Self::new(&spelled.title, when, &spelled.place).map_err(serde::de::Error::custom)
    }
}
