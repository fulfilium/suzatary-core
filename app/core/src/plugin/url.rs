//! A URL a plugin may fetch, parsed once and reprinted, so what is sent is what was checked.

use std::fmt;

use super::Origin;

/// The longest URL accepted.
pub const MAX_URL: usize = 8192;

/// An allowed-grammar `http(s)` URL: an [`Origin`], then a path and a query.
///
/// It is sent as [`fmt::Display`] prints it, the canonical origin followed by the rest, never as
/// the text it was parsed from: a transport's own URL parser then reads the same host this one
/// did. A fragment, userinfo, a backslash, whitespace and anything outside RFC 3986's printable
/// characters are refused rather than normalised.
#[derive(Clone, PartialEq, Eq)]
pub struct PluginUrl {
    origin: Origin,
    rest: String,
}

impl PluginUrl {
    /// Parse one, or `None`; the caller decides whether the text may be named in its error.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > MAX_URL {
            return None;
        }
        let (origin, rest) = Origin::split(text)?;
        let rest = match rest.as_bytes().first() {
            None => "/".to_owned(),
            Some(b'/') => rest.to_owned(),
            Some(b'?') => format!("/{rest}"),
            Some(_) => return None,
        };
        printable(&rest).then_some(Self { origin, rest })
    }

    /// Its path and query, `/`-rooted.
    #[must_use]
    pub fn rest(&self) -> &str {
        &self.rest
    }

    /// Its origin, which the manifest must allow.
    #[must_use]
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
}

/// `/`-rooted path and query characters, with every `%` the start of a two-digit escape.
fn printable(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?%".contains(&b);
    bytes.iter().copied().all(allowed)
        && bytes.iter().enumerate().all(|(at, &b)| {
            b != b'%'
                || bytes
                    .get(at + 1..at + 3)
                    .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
        })
}

impl fmt::Display for PluginUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.origin, self.rest)
    }
}

/// Only the origin: a secret URL's path is the secret.
impl fmt::Debug for PluginUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PluginUrl({}/…)", self.origin)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_url_is_reprinted_from_what_was_checked() {
        let printed = |text: &str| PluginUrl::parse(text).map(|url| url.to_string());
        assert_eq!(
            printed("HTTPS://Calendar.Google.com:443/calendar/ical/a%40b/basic.ics?x=1"),
            None,
            "the scheme is matched as written, lowercase"
        );
        assert_eq!(
            printed("https://Calendar.Google.com:443/calendar/ical/a%40b/basic.ics?x=1"),
            Some("https://calendar.google.com/calendar/ical/a%40b/basic.ics?x=1".to_owned())
        );
        assert_eq!(
            printed("http://127.0.0.1:8080"),
            Some("http://127.0.0.1:8080/".to_owned())
        );
        assert_eq!(
            printed("http://127.0.0.1:8080?q=1"),
            Some("http://127.0.0.1:8080/?q=1".to_owned())
        );
        assert_eq!(
            printed("https://a.example/user@host/path"),
            Some("https://a.example/user@host/path".to_owned()),
            "an @ after the authority is a path character"
        );
    }

    #[test]
    fn what_two_parsers_could_disagree_on_is_refused() {
        for bad in [
            "https://allowed.example@evil.example/",
            "https://evil.example\\@allowed.example/",
            "https://allowed.example/a\\b",
            "https://allowed.example/#frag",
            "https://allowed.example/ space",
            "https://allowed.example/%zz",
            "https://allowed.example/%4",
            "https://allowed.example/é",
            "https://allowed.example/\r\n",
            "https://allowed.example:80:80/",
            "javascript:alert(1)",
            "//allowed.example/",
        ] {
            assert!(PluginUrl::parse(bad).is_none(), "{bad:?}");
        }
        assert!(PluginUrl::parse(&format!("https://a.example/{}", "a".repeat(MAX_URL))).is_none());
    }

    #[test]
    fn debug_never_prints_the_path() {
        let url =
            PluginUrl::parse("https://calendar.example/private-s3cret/basic.ics").expect("a URL");
        let shown = format!("{url:?}");
        assert!(!shown.contains("s3cret"), "{shown}");
    }

    proptest! {
        /// Reprinting is idempotent: a printed URL parses back to itself.
        #[test]
        fn a_printed_url_parses_to_itself(
            host in "[a-z][a-z0-9]{0,8}(\\.[a-z][a-z0-9]{0,8}){0,2}",
            port in proptest::option::of(1..=u16::MAX),
            rest in "(/[A-Za-z0-9._~!$&'()*+,;=:@-]{0,8}){0,3}(\\?[A-Za-z0-9=&]{0,8})?",
        ) {
            let port = port.map(|port| format!(":{port}")).unwrap_or_default();
            let url = PluginUrl::parse(&format!("https://{host}{port}{rest}"));
            prop_assert!(url.is_some());
            let url = url.expect("checked above");
            prop_assert_eq!(PluginUrl::parse(&url.to_string()), Some(url));
        }

        /// The parser answers every string and never panics.
        #[test]
        fn parse_is_total(text in ".{0,80}") {
            let _ = PluginUrl::parse(&text);
        }
    }
}
