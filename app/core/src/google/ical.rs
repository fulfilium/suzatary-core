//! A Google calendar's secret iCal link, which the box never fetches: a Google account's
//! calendars are read through Google's own API once the account is connected. The link's path
//! names its calendar (`/calendar/ical/<id>/private-<key>/basic.ics`), and an account's own
//! calendar has the account's address for its id.

use crate::mail::address::EmailAddress;
use crate::plugin::PluginUrl;

/// The host that serves Google calendars' iCal links.
const HOST: &str = "calendar.google.com";

/// A calendar whose id ends so is no account's own (a shared, imported or holiday calendar).
const SHARED: &str = ".calendar.google.com";

/// The calendar a link from Google names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Linked {
    /// An account's own calendar, by the account.
    Account(EmailAddress),
    /// A calendar that is no account's own, or a path that names none.
    Other,
}

/// What `url` names, when Google serves it.
#[must_use]
pub fn linked(url: &PluginUrl) -> Option<Linked> {
    (url.origin().host() == HOST).then(|| {
        url.rest()
            .strip_prefix("/calendar/ical/")
            .and_then(|path| path.split('/').next())
            .and_then(decoded)
            .filter(|id| !id.ends_with(SHARED))
            .and_then(|id| EmailAddress::new(&id).ok())
            .map_or(Linked::Other, Linked::Account)
    })
}

/// `escaped` with every `%XX` read as its byte; `None` unless the bytes are UTF-8.
fn decoded(escaped: &str) -> Option<String> {
    let bytes = escaped.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An account's own calendar names its account; a shared calendar, a path that names no
    /// calendar and another host do not.
    #[test]
    fn a_google_link_names_its_account_s_own_calendar() {
        let read = |text: &str| linked(&PluginUrl::parse(text).expect("a link"));
        assert_eq!(
            read(
                "https://calendar.google.com/calendar/ical/ada%40example.test/private-7f3a/basic.ics"
            ),
            Some(Linked::Account(
                EmailAddress::new("ada@example.test").expect("an address")
            ))
        );
        for other in [
            "https://calendar.google.com/calendar/ical/c_1f2e%40group.calendar.google.com/private-7f3a/basic.ics",
            "https://calendar.google.com/calendar/ical/work/private-7f3a/basic.ics",
            "https://calendar.google.com/",
        ] {
            assert_eq!(read(other), Some(Linked::Other), "{other}");
        }
        assert_eq!(
            read("https://outlook.office365.com/owa/calendar/a/b/calendar.ics"),
            None
        );
    }
}
