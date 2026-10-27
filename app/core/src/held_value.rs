//! A held value (`docs/HELDVALUES-PLAN-2026-10-22.md`): a number in mail that [`crate::redact`]
//! recognises, which the box keeps where it is and a model reads only as a [`Placeholder`].
//!
//! Every text a model reads goes through [`mask`], with the message it came from, or through
//! [`withheld`] when no message is known. A placeholder names the number's kind and, for every
//! kind but a code, its last digits, never the number. A [`Handle`] in a placeholder names the
//! message and the number's place in it. The owner's trusted device asks the box for that one
//! value on a tap, and a proposed email that carries the placeholder in its body has the value
//! written in by [`fill`] when the owner taps Send.

use std::sync::LazyLock;

use base64::Engine as _;
use regex::Regex;
use serde_json::Value;

use crate::redact::{Kind, identity_numbers};

/// The text form's prefix.
pub const PREFIX: &str = "held:";

/// How many of a number's last digits a placeholder shows, for a kind that shows any.
pub const HINT_DIGITS: usize = 4;

/// The longest Message-ID a handle names: RFC 5322's line limit.
pub const MAX_MESSAGE: usize = 998;

/// One held value's name: the message it is in, and its place among that message's values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle {
    message: String,
    ordinal: u16,
}

/// A text that is no [`Handle`]'s text form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a held value's handle: {0}")]
pub struct NotAHandle(String);

impl Handle {
    /// The handle of the `ordinal`th value of `message`, a notmuch id.
    ///
    /// # Errors
    /// [`NotAHandle`] for an empty or overlong id.
    pub fn new(message: &str, ordinal: u16) -> Result<Self, NotAHandle> {
        if message.is_empty() || message.len() > MAX_MESSAGE {
            return Err(NotAHandle(format!(
                "a message id of {} bytes",
                message.len()
            )));
        }
        Ok(Self {
            message: message.to_owned(),
            ordinal,
        })
    }

    /// Read the text form [`Handle::print`] writes: `held:<message id in base64url>.<ordinal>`.
    ///
    /// # Errors
    /// [`NotAHandle`] for any other text.
    pub fn parse(text: &str) -> Result<Self, NotAHandle> {
        let refused = || NotAHandle(text.chars().take(64).collect());
        let rest = text.strip_prefix(PREFIX).ok_or_else(refused)?;
        let (message, ordinal) = rest.rsplit_once('.').ok_or_else(refused)?;
        let ordinal = ordinal.parse::<u16>().map_err(|_| refused())?;
        if ordinal.to_string() != rest[message.len() + 1..] {
            return Err(refused());
        }
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(message)
            .map_err(|_| refused())?;
        let message = String::from_utf8(bytes).map_err(|_| refused())?;
        Self::new(&message, ordinal).map_err(|_| refused())
    }

    /// The text form.
    #[must_use]
    pub fn print(&self) -> String {
        format!(
            "{PREFIX}{}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&self.message),
            self.ordinal
        )
    }

    /// The notmuch id of the message the value is in.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Its place among the message's [`Values`].
    #[must_use]
    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }
}

/// Whether a placeholder can be revealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// On a tap, by this handle.
    Revealable(Handle),
    /// Never: the text came from nothing the box can read again (Junk read from the server, a
    /// calendar, a page on the web), or its message no longer reads as holding it.
    Withheld,
}

/// What a model reads in a number's place.
///
/// A placeholder itself never reads as a number: [`crate::redact`] skips its hint, and a
/// handle's base64 of an ASCII Message-ID has a letter in every fourth place, so it holds no run
/// of four digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    kind: Kind,
    hint: String,
    reach: Reach,
}

/// How many digits a value needs before its placeholder shows its last [`HINT_DIGITS`]: a shorter
/// value, such as a code or a short account number, would show most of itself.
pub const MIN_HINTED: usize = 9;

