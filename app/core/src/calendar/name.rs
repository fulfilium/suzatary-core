//! What names an occurrence: its calendar, its event's `UID` and its original start.

use std::fmt;

use super::NotAnOccurrence;

/// The longest name, `UID` or key kept.
const MAX: usize = 500;

/// A calendar, by the credential its link is held in: `PLUGIN_<NAME>_<CALENDAR>`. A name, never
/// the link, so a store and a page can say which calendar without holding the secret.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarName(String);

impl CalendarName {
    /// Parse one.
    ///
    /// # Errors
    /// [`NotAnOccurrence::Calendar`] for anything not shaped like a plugin's credential name.
    pub fn parse(text: &str) -> Result<Self, NotAnOccurrence> {
        let shaped = text.len() <= 96
            && text
                .strip_prefix("PLUGIN_")
                .and_then(|rest| rest.split_once('_'))
                .is_some_and(|(plugin, calendar)| {
                    !plugin.is_empty()
                        && !calendar.is_empty()
                        && text
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                });
        if shaped {
            Ok(Self(text.to_owned()))
        } else {
            Err(NotAnOccurrence::Calendar(text.chars().take(100).collect()))
        }
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether it is in the namespace of the plugin named `plugin`, `PLUGIN_<PLUGIN>_`: the only
    /// calendars that plugin reads, and so the only ones it may report.
    #[must_use]
    pub fn of_plugin(&self, plugin: &str) -> bool {
        self.0
            .strip_prefix("PLUGIN_")
            .and_then(|rest| rest.split_once('_'))
            .is_some_and(|(namespace, _)| namespace.eq_ignore_ascii_case(plugin))
    }

    /// The part after the plugin's namespace, as a page labels it: `WORK` of `PLUGIN_ICS_WORK`.
    #[must_use]
    pub fn label(&self) -> &str {
        self.0
            .strip_prefix("PLUGIN_")
            .and_then(|rest| rest.split_once('_'))
            .map_or(self.0.as_str(), |(_, calendar)| calendar)
    }
}

impl fmt::Display for CalendarName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One occurrence of one event: the store's key.
///
/// `occurrence` is the instance's ORIGINAL start (`YYYYMMDD` or `YYYYMMDDTHHMMSSZ`), what a
/// `RECURRENCE-ID` names, so an instance moved to another day is still itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OccurrenceId {
    calendar: CalendarName,
    uid: String,
    occurrence: String,
}

impl OccurrenceId {
    /// Check and build one.
    ///
    /// # Errors
    /// An empty or overlong `UID`, one holding a control character, or a key of neither shape.
    pub fn new(
        calendar: CalendarName,
        uid: &str,
        occurrence: &str,
    ) -> Result<Self, NotAnOccurrence> {
        if uid.is_empty() || uid.chars().count() > MAX || uid.chars().any(char::is_control) {
            return Err(NotAnOccurrence::Uid(uid.chars().take(100).collect()));
        }
        let digits = |text: &str| text.bytes().all(|b| b.is_ascii_digit());
        let key = match occurrence.len() {
            8 => digits(occurrence),
            16 => {
                digits(&occurrence[..8])
                    && occurrence.as_bytes()[8] == b'T'
                    && digits(&occurrence[9..15])
                    && occurrence.as_bytes()[15] == b'Z'
            }
            _ => false,
        };
        if !key {
            return Err(NotAnOccurrence::Key(occurrence.chars().take(100).collect()));
        }
        Ok(Self {
            calendar,
            uid: uid.to_owned(),
            occurrence: occurrence.to_owned(),
        })
    }

    /// Read the one text a store keys an occurrence by: its calendar, its original start and its
    /// `UID`, each separated by a space. Neither of the first two holds one, so a `UID` that does
    /// is still read whole.
    ///
    /// # Errors
    /// As [`OccurrenceId::new`], and [`NotAnOccurrence::Key`] for a text of fewer parts.
    pub fn parse(key: &str) -> Result<Self, NotAnOccurrence> {
        let mut parts = key.splitn(3, ' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(calendar), Some(occurrence), Some(uid)) => {
                Self::new(CalendarName::parse(calendar)?, uid, occurrence)
            }
            _ => Err(NotAnOccurrence::Key(key.chars().take(100).collect())),
        }
    }

    /// Its one spelling as a key, as [`OccurrenceId::parse`] reads it.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{} {} {}", self.calendar, self.occurrence, self.uid)
    }

    /// Its calendar.
    #[must_use]
    pub fn calendar(&self) -> &CalendarName {
        &self.calendar
    }

    /// Its event's `UID`.
    #[must_use]
    pub fn uid(&self) -> &str {
        &self.uid
    }

    /// Its original start, as written.
    #[must_use]
    pub fn occurrence(&self) -> &str {
        &self.occurrence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calendar_is_a_plugin_credential_name() {
        let name = CalendarName::parse("PLUGIN_ICS_WORK").expect("a name");
        assert_eq!(name.label(), "WORK");
        for bad in [
            "MODEL_TOKEN",
            "PLUGIN_ICS",
            "PLUGIN__X",
            "PLUGIN_ICS_",
            "plugin_ics_work",
            "PLUGIN_ICS_WO-RK",
        ] {
            assert!(CalendarName::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_occurrence_key_is_a_date_or_a_utc_instant() {
        let calendar = CalendarName::parse("PLUGIN_ICS_HOME").expect("a name");
        assert!(OccurrenceId::new(calendar.clone(), "a@b", "20261005").is_ok());
        assert!(OccurrenceId::new(calendar.clone(), "a@b", "20261005T130000Z").is_ok());
        for bad in ["2026-10-05", "20261005T130000", "20261005X130000Z", ""] {
            assert!(
                OccurrenceId::new(calendar.clone(), "a@b", bad).is_err(),
                "{bad}"
            );
        }
        assert!(OccurrenceId::new(calendar.clone(), "", "20261005").is_err());
        assert!(OccurrenceId::new(calendar.clone(), "a\nb", "20261005").is_err());
        let spaced = OccurrenceId::new(calendar, "a b@c", "20261005T130000Z").expect("an id");
        assert_eq!(spaced.key(), "PLUGIN_ICS_HOME 20261005T130000Z a b@c");
        assert_eq!(OccurrenceId::parse(&spaced.key()), Ok(spaced));
        assert!(OccurrenceId::parse("PLUGIN_ICS_HOME 20261005").is_err());
    }

    #[test]
    fn a_calendar_belongs_to_the_plugin_it_is_named_for() {
        let name = CalendarName::parse("PLUGIN_ICS_WORK").expect("a name");
        assert!(name.of_plugin("ics"));
        assert!(!name.of_plugin("bank"));
        assert!(!name.of_plugin("ic"));
    }
}
