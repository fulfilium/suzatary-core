//! An origin a plugin may reach: a scheme, a host and a port, and nothing else.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::ManifestError;

/// The longest DNS name.
const MAX_HOST: usize = 253;

/// The two schemes `fetch` speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scheme {
    /// Plain HTTP.
    Http,
    /// HTTP over TLS.
    Https,
}

impl Scheme {
    /// The port a URL means when it names none.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,
            Scheme::Https => 443,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }
}

/// `scheme://host[:port]`, canonical: the host lowercased, an IPv6 address bracketed, the
/// default port elided when printed, so two spellings of one origin are one value.
///
/// The grammar is narrower than a URL's on purpose: no userinfo, no backslash, nothing a second
/// URL parser could read as a different host than this one did.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct Origin {
    scheme: Scheme,
    host: String,
    port: u16,
}

impl Origin {
    /// Parse an origin as a manifest writes it, `https://host[:port]` with at most a `/` after.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let bad = || ManifestError::BadOrigin(text.to_owned());
        let (origin, rest) = Self::split(text).ok_or_else(bad)?;
        if rest.is_empty() || rest == "/" {
            Ok(origin)
        } else {
            Err(bad())
        }
    }

    /// Split a URL into its origin and everything after the authority, or `None` when the text
    /// is not `http(s)://` and an authority this grammar reads.
    pub(super) fn split(text: &str) -> Option<(Self, &str)> {
        let (scheme, rest) = if let Some(rest) = text.strip_prefix("https://") {
            (Scheme::Https, rest)
        } else {
            (Scheme::Http, text.strip_prefix("http://")?)
        };
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, after) = rest.split_at(end);
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !port.contains(']') => (host, Some(port)),
            _ => (authority, None),
        };
        let host = canonical_host(host)?;
        let port = match port {
            None => scheme.default_port(),
            Some(digits) => parse_port(digits)?,
        };
        Some((Self { scheme, host, port }, after))
    }

    /// The scheme.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The host, lowercased; an IPv6 address in brackets.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port, the scheme's default when the text named none.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Whether what is sent here stays between this box and the origin: TLS, or a loopback
    /// address that never leaves the box. A credential is sent nowhere else.
    #[must_use]
    pub fn is_confidential(&self) -> bool {
        let address = self.host.trim_start_matches('[').trim_end_matches(']');
        self.scheme == Scheme::Https || address.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }
}

impl TryFrom<String> for Origin {
    type Error = ManifestError;
    fn try_from(text: String) -> Result<Self, ManifestError> {
        Self::parse(&text)
    }
}

impl From<Origin> for String {
    fn from(origin: Origin) -> Self {
        origin.to_string()
    }
}

// A grant card stores the origin by its canonical print.
prodrome_core::text_literal!(Origin => ToString::to_string);

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.scheme.as_str(), self.host)?;
        if self.port != self.scheme.default_port() {
            write!(f, ":{}", self.port)?;
        }
        Ok(())
    }
}

/// A port of one to five digits, `1..=65535`, no sign and no leading zero.
fn parse_port(digits: &str) -> Option<u16> {
    let plain = !digits.is_empty()
        && digits.len() <= 5
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit());
    plain.then(|| digits.parse().ok()).flatten()
}

/// A DNS name of LDH labels, an IPv4 address or a bracketed IPv6 address, lowercased.
fn canonical_host(host: &str) -> Option<String> {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner
            .parse::<Ipv6Addr>()
            .ok()
            .map(|address| format!("[{address}]"));
    }
    let host = host.to_ascii_lowercase();
    if host.parse::<Ipv4Addr>().is_ok() {
        return Some(host);
    }
    let label = |label: &str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    let numeric_tail = host
        .rsplit('.')
        .next()
        .is_some_and(|last| last.bytes().all(|b| b.is_ascii_digit()));
    (host.len() <= MAX_HOST && host.split('.').all(label) && !numeric_tail).then_some(host)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn two_spellings_of_one_origin_are_one_value() {
        let plain = Origin::parse("https://calendar.google.com").expect("an origin");
        for same in [
            "https://Calendar.Google.com",
            "https://calendar.google.com:443",
            "https://calendar.google.com/",
        ] {
            assert_eq!(Origin::parse(same), Ok(plain.clone()), "{same}");
        }
        assert_eq!(plain.to_string(), "https://calendar.google.com");
        let other_port = Origin::parse("http://127.0.0.1:8080").expect("an origin");
        assert_eq!(other_port.to_string(), "http://127.0.0.1:8080");
        assert_ne!(
            Origin::parse("http://calendar.google.com").expect("an origin"),
            plain
        );
    }

    #[test]
    fn anything_another_parser_could_read_differently_is_refused() {
        for bad in [
            "https://user@calendar.google.com",
            "https://calendar.google.com@evil.example",
            "https://evil.example\\@calendar.google.com",
            "https://calendar.google.com/ical",
            "https://calendar.google.com#x",
            "https://calendar.google.com:0443",
            "https://calendar.google.com:99999",
            "https://calendar.google.com:",
            "https://calendar.google.com.",
            "https://-calendar.google.com",
            "https://calendar..google.com",
            "https://ca lendar.google.com",
            "https://0x7f.1",
            "https://127.1",
            "https://[::1",
            "ftp://calendar.google.com",
            "calendar.google.com",
            "https://",
        ] {
            assert!(Origin::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn only_tls_or_loopback_is_confidential() {
        for (text, confidential) in [
            ("https://calendar.example", true),
            ("http://127.0.0.1:9000", true),
            ("http://127.8.9.10", true),
            ("http://[::1]:9000", true),
            ("http://calendar.example", false),
            ("http://localhost:9000", false),
            ("http://10.0.0.1", false),
        ] {
            let origin = Origin::parse(text).expect("an origin");
            assert_eq!(origin.is_confidential(), confidential, "{text}");
        }
    }

    #[test]
    fn an_ipv6_origin_is_bracketed_and_canonical() {
        let origin = Origin::parse("http://[0:0:0:0:0:0:0:1]:9000").expect("an origin");
        assert_eq!(origin.host(), "[::1]");
        assert_eq!(origin.to_string(), "http://[::1]:9000");
    }

    fn origins() -> impl Strategy<Value = Origin> {
        let label = "[a-z][a-z0-9-]{0,10}[a-z0-9]";
        let name = proptest::collection::vec(label, 1..4).prop_map(|labels| labels.join("."));
        let ipv4 = any::<[u8; 4]>().prop_map(|b| Ipv4Addr::from(b).to_string());
        let ipv6 = any::<[u16; 8]>().prop_map(|s| format!("[{}]", Ipv6Addr::from(s)));
        let host = prop_oneof![name, ipv4, ipv6];
        let scheme = prop_oneof![Just(Scheme::Http), Just(Scheme::Https)];
        (scheme, host, 1..=u16::MAX).prop_map(|(scheme, host, port)| Origin { scheme, host, port })
    }

    proptest! {
        /// The law a canonical form owes: printing then parsing is the identity.
        #[test]
        fn print_then_parse_is_the_identity(origin in origins()) {
            prop_assert_eq!(Origin::parse(&origin.to_string()), Ok(origin));
        }

        /// The parser answers every string and never panics.
        #[test]
        fn parse_is_total(text in ".{0,64}") {
            let _ = Origin::parse(&text);
        }
    }
}