/// A placeholder as written, its kind's name, hint and whatever stands where a handle goes: a
/// written placeholder whose handle does not decode is refused, never sent as its text.
static WRITTEN: LazyLock<Regex> = LazyLock::new(|| {
    let names: Vec<String> = Kind::ALL
        .iter()
        .map(|kind| regex::escape(kind.name()))
        .collect();
    Regex::new(&format!(
        r"\[({})(?: ending ([0-9]{{1,{HINT_DIGITS}}}))? · (?:tap to show · ([^\]]*)|withheld)\]",
        names.join("|")
    ))
    .expect("a pattern built from the kinds' literal names")
});

impl Placeholder {
    /// The placeholder for `value`, a number of `kind`.
    #[must_use]
    pub fn of(kind: Kind, value: &str, reach: Reach) -> Self {
        let digits: Vec<char> = value.chars().filter(char::is_ascii_digit).collect();
        let hint = if digits.len() >= MIN_HINTED {
            digits[digits.len() - HINT_DIGITS..].iter().collect()
        } else {
            String::new()
        };
        Self { kind, hint, reach }
    }

    /// The text form: `[SSN ending 3456 · tap to show · held:…]`, `[SSN ending 3456 ·
    /// withheld]`, or for a short value `[verification code · tap to show · held:…]`.
    #[must_use]
    pub fn print(&self) -> String {
        let reach = match &self.reach {
            Reach::Revealable(handle) => format!("tap to show · {}", handle.print()),
            Reach::Withheld => "withheld".to_owned(),
        };
        format!("[{} · {reach}]", self.described())
    }

    /// The kind and hint as a person reads them: `SSN ending 3456`, `verification code`.
    #[must_use]
    pub fn described(&self) -> String {
        if self.hint.is_empty() {
            self.kind.name().to_owned()
        } else {
            format!("{} ending {}", self.kind.name(), self.hint)
        }
    }

    /// What it is.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Whether a tap can reach it.
    #[must_use]
    pub fn reach(&self) -> &Reach {
        &self.reach
    }
}

/// One stretch of a [`Written`] text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text as written.
    Text(String),
    /// A placeholder, read.
    Held(Placeholder),
}

/// A text a model wrote, such as a proposed email's body, with each placeholder read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written(Vec<Part>);

/// A written placeholder whose handle is no handle's text form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} names no held value: its handle is not one the box printed")]
pub struct Malformed(pub String);

impl Written {
    /// Read `text`.
    ///
    /// # Errors
    /// [`Malformed`] for the first placeholder whose handle does not decode.
    pub fn parse(text: &str) -> Result<Self, Malformed> {
        let mut parts = Vec::new();
        let mut at = 0;
        for captures in WRITTEN.captures_iter(text) {
            let (Some(whole), Some(name)) = (captures.get(0), captures.get(1)) else {
                continue;
            };
            let Some(kind) = Kind::ALL
                .into_iter()
                .find(|kind| kind.name() == name.as_str())
            else {
                continue;
            };
            let reach = match captures.get(3) {
                Some(handle) => Reach::Revealable(
                    Handle::parse(handle.as_str())
                        .map_err(|_| Malformed(whole.as_str().to_owned()))?,
                ),
                None => Reach::Withheld,
            };
            if whole.start() > at {
                parts.push(Part::Text(text[at..whole.start()].to_owned()));
            }
            parts.push(Part::Held(Placeholder {
                kind,
                hint: captures.get(2).map_or("", |hint| hint.as_str()).to_owned(),
                reach,
            }));
            at = whole.end();
        }
        if at < text.len() {
            parts.push(Part::Text(text[at..].to_owned()));
        }
        Ok(Self(parts))
    }

    /// Its placeholders, in order.
    pub fn held(&self) -> impl Iterator<Item = &Placeholder> {
        self.0.iter().filter_map(|part| match part {
            Part::Held(placeholder) => Some(placeholder),
            Part::Text(_) => None,
        })
    }
}

