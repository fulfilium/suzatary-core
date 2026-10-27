//! What one consent asks Google for, and where Google sends the owner back.

use std::collections::BTreeSet;

use prodrome_core::literal::{ProdromeError, Value};

use prodrome_core::literal::codec::Literal;

/// One part of a Google account the box reads, each the set of Google scopes it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// The mail: what lieer's sync of Gmail asks for, which the box's sync hands it.
    Mail,
    /// The calendars, read, and the events of the calendars the account owns, written.
    Calendar,
}

/// Asked beside every consent, so the grant names the account it was given for.
const IDENTITY: [&str; 2] = ["openid", "email"];

impl Scope {
    /// Every part, in the order a card lists them.
    pub const ALL: [Scope; 2] = [Scope::Mail, Scope::Calendar];

    /// The Google scopes it needs; a grant holds the part only when it holds every one.
    #[must_use]
    pub const fn urls(self) -> &'static [&'static str] {
        match self {
            // lieer's own three (`lieer/remote.py`'s `Remote.SCOPES`), so the grant the box
            // writes for it is the grant lieer asks for. gmail.modify also sends, as lieer's own
            // sign-in always has: the Gmail transport borrows lieer's grant.
            Scope::Mail => &[
                "https://www.googleapis.com/auth/gmail.readonly",
                "https://www.googleapis.com/auth/gmail.labels",
                "https://www.googleapis.com/auth/gmail.modify",
            ],
            // Every calendar read; an event written only into a calendar the account owns, which
            // its primary calendar, the one the box writes to, is.
            Scope::Calendar => &[
                "https://www.googleapis.com/auth/calendar.readonly",
                "https://www.googleapis.com/auth/calendar.events.owned",
            ],
        }
    }

    /// Its name, as a stored act spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Scope::Mail => "mail",
            Scope::Calendar => "calendar",
        }
    }

    /// What it reaches, as a card says it.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Scope::Mail => "its mail",
            Scope::Calendar => "its calendars (view and edit)",
        }
    }

    /// The part `text` names.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.as_str() == text)
    }
}

/// A set of parts: what a consent asks for, or what a grant holds. Never empty when asked.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Scopes(BTreeSet<Scope>);

impl Scopes {
    /// Every part: what a Google account's connection asks for.
    #[must_use]
    pub fn all() -> Self {
        Self(Scope::ALL.into_iter().collect())
    }

    /// The parts named, `None` for none, since a consent for nothing asks Google nothing.
    #[must_use]
    pub fn of(parts: impl IntoIterator<Item = Scope>) -> Option<Self> {
        let parts: BTreeSet<Scope> = parts.into_iter().collect();
        (!parts.is_empty()).then_some(Self(parts))
    }

    /// The parts a grant holds, read from the space-separated `scope` Google answered with: a
    /// part is held when every scope it needs is there. Google lets a person untick a part on
    /// its consent page, so this can hold less than was asked.
    #[must_use]
    pub fn granted(scope: &str) -> BTreeSet<Scope> {
        let held: BTreeSet<&str> = scope.split_ascii_whitespace().collect();
        Scope::ALL
            .into_iter()
            .filter(|part| part.urls().iter().all(|url| held.contains(url)))
            .collect()
    }

    /// The `scope` parameter that asks for these parts: the account's identity, then each part's
    /// scopes.
    #[must_use]
    pub fn requested(&self) -> String {
        IDENTITY
            .into_iter()
            .chain(self.0.iter().flat_map(|part| part.urls().iter().copied()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Its parts `held` does not hold, `None` when it holds them all.
    #[must_use]
    pub fn missing(&self, held: &Scopes) -> Option<Scopes> {
        Self::of(self.iter().filter(|part| !held.contains(*part)))
    }

    /// Whether it asks for `part`.
    #[must_use]
    pub fn contains(&self, part: Scope) -> bool {
        self.0.contains(&part)
    }

    /// Its parts, in order.
    pub fn iter(&self) -> impl Iterator<Item = Scope> + '_ {
        self.0.iter().copied()
    }

    /// What it reaches, as a card says it: "its mail and its calendars (view and edit)".
    #[must_use]
    pub fn sentence(&self) -> String {
        let parts: Vec<&str> = self.iter().map(Scope::sentence).collect();
        match parts.split_last() {
            Some((last, [])) => (*last).to_owned(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => String::new(),
        }
    }
}

prodrome_core::word_literal!(Scope: Scope::ALL => |scope| scope.as_str());

impl Literal for Scopes {
    fn to_value(&self) -> Value {
        self.iter().collect::<Vec<_>>().to_value()
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        Self::of(Vec::<Scope>::from_value(value)?)
            .ok_or_else(|| ProdromeError::invalid("a consent asks for some scope"))
    }
}

impl serde::Serialize for Scopes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter().map(Scope::as_str))
    }
}

