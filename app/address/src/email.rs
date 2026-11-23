//! The one recogniser of an email address: a local part, `@`, a dotted domain, in ASCII.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[a-z0-9._%+\-]+@[a-z0-9\-]+(?:\.[a-z0-9\-]+)+").expect("a literal pattern")
});

/// Whether `text` holds an address. The pattern has no anchor, so a text that holds none has no
/// part that holds one.
#[must_use]
pub fn holds_address(text: &str) -> bool {
    EMAIL.is_match(text)
}

/// `text` with every address replaced by `with`.
#[must_use]
pub fn replace_addresses<'t>(text: &'t str, with: &str) -> Cow<'t, str> {
    EMAIL.replace_all(text, with)
}