/// `text` with every placeholder whose handle does not name `message` written as withheld: raw
/// mail never carries one the box printed, so such a placeholder is a sender's attempt to have
/// another message's value sent.
fn foreign_withheld(text: &str, message: &str) -> String {
    WRITTEN
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let whole = captures.get(0).map_or("", |whole| whole.as_str());
            let own = captures
                .get(3)
                .and_then(|handle| Handle::parse(handle.as_str()).ok())
                .is_some_and(|handle| handle.message() == message);
            match (own, captures.get(1)) {
                (false, Some(name)) => format!(
                    "[{}{} · withheld]",
                    name.as_str(),
                    captures
                        .get(2)
                        .map_or(String::new(), |hint| format!(" ending {}", hint.as_str()))
                ),
                (true, _) | (_, None) => whole.to_owned(),
            }
        })
        .into_owned()
}

/// One message's held numbers as written, each once with the kind it was first found as, in the
/// order they first appear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values(Vec<(Kind, String)>);

impl Values {
    /// The numbers in `texts`, read in order: one message's every decoded text.
    pub fn of<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut values: Vec<(Kind, String)> = Vec::new();
        for text in texts {
            for found in identity_numbers(text) {
                let value = &text[found.start..found.end];
                if !values.iter().any(|(_, held)| same(held, value)) {
                    values.push((found.kind, value.to_owned()));
                }
            }
        }
        Self(values)
    }

    /// The value at `ordinal`, and its kind.
    #[must_use]
    pub fn get(&self, ordinal: u16) -> Option<(Kind, &str)> {
        self.0
            .get(usize::from(ordinal))
            .map(|(kind, value)| (*kind, value.as_str()))
    }

    /// The placeholder of `value`, revealable by its place among them in `message`, of the kind
    /// it was first found as, so every place it is written prints alike.
    #[must_use]
    pub fn placeholder(&self, message: &str, value: &str) -> Option<Placeholder> {
        let at = self.0.iter().position(|(_, held)| same(held, value))?;
        let handle = Handle::new(message, u16::try_from(at).ok()?).ok()?;
        Some(Placeholder::of(
            self.0[at].0,
            value,
            Reach::Revealable(handle),
        ))
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each value as a card names it, in order, with the hint its placeholder shows.
    #[must_use]
    pub fn held(&self) -> Vec<Held> {
        self.0
            .iter()
            .map(|(kind, value)| Held {
                kind: *kind,
                hint: Placeholder::of(*kind, value, Reach::Withheld).hint,
            })
            .collect()
    }
}

/// One held value as a card names it: its kind and the hint its placeholder shows, never the
/// value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Held {
    /// What kind of number it is.
    pub kind: Kind,
    /// Its last [`HINT_DIGITS`] digits, `""` for a value too short to show any.
    pub hint: String,
}

impl Held {
    /// "SSN ending 3456", or "verification code".
    #[must_use]
    pub fn said(&self) -> String {
        Placeholder {
            kind: self.kind,
            hint: self.hint.clone(),
            reach: Reach::Withheld,
        }
        .described()
    }
}

/// `Held(kind, hint)`.
pub const HELD: prodrome_core::literal::codec::Constructor = ("Held", &["kind", "hint"]);

impl prodrome_core::literal::codec::Literal for Held {
    fn to_value(&self) -> prodrome_core::literal::Value {
        prodrome_core::literal::codec::record(
            HELD,
            vec![self.kind.to_value(), self.hint.to_value()],
        )
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        let fields = prodrome_core::literal::codec::Fields::of(value, HELD)?;
        Ok(Held {
            kind: fields.get("kind")?,
            hint: fields.get("hint")?,
        })
    }
}

/// Two spellings of one number: `000-12-3456` and `000 12 3456` are the same value.
fn same(a: &str, b: &str) -> bool {
    let kept = |text: &str| {
        text.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect::<String>()
    };
    kept(a) == kept(b)
}

/// `text` with each held number replaced by its placeholder, revealable when `values`, the
/// numbers of `message`, hold it, and every placeholder naming another message withheld.
#[must_use]
pub fn mask(text: &str, message: &str, values: &Values) -> String {
    replaced(&foreign_withheld(text, message), |kind, value| {
        values
            .placeholder(message, value)
            .unwrap_or_else(|| Placeholder::of(kind, value, Reach::Withheld))
    })
}

