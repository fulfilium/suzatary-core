//! What a log line may quote of a failure it did not write: a child's stderr, a provider's
//! error body, a browser's message.
//!
//! An [`Excerpt`] is bounded, holds no email address, IP address, link credential or JWT, and
//! prints as one line, so whatever a child or a provider says, the journal holds a few hundred
//! characters of it with nothing in them that names a person, a host or a key. Its constructors
//! are the only way to one, so a log site that quotes an `Excerpt` cannot quote the raw text.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

/// The most characters an excerpt keeps when its caller names no bound: enough for a failure's
/// last sentence, never a body.
pub(crate) const CHARS: usize = 500;

/// What an address becomes.
pub(crate) const ADDRESS: &str = "[address]";

/// Bounded, scrubbed words of a failure. [`fmt::Display`] prints them as one line, control
/// characters escaped; [`Excerpt::as_str`] keeps their line breaks for a reader that is not a
/// log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Excerpt(String);

/// An email address, loosely: a local part, `@`, a dotted domain.
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+\-]+@[a-z0-9\-]+(?:\.[a-z0-9\-]+)+").expect("a valid pattern")
});

/// What might be an IP address: dotted digits, or hex groups joined by colons. A candidate is
/// replaced only where it parses as one and, for IPv6, also holds a digit or three groups
/// ([`is_ip`]), so a clock's `12:30` and a Rust path's `a::b` or `core::excerpt` stay.
static IP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:\d{1,3}\.){3}\d{1,3}\b|(?:[0-9a-f]{0,4}:){2,7}[0-9a-f]{0,4}")
        .expect("a valid pattern")
});

fn is_ip(candidate: &str) -> bool {
    match candidate.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(_)) => true,
        // `e::e` parses, and is the tail of a Rust path far more often than an address.
        Ok(std::net::IpAddr::V6(_)) => {
            candidate.bytes().any(|b| b.is_ascii_digit())
                || candidate.split(':').filter(|g| !g.is_empty()).count() >= 3
        }
        Err(_) => false,
    }
}

impl Excerpt {
    /// The last `CHARS` characters of `text`, trimmed and scrubbed: where a failure says why.
    #[must_use]
    pub fn of(text: &str) -> Self {
        Self::tail(text, CHARS)
    }

    /// The last `most` characters of `text`, trimmed and scrubbed, `…` first when cut.
    #[must_use]
    pub fn tail(text: &str, most: usize) -> Self {
        let text = scrubbed(text.trim());
        let total = text.chars().count();
        if total <= most {
            return Excerpt(text);
        }
        Excerpt(
            std::iter::once('…')
                .chain(text.chars().skip(total - most))
                .collect(),
        )
    }

    /// The first `most` characters of `text`, scrubbed, `…` last when cut.
    #[must_use]
    pub fn head(text: &str, most: usize) -> Self {
        let text = scrubbed(text);
        if text.chars().count() <= most {
            return Excerpt(text);
        }
        Excerpt(
            text.chars()
                .take(most)
                .chain(std::iter::once('…'))
                .collect(),
        )
    }

    /// The words, line breaks kept.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Is there nothing to quote?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for Excerpt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in self.0.chars() {
            if c.is_control() {
                write!(f, "{}", c.escape_default())?;
            } else {
                write!(f, "{c}")?;
            }
        }
        Ok(())
    }
}

/// Scrubbed before it is cut, so a cut never leaves half an address to slip past the pattern.
fn scrubbed(text: &str) -> String {
    let text = crate::redact::unlink(text);
    let text = EMAIL.replace_all(&text, ADDRESS);
    IP.replace_all(&text, |found: &regex::Captures<'_>| {
        // A dotted quad after a `/` is a product's version (`Chrome/154.0.0.0`), not a host.
        let versioned = text[..found.get(0).map_or(0, |m| m.start())].ends_with('/');
        if is_ip(&found[0]) && !versioned {
            ADDRESS.to_owned()
        } else {
            found[0].to_owned()
        }
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn an_excerpt_names_no_address_host_or_key() {
        let said = Excerpt::of(concat!(
            "send to owner@example.invalid failed: connect 100.64.0.103:993 refused; ",
            "fe80::1:2:3 unreachable; retry https://x.example/cb?access_token=abc123 ",
            "with eyJhbGciOi.eyJzdWIi.c2ln at 12:30"
        ));
        let shown = said.to_string();
        for gone in ["owner@", "100.64", "fe80", "abc123", "eyJhbGciOi"] {
            assert!(!shown.contains(gone), "{gone} in {shown}");
        }
        assert!(
            shown.contains("12:30"),
            "a clock is not an address: {shown}"
        );
        assert!(shown.contains(ADDRESS));
    }

    #[test]
    fn a_rust_path_is_not_an_address() {
        let said = "failed in core::excerpt and a::b, from fe80::1 and ::1 and ab:cd:ef::";
        let shown = Excerpt::of(said).to_string();
        assert!(
            shown.contains("core::excerpt") && shown.contains("a::b"),
            "{shown}"
        );
        for gone in ["fe80::1", " ::1", "ab:cd:ef::"] {
            assert!(!shown.contains(gone), "{gone} in {shown}");
        }
    }

    #[test]
    fn a_product_version_is_not_an_address() {
        let agent = "Mozilla/5.0 (X11) HeadlessChrome/154.0.0.0 Safari/537.36 from 10.1.2.3";
        let shown = Excerpt::of(agent).to_string();
        assert!(shown.contains("HeadlessChrome/154.0.0.0"), "{shown}");
        assert!(!shown.contains("10.1.2.3"), "{shown}");
    }

    #[test]
    fn a_long_failure_keeps_its_end_and_a_long_message_its_start() {
        let long = format!("{}the reason", "x".repeat(10_000));
        let tail = Excerpt::of(&long);
        assert!(tail.as_str().ends_with("the reason"));
        assert!(tail.as_str().starts_with('…'));
        assert_eq!(tail.as_str().chars().count(), CHARS + 1);
        let head = Excerpt::head(&format!("the start{}", "y".repeat(10_000)), 20);
        assert_eq!(head.as_str(), "the startyyyyyyyyyyy…");
        assert_eq!(Excerpt::of("  short\n").as_str(), "short");
    }

    proptest! {
        /// Whatever a child prints, the excerpt is bounded and prints as one line.
        #[test]
        fn an_excerpt_is_bounded_and_one_line(text in ".{0,2000}", most in 1usize..600) {
            let tail = Excerpt::tail(&text, most);
            let head = Excerpt::head(&text, most);
            for one in [&tail, &head] {
                prop_assert!(one.as_str().chars().count() <= most + 1);
                prop_assert!(!one.to_string().chars().any(char::is_control));
            }
        }

        /// An address anywhere in a failure is gone from its excerpt, however it is cut.
        #[test]
        fn no_address_survives(
            before in "[a-z ]{0,600}",
            user in "[a-z]{1,12}",
            after in "[a-z ]{0,600}",
            most in 1usize..600,
        ) {
            let text = format!("{before} {user}@mail.example.org {after}");
            prop_assert!(!Excerpt::tail(&text, most).as_str().contains('@'));
            prop_assert!(!Excerpt::head(&text, most).as_str().contains('@'));
        }
    }
}
