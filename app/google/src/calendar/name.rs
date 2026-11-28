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

    /// The name a calendar's occurrences are kept under: `PLUGIN_<PROVIDER>_` and the account's
    /// folder for its own calendar, and a digest of the calendar's id after it for each other, so
    /// a calendar keeps its name however the list is ordered. A folder too long for a calendar's
    /// name is spelt by its digest.
    ///
    /// # Errors
    /// None for any account and calendar the box reads.
    pub fn of(
        provider: super::Provider,
        account: &suzatary_mail::mail::address::EmailAddress,
        calendar: &super::Listed,
    ) -> Result<Self, NotAnOccurrence> {
        use sha2::Digest as _;
        let digest = |text: &str, digits: usize| {
            hex::encode_upper(sha2::Sha256::digest(text.as_bytes()))[..digits].to_owned()
        };
        let folder = account_folder(account);
        let folder = if folder.len() <= 60 {
            folder
        } else {
            digest(&folder, 16)
        };
        let plugin = provider.actor().to_ascii_uppercase();
        Self::parse(&if calendar.primary {
            format!("PLUGIN_{plugin}_{folder}")
        } else {
            format!("PLUGIN_{plugin}_{folder}_{}", digest(&calendar.id, 8))
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The provider whose namespace it is in.
    #[must_use]
    pub fn provider(&self) -> Option<super::Provider> {
        [super::Provider::Google, super::Provider::Microsoft]
            .into_iter()
            .find(|provider| self.of_plugin(provider.actor()))
    }

    /// Whether it is in the namespace of the plugin named `plugin`, `PLUGIN_<PLUGIN>_`: the only
    /// calendars that plugin reads, and so the only ones it may report.
    #[must_use]
    pub(crate) fn of_plugin(&self, plugin: &str) -> bool {
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

/// An account's maildir folder in capitals, what its calendars and its Google grant are named by.
#[must_use]
pub fn account_folder(account: &suzatary_mail::mail::address::EmailAddress) -> String {
    suzatary_mail::mail::account::slug_of(account.as_str())
        .to_ascii_uppercase()
        .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
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

    #[must_use]
    pub fn calendar(&self) -> &CalendarName {
        &self.calendar
    }

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

    /// Every provider names its calendars in its own namespace, the account's own as its folder,
    /// and a long address by its digest.
    #[test]
    fn a_calendars_name_is_its_own() {
        use crate::calendar::{Listed, Provider};
        use suzatary_mail::mail::address::EmailAddress;
        let ada = EmailAddress::new("ada@example.test").expect("an address");
        let long =
            EmailAddress::new(&format!("{}@example.test", "a".repeat(120))).expect("an address");
        let listed = |id: &str, primary| Listed {
            id: id.to_owned(),
            primary,
        };
        for (provider, namespace) in [
            (Provider::Google, "PLUGIN_GOOGLE_"),
            (Provider::Microsoft, "PLUGIN_MICROSOFT_"),
        ] {
            let own = CalendarName::of(provider, &ada, &listed("a", true)).expect("a name");
            assert_eq!(own.as_str(), format!("{namespace}ADA_AT_EXAMPLE_TEST"));
            let team = CalendarName::of(provider, &ada, &listed("team", false)).expect("a name");
            assert!(
                team.as_str()
                    .starts_with(&format!("{namespace}ADA_AT_EXAMPLE_TEST_")),
                "{team}"
            );
            assert!(team.of_plugin(provider.actor()));
            for primary in [true, false] {
                let name = CalendarName::of(provider, &long, &listed("x", primary))
                    .expect("a long address's calendar is named by its digest");
                assert!(name.of_plugin(provider.actor()), "{name}");
            }
        }
    }
}