/// `text` with each held number replaced by a placeholder no tap reveals.
#[must_use]
pub fn withheld(text: &str) -> String {
    replaced(text, |kind, value| {
        Placeholder::of(kind, value, Reach::Withheld)
    })
}

fn replaced(text: &str, placeholder: impl Fn(Kind, &str) -> Placeholder) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for found in identity_numbers(text) {
        out.push_str(&text[at..found.start]);
        out.push_str(&placeholder(found.kind, &text[found.start..found.end]).print());
        at = found.end;
    }
    out.push_str(&text[at..]);
    out
}

/// Every string of `value`, object keys included, through [`withheld`]: the last pass over a
/// verb's answer before a model reads it, whatever the verb read.
#[must_use]
pub fn withheld_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(withheld(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(withheld_value).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (withheld(&key), withheld_value(value)))
                .collect(),
        ),
        other @ (Value::Null | Value::Bool(_) | Value::Number(_)) => other,
    }
}

/// A held value read back for the owner, which never prints in a log line or a `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Shown {
    kind: Kind,
    value: String,
}

impl Shown {
    /// The value, as the message writes it, and the kind the message holds it as.
    #[must_use]
    pub fn new(kind: Kind, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }

    /// The value, for the one answer that carries it.
    #[must_use]
    pub fn into_string(self) -> String {
        self.value
    }

    /// The placeholder the box prints for it.
    #[must_use]
    pub fn placeholder(&self, reach: Reach) -> Placeholder {
        Placeholder::of(self.kind, &self.value, reach)
    }
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shown(a held value)")
    }
}

/// What reading one handle found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revealed {
    /// The value.
    Value(Shown),
    /// The message is gone, or holds no value at that place any more.
    Gone,
}

/// Reading one held value back from the message that holds it, for the owner's tap alone. The
/// server's state holds it, and no record does, so no verb of the secretary or the agent can.
pub trait Reveal {
    /// The value `handle` names.
    ///
    /// # Errors
    /// The mail index's own failure.
    fn reveal(&self, handle: &Handle) -> Result<Revealed, crate::capability::MailError>;

    /// The message `message`, a notmuch id, unmasked, for the tap that forwards it, read only
    /// when its file is still `size` bytes long.
    ///
    /// # Errors
    /// The mail index's own failure.
    fn original(&self, message: &str, size: u64) -> Result<Reopened, crate::capability::MailError>;
}

/// What the tap's read of a forwarded message found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reopened {
    /// The corpus no longer holds it.
    Gone,
    /// Its file is no longer the size the proposal recorded, so it was not read.
    Resized,
    /// The file, read once.
    Read(Original),
}

/// A stored message with every held value in place, read only by the tap that forwards it. Every
/// field is read from `raw`, so the digest of `raw` vouches for all of them.
#[derive(Clone, PartialEq, Eq)]
pub struct Original {
    /// The message file's bytes, which an attached forward carries whole.
    pub raw: Vec<u8>,
    /// The headers a page shows.
    pub headers: Vec<crate::mail::MailHeader>,
    /// The body as text.
    pub body: String,
    /// Every attachment, in the file's order.
    pub parts: Vec<crate::mail::send::Attachment>,
}

impl std::fmt::Debug for Original {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Original({} bytes, unmasked)", self.raw.len())
    }
}

/// A draft's body with each held value written in, for the transport alone; it never prints in a
/// log line or a `{:?}`, and [`fill`] is its only constructor.
#[derive(Clone, PartialEq, Eq)]
pub struct Filled(String);

impl Filled {
    /// This body followed by `text`, which the tap read from a forwarded message's file and
    /// checked against the digest its card recorded.
    #[must_use]
    pub fn then(mut self, text: &str) -> Self {
        self.0.push_str(text);
        self
    }

    /// The body to put on the wire.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Debug for Filled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Filled(a body with its held values)")
    }
}