impl<'de> serde::Deserialize<'de> for Scopes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let parts = Vec::<String>::deserialize(deserializer)?
            .iter()
            .map(|text| {
                Scope::parse(text)
                    .ok_or_else(|| serde::de::Error::custom(format!("{text:?} is no Google scope")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::of(parts).ok_or_else(|| serde::de::Error::custom("a consent asks for some scope"))
    }
}

/// Where Google sends the owner back: the app's own origin and [`super::CALLBACK`], exactly,
/// since Google compares it byte for byte with what the client lists (RFC 9700 §2.1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RedirectUri(String);

impl RedirectUri {
    /// The callback on the origin of `app_url`, the URL the instance serves the app at: https,
    /// or plain http on a loopback host (RFC 8252 §7.3), which only a test serves.
    ///
    /// # Errors
    /// Why `app_url` names no such origin.
    pub fn of_app(app_url: &str) -> Result<Self, String> {
        let (scheme, rest) = app_url
            .split_once("://")
            .ok_or_else(|| format!("{app_url:?} is no URL"))?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host = authority
            .rsplit_once(':')
            .map_or(authority, |(host, port)| {
                if port.bytes().all(|byte| byte.is_ascii_digit()) {
                    host
                } else {
                    authority
                }
            });
        let named = !host.is_empty()
            && !authority.contains('@')
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-');
        let loopback = matches!(host, "127.0.0.1" | "localhost");
        let callback = || {
            Self(format!(
                "{scheme}://{}{}",
                authority.to_ascii_lowercase(),
                super::CALLBACK
            ))
        };
        match scheme {
            _ if !named => Err(format!(
                "{app_url:?} names no origin Google can send the owner back to"
            )),
            "https" => Ok(callback()),
            "http" if loopback => Ok(callback()),
            "http" => Err(format!(
                "Google sends the owner back over https, and {app_url} is plain http"
            )),
            _ => Err(format!(
                "{app_url:?} names no origin Google can send the owner back to"
            )),
        }
    }

    /// As the consent's URL, the token request and the client's list spell it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RedirectUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Literal for RedirectUri {
    fn to_value(&self) -> Value {
        Value::str(&self.0)
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let text = String::from_value(value)?;
        let read = text
            .strip_suffix(super::CALLBACK)
            .ok_or_else(|| ProdromeError::invalid(format!("{text:?} is not the callback")))
            .and_then(|origin| Self::of_app(origin).map_err(ProdromeError::invalid))?;
        if read.0 == text {
            Ok(read)
        } else {
            Err(ProdromeError::invalid(format!(
                "{text:?} is not spelt as the box spells a callback"
            )))
        }
    }
}

impl serde::Serialize for RedirectUri {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for RedirectUri {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_value(&Value::str(&text)).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The callback is the app's origin and the fixed path, whatever path the app URL names, and
    /// only https leaves a loopback host.
    #[test]
    fn the_redirect_is_the_apps_origin_and_the_callback() {
        for (app, redirect) in [
            (
                "https://box.example.test/",
                "https://box.example.test/api/oauth/google/callback",
            ),
            (
                "https://Box.Example.test:8443/#/inbox",
                "https://box.example.test:8443/api/oauth/google/callback",
            ),
            (
                "http://127.0.0.1:8787",
                "http://127.0.0.1:8787/api/oauth/google/callback",
            ),
        ] {
            assert_eq!(
                RedirectUri::of_app(app).map(|uri| uri.0),
                Ok(redirect.to_owned())
            );
        }
        for app in [
            "http://box.example.test/",
            "box.example.test",
            "https://user@box.example.test/",
            "https:///",
            "javascript://x",
        ] {
            assert!(RedirectUri::of_app(app).is_err(), "{app}");
        }
    }

    /// A stored callback reads back only as the box spells one.
    #[test]
    fn a_stored_redirect_round_trips_and_nothing_else_reads() {
        let uri = RedirectUri::of_app("https://box.example.test").expect("a callback");
        assert_eq!(RedirectUri::from_value(&uri.to_value()), Ok(uri));
        for text in [
            "https://box.example.test/elsewhere",
            "https://BOX.example.test/api/oauth/google/callback",
            "http://box.example.test/api/oauth/google/callback",
        ] {
            assert!(
                RedirectUri::from_value(&Value::str(text)).is_err(),
                "{text}"
            );
        }
    }

    /// A part is granted only when every scope it needs is, so a person who unticks Calendar's
    /// write on Google's page holds no calendar.
    #[test]
    fn a_part_is_granted_only_whole() {
        let all = Scopes::all().requested();
        assert!(all.starts_with("openid email "), "{all}");
        assert_eq!(
            Scopes::granted(&all),
            Scope::ALL.into_iter().collect::<BTreeSet<_>>()
        );
        let without_events =
            all.replace("https://www.googleapis.com/auth/calendar.events.owned", "");
        assert_eq!(
            Scopes::granted(&without_events),
            BTreeSet::from([Scope::Mail])
        );
        assert!(Scopes::granted("openid email").is_empty());
        assert_eq!(Scopes::of([]), None);
    }

    #[test]
    fn scopes_round_trip_and_read_in_words() {
        let all = Scopes::all();
        assert_eq!(Scopes::from_value(&all.to_value()), Ok(all.clone()));
        assert_eq!(all.sentence(), "its mail and its calendars (view and edit)");
        assert!(Scopes::from_value(&Vec::<String>::new().to_value()).is_err());
        assert!(Scopes::from_value(&vec!["photos".to_owned()].to_value()).is_err());
    }
}
