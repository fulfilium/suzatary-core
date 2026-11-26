//! The one recogniser of an email address, parsed once into an [`Address`].
//!
//! It reads what a mail reader shows as an address: a local part (a run of letters, digits and
//! the characters RFC 5322 allows, in any script, or a quoted string), an at sign (`@`, `＠` or
//! `﹫`), and a dotted domain of letters in any script or an IP literal. Format characters
//! (zero-width spaces, joiners, soft hyphens) are invisible, so they may sit anywhere inside a
//! word or beside the at sign. White space beside the at sign is read, and the answer says so.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// How the local part and the domain meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// The at sign touches both words, as a mail client needs it.
    Tight,
    /// White space stands beside the at sign: `ada @ example.test`.
    Spaced,
}

/// One address in a text, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address<'t> {
    range: Range<usize>,
    domain: &'t str,
    join: Join,
}

impl<'t> Address<'t> {
    /// The byte range of the whole address in the text.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// The domain as written: dotted names, or an IP literal in brackets.
    #[must_use]
    pub fn domain(&self) -> &'t str {
        self.domain
    }

    #[must_use]
    pub fn join(&self) -> Join {
        self.join
    }
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    let word = r"\p{L}\p{M}\p{N}";
    let atext = format!(r"{word}!#$%&'*+=?^_`{{|}}~.\-．");
    let invisible = r"\p{Cf}";
    let white = r"\p{Z}\p{Cc}";
    let gap = format!("[{white}{invisible}]");
    let at = "[@＠﹫]";
    let local = format!(r#""(?:[^"\\]|\\(?s:.))*"|[{atext}][{atext}{invisible}]*"#);
    let label = format!(r"{invisible}*[{word}\-][{word}\-{invisible}]*");
    let names = format!(r"{label}(?:[.．。｡]{label})+");
    let literal = r"\[(?:[Ii][Pp][Vv]6:)?[0-9A-Fa-f]*[.:][0-9A-Fa-f:.]*\]";
    Regex::new(&format!(
        "(?:{local})(?:(?P<tight>{at})|(?P<spaced>{gap}*[{white}]{invisible}*{at}{gap}*|{at}{invisible}*[{white}]{gap}*))(?P<domain>{names}|{literal})"
    ))
    .expect("a literal pattern")
});

fn read<'t>(found: &Captures<'t>) -> Address<'t> {
    let whole = found.get(0).expect("a match has a whole");
    let domain = found.name("domain").expect("the domain group takes part");
    Address {
        range: whole.range(),
        domain: domain.as_str(),
        join: if found.name("tight").is_some() {
            Join::Tight
        } else {
            Join::Spaced
        },
    }
}

/// Every address in `text`, in order.
pub fn addresses(text: &str) -> impl Iterator<Item = Address<'_>> {
    EMAIL.captures_iter(text).map(|found| read(&found))
}

/// Whether `text` holds an address. The pattern has no anchor, so a text that holds none has no
/// part that holds one.
#[must_use]
pub fn holds_address(text: &str) -> bool {
    EMAIL.is_match(text)
}

/// `text` with every address replaced by `with`.
#[must_use]
pub fn replace_addresses<'t>(text: &'t str, with: &str) -> Cow<'t, str> {
    let mut found = addresses(text).peekable();
    if found.peek().is_none() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for address in found {
        let range = address.range();
        out.push_str(&text[from..range.start]);
        out.push_str(with);
        from = range.end;
    }
    out.push_str(&text[from..]);
    Cow::Owned(out)
}