/// Why a body's held values could not be written in. Each names the value as its card did, never
/// the value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unfilled {
    /// A code, which the owner types themselves and the box never sends.
    #[error("a verification code is never sent: the owner types it where it is asked for")]
    Code,
    /// A placeholder no tap reaches.
    #[error("the {0} is withheld, so the box cannot write it into a message")]
    Withheld(String),
    /// Its message is gone, or holds no value at that place.
    #[error("the {0} is no longer held: its message is gone or no longer names it")]
    Gone(String),
    /// The value at that place is not the one the card described.
    #[error(
        "the message now holds a different value where the {0} was, so the card no longer describes what would be sent"
    )]
    Changed(String),
    /// The mail index failed.
    #[error("could not read the {0}: {1}")]
    Unreadable(String, String),
}

/// `written` with every placeholder replaced by the value it names, read now through `reveal`;
/// the owner's tap on Send is its only caller.
///
/// # Errors
/// [`Unfilled`] for the first placeholder that is a code, withheld, gone, changed or unreadable.
pub fn fill(written: &Written, reveal: &dyn Reveal) -> Result<Filled, Unfilled> {
    let mut out = String::new();
    for part in &written.0 {
        let placeholder = match part {
            Part::Text(text) => {
                out.push_str(text);
                continue;
            }
            Part::Held(placeholder) => placeholder,
        };
        let named = placeholder.described();
        let handle = match (&placeholder.kind, &placeholder.reach) {
            (Kind::Code, _) => return Err(Unfilled::Code),
            (_, Reach::Withheld) => return Err(Unfilled::Withheld(named)),
            (_, Reach::Revealable(handle)) => handle,
        };
        let shown = match reveal.reveal(handle) {
            Ok(Revealed::Value(shown)) => shown,
            Ok(Revealed::Gone) => return Err(Unfilled::Gone(named)),
            Err(failed) => return Err(Unfilled::Unreadable(named, failed.to_string())),
        };
        if shown.placeholder(placeholder.reach.clone()) != *placeholder {
            return Err(Unfilled::Changed(named));
        }
        out.push_str(&shown.value);
    }
    Ok(Filled(out))
}

/// A server that reads no mail: every handle is gone.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unread;

impl Reveal for Unread {
    fn reveal(&self, _: &Handle) -> Result<Revealed, crate::capability::MailError> {
        Ok(Revealed::Gone)
    }

    fn original(&self, _: &str, _: u64) -> Result<Reopened, crate::capability::MailError> {
        Ok(Reopened::Gone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Synthetic numbers no person holds: SSN area 000, ITIN area 912, an EIN of zeros.
    const SSN: &str = "000-12-3456";
    const ITIN: &str = "912-70-1234";

    #[test]
    fn a_placeholder_names_the_kind_and_last_digits_and_never_the_number() {
        let values = Values::of([format!("SSN {SSN}, ITIN {ITIN}").as_str()]);
        let masked = mask(
            &format!("mine is {SSN}; also 000 12 3456 and {ITIN}"),
            "form-1@example.test",
            &values,
        );
        let handle = |ordinal| {
            Handle::new("form-1@example.test", ordinal)
                .expect("a handle")
                .print()
        };
        assert_eq!(
            masked,
            format!(
                "mine is [SSN ending 3456 · tap to show · {}]; also [SSN ending 3456 · tap to \
                 show · {}] and [ITIN ending 1234 · tap to show · {}]",
                handle(0),
                handle(0),
                handle(1)
            )
        );
        assert!(!masked.contains(SSN) && !masked.contains(ITIN));
    }

    #[test]
    fn a_number_the_message_does_not_hold_is_withheld() {
        let masked = mask(&format!("SSN {SSN}"), "x@example.test", &Values::default());
        assert_eq!(masked, "SSN [SSN ending 3456 · withheld]");
        assert_eq!(
            withheld(&format!("ITIN {ITIN}")),
            "ITIN [ITIN ending 1234 · withheld]"
        );
    }

    #[test]
    fn text_with_no_number_is_unchanged() {
        let text = "Order 123456789 ships 2026-10-21; call 555 0100.";
        assert_eq!(mask(text, "x@example.test", &Values::of([text])), text);
        assert_eq!(withheld(text), text);
    }

    /// A placeholder reads as no number, so masking twice changes nothing, for every kind.
    #[test]
    fn a_masked_text_masks_to_itself() {
        let text = format!(
            "SSN {SSN}; your verification code is 482913; card 4111 1111 1111 1111; account \
             000123456789, routing 021000021; IBAN GB82 WEST 1234 5698 7654 32"
        );
        let values = Values::of([text.as_str()]);
        let once = mask(&text, "x@example.test", &values);
        assert_eq!(
            Written::parse(&once).expect("written").held().count(),
            6,
            "{once}"
        );
        assert!(!once.contains("482913"), "{once}");
        assert_eq!(mask(&once, "x@example.test", &values), once);
        assert_eq!(withheld(&once), once);
        assert_eq!(withheld(&withheld(&text)), withheld(&text));
    }

    /// The value and a body that holds it print as neither in a log line nor a `{:?}`.
    #[test]
    fn a_read_value_and_a_filled_body_never_print() {
        struct Holds;
        impl Reveal for Holds {
            fn reveal(&self, _: &Handle) -> Result<Revealed, crate::capability::MailError> {
                Ok(Revealed::Value(Shown::new(Kind::Ssn, SSN)))
            }
            fn original(&self, _: &str, _: u64) -> Result<Reopened, crate::capability::MailError> {
                Ok(Reopened::Gone)
            }
        }
        let handle = Handle::new("x@example.test", 0).expect("a handle");
        let body = format!(
            "It is {}.",
            Placeholder::of(Kind::Ssn, SSN, Reach::Revealable(handle)).print()
        );
        let filled = fill(&Written::parse(&body).expect("written"), &Holds).expect("filled");
        let printed = format!(
            "{filled:?} {:?}",
            Holds.reveal(&Handle::new("x@example.test", 0).expect("a handle"))
        );
        assert!(!printed.contains(SSN), "{printed}");
        assert_eq!(filled.into_string(), format!("It is {SSN}."));
    }

    #[test]
    fn every_string_of_an_answer_is_withheld() {
        let answer = serde_json::json!({
            "subject": format!("SSN {SSN}"),
            "rows": [{"body": format!("ITIN {ITIN}"), "n": 3}],
            format!("SSN {SSN}"): true,
        });
        let text = withheld_value(answer).to_string();
        assert!(!text.contains(SSN) && !text.contains(ITIN), "{text}");
        assert!(text.contains("[ITIN ending 1234 · withheld]"), "{text}");
    }

    #[test]
    fn a_handle_refuses_what_it_does_not_print() {
        for text in [
            "",
            "held:",
            "held:.0",
            "held:YQ",
            "held:YQ.x",
            "held:YQ.01",
            "held:YQ.70000",
            "mail:YQ.0",
            "held:!!.0",
        ] {
            assert!(Handle::parse(text).is_err(), "{text}");
        }
        assert!(Handle::new("", 0).is_err());
        assert!(Handle::new(&"a".repeat(MAX_MESSAGE + 1), 0).is_err());
    }

    proptest! {
        /// A placeholder's text form is read back as the same placeholder, whatever its kind.
        #[test]
        fn a_placeholder_round_trips(
            at in 0..Kind::ALL.len(),
            value in "[0-9]{9,19}",
            message in "[ -~]{1,80}",
            ordinal in any::<u16>(),
            revealable in any::<bool>(),
        ) {
            let reach = if revealable {
                Reach::Revealable(Handle::new(&message, ordinal).expect("a handle"))
            } else {
                Reach::Withheld
            };
            let placeholder = Placeholder::of(Kind::ALL[at], &value, reach);
            let text = format!("a {} b", placeholder.print());
            prop_assert_eq!(
                Written::parse(&text).expect("written").held().cloned().collect::<Vec<_>>(),
                vec![placeholder]
            );
        }

        /// The text form prints and parses back to the same handle.
        #[test]
        fn a_handle_round_trips(message in "[ -~]{1,200}", ordinal in any::<u16>()) {
            let handle = Handle::new(&message, ordinal).expect("a handle");
            prop_assert_eq!(Handle::parse(&handle.print()), Ok(handle));
        }
    }
}
