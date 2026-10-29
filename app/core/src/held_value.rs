//! A held value (`docs/HELDVALUES-PLAN-2026-10-22.md`): a number in mail or in a file that
//! [`crate::redact`] recognises, or a file the box holds back, which the box keeps where it is
//! and a model reads only as a [`Placeholder`].
//!
//! Every text a model reads goes through [`mask`], with the [`Holder`] it came from, or through
//! [`withheld`] when no holder is known. A placeholder names the value's kind and a hint, never
//! the value: a number's last digits, or a file's name and size. A [`Handle`] in a placeholder
//! names the holder and the value's place in it, or the file itself. The owner's trusted device
//! asks the box for that one value on a tap, and a proposed email that carries the placeholder in
//! its body has the value written in by [`fill`], or the file attached, when the owner taps Send.

use std::sync::LazyLock;

use base64::Engine as _;
use regex::Regex;
use serde_json::Value;

use crate::enclosed::Enclosure;
pub use crate::held_file::HeldFile;
use crate::redact::{Kind, identity_numbers};

/// The text form's prefix.
pub const PREFIX: &str = "held:";

/// The prefix of a file's part of a handle; a colon is no base64url character, so no message's
/// part starts with it.
const FILE: &str = "file:";

/// The place word of a handle that names a file itself.
const WHOLE: &str = "file";

/// How many of a number's last digits a placeholder shows, for a kind that shows any.
pub const HINT_DIGITS: usize = 4;

/// The longest Message-ID a handle names: RFC 5322's line limit.
pub const MAX_MESSAGE: usize = 998;

/// The longest handle text read, in bytes.
const MAX_HANDLE: usize = 4096;

/// The most characters of a file's name a placeholder shows.
const MAX_FILE_NAME: usize = 80;

/// What holds values in its text: a message, by its notmuch id, or a file, by the source the box
/// reads it again from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Holder {
    /// A message in the mail index.
    Message(String),
    /// A file's text: a PDF's text layer, or a text file's words.
    File(HeldFile),
}

impl Holder {
    /// Whether a handle may name it: a file whose source names a number (a Drive path or an
    /// attachment called `SSN 000-12-3456.pdf`) is never named, since a handle carries its source
    /// in base64, which no masker or log scan reads.
    #[must_use]
    pub fn named(&self) -> bool {
        match self {
            Self::Message(_) => true,
            Self::File(file) => identity_numbers(&file.print()).is_empty(),
        }
    }
}

/// One value's place: its holder, and its place among the holder's [`Values`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    holder: Holder,
    ordinal: u16,
}

impl Place {
    /// The `ordinal`th value of `holder`.
    ///
    /// # Errors
    /// [`NotAHandle`] for a message id that is empty or overlong.
    pub fn new(holder: Holder, ordinal: u16) -> Result<Self, NotAHandle> {
        match &holder {
            Holder::Message(message) if message.is_empty() || message.len() > MAX_MESSAGE => Err(
                NotAHandle(format!("a message id of {} bytes", message.len())),
            ),
            Holder::Message(_) | Holder::File(_) => Ok(Self { holder, ordinal }),
        }
    }

    /// What holds the value.
    #[must_use]
    pub fn holder(&self) -> &Holder {
        &self.holder
    }

    /// Its place among the holder's [`Values`].
    #[must_use]
    pub fn ordinal(&self) -> u16 {
        self.ordinal
    }
}

/// One held value's name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Handle {
    /// A number at its place.
    Value(Place),
    /// A file the box holds back, as itself.
    File(HeldFile),
}

/// A text that is no [`Handle`]'s text form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a held value's handle: {0}")]
pub struct NotAHandle(String);

fn base64(text: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(text)
}

fn unbase64(text: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(text)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn file_part(file: &HeldFile) -> String {
    format!("{FILE}{}", base64(&file.print()))
}

impl Handle {
    /// The handle of the `ordinal`th value of `message`, a notmuch id.
    ///
    /// # Errors
    /// [`NotAHandle`] for an empty or overlong id.
    pub fn new(message: &str, ordinal: u16) -> Result<Self, NotAHandle> {
        Place::new(Holder::Message(message.to_owned()), ordinal).map(Self::Value)
    }

    /// Read the text form [`Handle::print`] writes: `held:<message id in base64url>.<ordinal>`,
    /// `held:file:<the file's source in base64url>.<ordinal>`, or `held:file:<…>.file` for the
    /// file itself.
    ///
    /// # Errors
    /// [`NotAHandle`] for any other text.
    pub fn parse(text: &str) -> Result<Self, NotAHandle> {
        let refused = || NotAHandle(text.chars().take(64).collect());
        if text.len() > MAX_HANDLE {
            return Err(refused());
        }
        let rest = text.strip_prefix(PREFIX).ok_or_else(refused)?;
        let (holder, place) = rest.rsplit_once('.').ok_or_else(refused)?;
        let holder = match holder.strip_prefix(FILE) {
            Some(file) => {
                let printed = unbase64(file).ok_or_else(refused)?;
                Holder::File(HeldFile::parse(&printed).map_err(|_| refused())?)
            }
            None => Holder::Message(unbase64(holder).ok_or_else(refused)?),
        };
        match (holder, place) {
            (Holder::File(file), WHOLE) => Ok(Self::File(file)),
            (holder, place) => {
                let ordinal = place.parse::<u16>().map_err(|_| refused())?;
                if ordinal.to_string() != place {
                    return Err(refused());
                }
                Place::new(holder, ordinal)
                    .map(Self::Value)
                    .map_err(|_| refused())
            }
        }
    }

    /// The text form.
    #[must_use]
    pub fn print(&self) -> String {
        match self {
            Self::Value(Place {
                holder: Holder::Message(message),
                ordinal,
            }) => format!("{PREFIX}{}.{ordinal}", base64(message)),
            Self::Value(Place {
                holder: Holder::File(file),
                ordinal,
            }) => format!("{PREFIX}{}.{ordinal}", file_part(file)),
            Self::File(file) => format!("{PREFIX}{}.{WHOLE}", file_part(file)),
        }
    }
}

/// Whether a placeholder can be revealed: by `H`, a [`Place`] for a number and a file's source
/// for a file, or never.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach<H> {
    /// On a tap, by this.
    Revealable(H),
    /// Never: the text came from nothing the box can read again (Junk read from the server, a
    /// calendar, a page on the web), or its holder no longer reads as holding it.
    Withheld,
}

/// What a model reads in a value's place.
///
/// A placeholder itself never reads as a number: [`crate::redact`] skips a number's hint, a
/// file's name is shown with every number in it removed and its size never stands alone, and a
/// handle's base64 of ASCII text has a letter in every fourth place, so it holds no run of four
/// digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placeholder {
    /// A number.
    Number {
        /// What kind of number it is.
        kind: Kind,
        /// Its last [`HINT_DIGITS`] digits, `""` for a value too short to show any.
        digits: String,
        /// How a tap reaches it.
        reach: Reach<Place>,
    },
    /// A file.
    File {
        /// Its name, as [`Placeholder::file`] shows it.
        name: String,
        /// Its size, as [`file_size`] words it.
        size: String,
        /// How a tap reaches it.
        reach: Reach<HeldFile>,
    },
}

/// How many digits a value needs before its placeholder shows its last [`HINT_DIGITS`]: a shorter
/// value, such as a code or a short account number, would show most of itself.
pub const MIN_HINTED: usize = 9;

/// A placeholder as written: a number's kind and hint, or a file's name and size, and whatever
/// stands where a handle goes. A written placeholder whose handle does not decode is refused,
/// never sent as its text.
static WRITTEN: LazyLock<Regex> = LazyLock::new(|| {
    let names: Vec<String> = Kind::ALL
        .iter()
        .map(|kind| regex::escape(kind.name()))
        .collect();
    Regex::new(&format!(
        r"\[(?:({})(?: ending ([0-9]{{1,{HINT_DIGITS}}}))?|file ([^\[\]·\r\n]+?), (under 1 KB|[0-9]+\.[0-9] [KM]B)) · (?:tap to show · ([^\]]*)|withheld)\]",
        names.join("|")
    ))
    .expect("a pattern built from the kinds' literal names")
});

/// The last [`HINT_DIGITS`] digits of `value`, or none for a value under [`MIN_HINTED`] digits.
fn last_digits(value: &str) -> String {
    let digits: Vec<char> = value.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= MIN_HINTED {
        digits[digits.len() - HINT_DIGITS..].iter().collect()
    } else {
        String::new()
    }
}

/// A file's size as its placeholder words it: under 1 KB, or in tenths of a KB or MB, so its
/// digits always stand beside a dot and never read as a number.
#[must_use]
pub fn file_size(size: u64) -> String {
    if size < 1024 {
        "under 1 KB".to_owned()
    } else {
        crate::secretary::card::bytes(size)
    }
}

/// A file's name as a placeholder shows it: no bracket, middle dot or control character, and no
/// number [`crate::redact`] finds, each removed until none is left.
fn file_name(name: &str) -> String {
    let mut shown: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '[' | ']' | '·') {
                ' '
            } else {
                c
            }
        })
        .collect();
    loop {
        let found = identity_numbers(&shown);
        if found.is_empty() {
            break;
        }
        shown = replaced(&shown, |_, _| "…".to_owned());
    }
    let shown: String = shown
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_FILE_NAME)
        .collect();
    let shown = shown.trim_end_matches([',', ' ']);
    if shown.is_empty() {
        "unnamed".to_owned()
    } else {
        shown.to_owned()
    }
}

impl Placeholder {
    /// The placeholder for `value`, a number of `kind`.
    #[must_use]
    pub fn of(kind: Kind, value: &str, reach: Reach<Place>) -> Self {
        Self::Number {
            kind,
            digits: last_digits(value),
            reach,
        }
    }

    /// The placeholder of the file `source` reads, called `name`, of `size` bytes: revealable,
    /// unless its handle would carry a number (see [`Holder::named`]).
    #[must_use]
    pub fn file(source: &HeldFile, name: &str, size: u64) -> Self {
        let holder = Holder::File(source.clone());
        Self::File {
            name: file_name(name),
            size: file_size(size),
            reach: if holder.named() {
                Reach::Revealable(source.clone())
            } else {
                Reach::Withheld
            },
        }
    }

    /// The text form: `[SSN ending 3456 · tap to show · held:…]`, `[SSN ending 3456 ·
    /// withheld]`, `[verification code · tap to show · held:…]` for a short value, or `[file
    /// w2.pdf, 48.2 KB · tap to show · held:…]`.
    #[must_use]
    pub fn print(&self) -> String {
        let reach = match self.reach() {
            Reach::Revealable(handle) => format!("tap to show · {}", handle.print()),
            Reach::Withheld => "withheld".to_owned(),
        };
        format!("[{} · {reach}]", self.described())
    }

    /// The kind and hint as a person reads them: `SSN ending 3456`, `verification code`, `file
    /// w2.pdf, 48.2 KB`.
    #[must_use]
    pub fn described(&self) -> String {
        match self {
            Self::Number { kind, digits, .. } if digits.is_empty() => kind.name().to_owned(),
            Self::Number { kind, digits, .. } => format!("{} ending {digits}", kind.name()),
            Self::File { name, size, .. } => format!("file {name}, {size}"),
        }
    }

    /// Whether a tap can reach it, and by which handle.
    #[must_use]
    pub fn reach(&self) -> Reach<Handle> {
        match self {
            Self::Number {
                reach: Reach::Revealable(place),
                ..
            } => Reach::Revealable(Handle::Value(place.clone())),
            Self::File {
                reach: Reach::Revealable(file),
                ..
            } => Reach::Revealable(Handle::File(file.clone())),
            Self::Number {
                reach: Reach::Withheld,
                ..
            }
            | Self::File {
                reach: Reach::Withheld,
                ..
            } => Reach::Withheld,
        }
    }

    /// The kind and hint of one match of [`WRITTEN`], withheld; `None` for a kind's name no kind
    /// has, which the pattern built from the names never matches.
    fn hinted(captures: &regex::Captures<'_>) -> Option<Self> {
        let text = |at: usize| captures.get(at).map_or("", |part| part.as_str()).to_owned();
        Some(match captures.get(1) {
            Some(name) => Self::Number {
                kind: Kind::ALL
                    .into_iter()
                    .find(|kind| kind.name() == name.as_str())?,
                digits: text(2),
                reach: Reach::Withheld,
            },
            None => Self::File {
                name: text(3),
                size: text(4),
                reach: Reach::Withheld,
            },
        })
    }

    /// Read one match of [`WRITTEN`], its handle included.
    fn read(captures: &regex::Captures<'_>) -> Result<Self, Malformed> {
        let whole = captures.get(0).map_or("", |whole| whole.as_str());
        let malformed = || Malformed(whole.to_owned());
        let hinted = Self::hinted(captures).ok_or_else(malformed)?;
        let Some(handle) = captures.get(5) else {
            return Ok(hinted);
        };
        match (
            hinted,
            Handle::parse(handle.as_str()).map_err(|_| malformed())?,
        ) {
            (Self::Number { kind, digits, .. }, Handle::Value(place)) => Ok(Self::Number {
                kind,
                digits,
                reach: Reach::Revealable(place),
            }),
            (Self::File { name, size, .. }, Handle::File(file)) => Ok(Self::File {
                name,
                size,
                reach: Reach::Revealable(file),
            }),
            (Self::Number { .. }, Handle::File(_)) | (Self::File { .. }, Handle::Value(_)) => {
                Err(malformed())
            }
        }
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

/// A written placeholder whose handle is no handle's text form, or names a file where a number
/// stands or a number where a file stands.
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
            let Some(whole) = captures.get(0) else {
                continue;
            };
            let placeholder = Placeholder::read(&captures)?;
            if whole.start() > at {
                parts.push(Part::Text(text[at..whole.start()].to_owned()));
            }
            parts.push(Part::Held(placeholder));
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

/// `text` with every placeholder that is not a value of `holder` written as withheld: no message
/// or file the box reads carries one the box printed of another holder, nor a file's own, so such
/// a placeholder is a sender's attempt to have another holder's value or a file sent.
fn foreign_withheld(text: &str, holder: &Holder) -> String {
    WRITTEN
        .replace_all(text, |captures: &regex::Captures<'_>| {
            let whole = captures.get(0).map_or("", |whole| whole.as_str());
            match Placeholder::read(captures) {
                Ok(Placeholder::Number {
                    reach: Reach::Revealable(place),
                    ..
                }) if place.holder() == holder && holder.named() => whole.to_owned(),
                Ok(_) | Err(_) => Placeholder::hinted(captures)
                    .map_or_else(|| whole.to_owned(), |hinted| hinted.print()),
            }
        })
        .into_owned()
}

/// One holder's held numbers as written, each once with the kind it was first found as, in the
/// order they first appear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Values(Vec<(Kind, String)>);

impl Values {
    /// The numbers in `texts`, read in order: one holder's every decoded text.
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

    /// The placeholder of `value`, revealable by its place among them in `holder`, of the kind it
    /// was first found as, so every place it is written prints alike.
    #[must_use]
    pub fn placeholder(&self, holder: &Holder, value: &str) -> Option<Placeholder> {
        let at = self.0.iter().position(|(_, held)| same(held, value))?;
        if !holder.named() {
            return None;
        }
        let place = Place::new(holder.clone(), u16::try_from(at).ok()?).ok()?;
        Some(Placeholder::of(
            self.0[at].0,
            value,
            Reach::Revealable(place),
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
                hint: last_digits(value),
            })
            .collect()
    }
}

/// One held number as a card names it: its kind and the hint its placeholder shows, never the
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
        Placeholder::Number {
            kind: self.kind,
            digits: self.hint.clone(),
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
/// numbers of `holder`, hold it, and every placeholder not of `holder` withheld.
#[must_use]
pub fn mask(text: &str, holder: &Holder, values: &Values) -> String {
    replaced(&foreign_withheld(text, holder), |kind, value| {
        values
            .placeholder(holder, value)
            .unwrap_or_else(|| Placeholder::of(kind, value, Reach::Withheld))
            .print()
    })
}

/// `text` with each held number replaced by a placeholder no tap reveals.
#[must_use]
pub fn withheld(text: &str) -> String {
    replaced(text, |kind, value| {
        Placeholder::of(kind, value, Reach::Withheld).print()
    })
}

fn replaced(text: &str, placeholder: impl Fn(Kind, &str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for found in identity_numbers(text) {
        out.push_str(&text[at..found.start]);
        out.push_str(&placeholder(found.kind, &text[found.start..found.end]));
        at = found.end;
    }
    out.push_str(&text[at..]);
    out
}

/// A file's text as a model reads it: every number in it a placeholder naming the file, and
/// every placeholder of another holder withheld. [`Masked::of`] is its only constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Masked(String);

impl Masked {
    /// `text`, the words of the file `file` reads, masked.
    #[must_use]
    pub fn of(file: &HeldFile, text: &str) -> Self {
        Self(mask(text, &Holder::File(file.clone()), &Values::of([text])))
    }

    /// At most `cap` characters of it, and whether it was cut: cut after it was masked, so a number
    /// across the cut was found whole.
    #[must_use]
    pub fn clipped(self, cap: usize) -> (Self, bool) {
        let (text, truncated) = crate::mail::clip(&self.0, cap);
        (Self(text), truncated)
    }

    /// The text, safe to show.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The text, owned.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
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

/// A held number read back for the owner, which never prints in a log line or a `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Shown {
    kind: Kind,
    value: String,
}

impl Shown {
    /// The value, as its holder writes it, and the kind the holder holds it as.
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
    pub fn placeholder(&self, reach: Reach<Place>) -> Placeholder {
        Placeholder::of(self.kind, &self.value, reach)
    }
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shown(a held value)")
    }
}

/// A held file read back, for the owner or for the attachment that sends it, which never prints
/// its bytes in a log line or a `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Opened {
    /// Its name, as its source holds it.
    pub name: String,
    /// Its `type/subtype`.
    pub media: String,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Opened({}, {}, {} bytes)",
            self.name,
            self.media,
            self.bytes.len()
        )
    }
}

/// The largest file a reveal answers, in bytes: what one message may carry, before base64.
pub const MAX_OPENED: u64 = crate::secretary::attached::MAX_TOTAL;

/// What reading one handle found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Revealed {
    /// A number.
    Value(Shown),
    /// A file.
    File(Opened),
    /// A file over [`MAX_OPENED`], which a reveal does not answer, of this many bytes.
    TooLarge(u64),
    /// The holder is gone, or holds no value at that place any more.
    Gone,
}

/// Reading the mail's held values back, for the owner's tap alone. The server's state holds it,
/// and no record does, so no verb of the secretary or the agent can.
pub trait Reveal {
    /// The value at `ordinal` among the values of `message`, a notmuch id; `None` when the message
    /// is gone or holds no value there.
    ///
    /// # Errors
    /// The mail index's own failure.
    fn value(
        &self,
        message: &str,
        ordinal: u16,
    ) -> Result<Option<Shown>, crate::capability::MailError>;

    /// The message `message`, a notmuch id, unmasked, for the tap that forwards it, read only
    /// when its file is still `size` bytes long.
    ///
    /// # Errors
    /// The mail index's own failure.
    fn original(&self, message: &str, size: u64) -> Result<Reopened, crate::capability::MailError>;
}

/// The held files as a tap reads them: the file, and a PDF's text layer.
pub trait Files {
    /// The file `file` names, as its source holds it now; `None` when the source holds none.
    ///
    /// # Errors
    /// The source's own sentence.
    fn open(&self, file: &HeldFile) -> Result<Option<Opened>, String>;

    /// A PDF's text layer, read as the readers that masked a file's text read it.
    ///
    /// # Errors
    /// The reader's sentence.
    fn layer(&self, pdf: &[u8]) -> Result<String, String>;
}

/// Why a held value could not be read back: its holder's reader failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Unreadable(pub String);

/// What a tap reads a held value back through: the mail and the files. The route and the tap
/// hold one, and no record does.
#[derive(Clone, Copy)]
pub struct Holders<'a> {
    /// The mail's values.
    pub mail: &'a dyn Reveal,
    /// The files, by their sources, and the PDF reader.
    pub files: &'a dyn Files,
}

impl Holders<'_> {
    /// What `handle` names now.
    ///
    /// # Errors
    /// [`Unreadable`].
    pub fn read(&self, handle: &Handle) -> Result<Revealed, Unreadable> {
        match handle {
            Handle::Value(place) => Ok(self.value(place)?.map_or(Revealed::Gone, Revealed::Value)),
            Handle::File(file) => Ok(match self.files.open(file).map_err(Unreadable)? {
                None => Revealed::Gone,
                Some(opened) if opened.bytes.len() as u64 > MAX_OPENED => {
                    Revealed::TooLarge(opened.bytes.len() as u64)
                }
                Some(opened) => Revealed::File(opened),
            }),
        }
    }

    /// The number at `place`, read again from its holder as the mask read it; `None` when the
    /// holder holds none there.
    ///
    /// # Errors
    /// [`Unreadable`].
    pub fn value(&self, place: &Place) -> Result<Option<Shown>, Unreadable> {
        match place.holder() {
            Holder::Message(message) => self
                .mail
                .value(message, place.ordinal())
                .map_err(|failed| Unreadable(failed.to_string())),
            Holder::File(file) => {
                let Some(Opened { bytes, .. }) = self.files.open(file).map_err(Unreadable)? else {
                    return Ok(None);
                };
                let layer = match Enclosure::sniff(&bytes) {
                    Some(Enclosure::Pdf) => Some(self.files.layer(&bytes).map_err(Unreadable)?),
                    Some(Enclosure::Image(_)) | None => None,
                };
                Ok(crate::file_read::values(&bytes, layer.as_deref())
                    .get(place.ordinal())
                    .map(|(kind, value)| Shown::new(kind, value)))
            }
        }
    }
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

/// A draft's body with each held number written in, for the transport alone; it never prints in
/// a log line or a `{:?}`, and [`fill`] is its only constructor.
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
    /// Its holder is gone, or holds no value at that place.
    #[error("the {0} is no longer held: its message or file is gone or no longer names it")]
    Gone(String),
    /// The value at that place is not the one the card described.
    #[error(
        "the message or file now holds a different value where the {0} was, so the card no longer describes what would be sent"
    )]
    Changed(String),
    /// The holder's reader failed.
    #[error("could not read the {0}: {1}")]
    Unreadable(String, String),
}

/// `written` with every number's placeholder replaced by the value it names, read now through
/// `holders`, and every file's by the name of its record in `attached`, the draft's attachments,
/// which carry the file itself and which its card listed; a file's placeholder must still print
/// as its record does. The owner's tap on Send is its only caller.
///
/// # Errors
/// [`Unfilled`] for the first placeholder that is a code, withheld, gone, changed or unreadable.
pub fn fill(
    written: &Written,
    holders: &Holders<'_>,
    attached: &[crate::secretary::attached::Attached],
) -> Result<Filled, Unfilled> {
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
        let place = match placeholder {
            Placeholder::Number {
                kind: Kind::Code, ..
            } => return Err(Unfilled::Code),
            Placeholder::Number {
                reach: Reach::Withheld,
                ..
            }
            | Placeholder::File {
                reach: Reach::Withheld,
                ..
            } => return Err(Unfilled::Withheld(named)),
            Placeholder::File {
                reach: Reach::Revealable(file),
                ..
            } => {
                let source = file.source();
                let Some(one) = attached.iter().find(|one| one.source == source) else {
                    return Err(Unfilled::Gone(named));
                };
                if Placeholder::file(file, &one.name, one.size) != *placeholder {
                    return Err(Unfilled::Changed(named));
                }
                out.push_str(&one.name);
                continue;
            }
            Placeholder::Number {
                reach: Reach::Revealable(place),
                ..
            } => place,
        };
        let shown = match holders.value(place) {
            Ok(Some(shown)) => shown,
            Ok(None) => return Err(Unfilled::Gone(named)),
            Err(failed) => return Err(Unfilled::Unreadable(named, failed.to_string())),
        };
        if shown.placeholder(Reach::Revealable(place.clone())) != *placeholder {
            return Err(Unfilled::Changed(named));
        }
        out.push_str(&shown.value);
    }
    Ok(Filled(out))
}

/// A server that reads no mail: every value is gone.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unread;

impl Reveal for Unread {
    fn value(&self, _: &str, _: u16) -> Result<Option<Shown>, crate::capability::MailError> {
        Ok(None)
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

    fn message(id: &str) -> Holder {
        Holder::Message(id.to_owned())
    }

    fn notice() -> HeldFile {
        HeldFile::Mail {
            message: "form-1@example.test".to_owned(),
            name: "w2.pdf".to_owned(),
        }
    }

    #[test]
    fn a_placeholder_names_the_kind_and_last_digits_and_never_the_number() {
        let values = Values::of([format!("SSN {SSN}, ITIN {ITIN}").as_str()]);
        let masked = mask(
            &format!("mine is {SSN}; also 000 12 3456 and {ITIN}"),
            &message("form-1@example.test"),
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
    fn a_number_the_holder_does_not_hold_is_withheld() {
        let masked = mask(
            &format!("SSN {SSN}"),
            &message("x@example.test"),
            &Values::default(),
        );
        assert_eq!(masked, "SSN [SSN ending 3456 · withheld]");
        assert_eq!(
            withheld(&format!("ITIN {ITIN}")),
            "ITIN [ITIN ending 1234 · withheld]"
        );
    }

    #[test]
    fn text_with_no_number_is_unchanged() {
        let text = "Order 123456789 ships 2026-10-21; call 555 0100.";
        assert_eq!(
            mask(text, &message("x@example.test"), &Values::of([text])),
            text
        );
        assert_eq!(withheld(text), text);
    }

    /// A placeholder reads as no number, so masking twice changes nothing, for every kind and for
    /// a file whose name and size name numbers.
    #[test]
    fn a_masked_text_masks_to_itself() {
        let text = format!(
            "SSN {SSN}; your verification code is 482913; card 4111 1111 1111 1111; account \
             000123456789, routing 021000021; IBAN GB82 WEST 1234 5698 7654 32"
        );
        let values = Values::of([text.as_str()]);
        let holder = message("x@example.test");
        let once = mask(&text, &holder, &values);
        assert_eq!(
            Written::parse(&once).expect("written").held().count(),
            6,
            "{once}"
        );
        assert!(!once.contains("482913"), "{once}");
        assert_eq!(mask(&once, &holder, &values), once);
        assert_eq!(withheld(&once), once);
        assert_eq!(withheld(&withheld(&text)), withheld(&text));
        for (name, size) in [
            (format!("SSN {SSN}.pdf"), 900),
            ("2FA backup codes.txt".to_owned(), 1003),
            (
                "account 000123456789 [statement] · 2026.pdf".to_owned(),
                49_356,
            ),
            (String::new(), 25 * 1024 * 1024),
        ] {
            let file = Placeholder::file(&notice(), &name, size).print();
            assert!(!file.contains(SSN), "{file}");
            assert_eq!(withheld(&file), file);
            let text = format!("see {file}, and {file}");
            let mine = Masked::of(&notice(), &text);
            assert_eq!(Masked::of(&notice(), mine.as_str()), mine);
            assert_eq!(
                Written::parse(&text)
                    .expect("written")
                    .held()
                    .cloned()
                    .collect::<Vec<_>>(),
                vec![Placeholder::file(&notice(), &name, size); 2]
            );
        }
    }

    /// A file's text names its numbers by the file; a placeholder it carries of another holder,
    /// or of a file itself, is withheld, so a document cannot have another value or a file sent.
    /// A file whose source names a number is never named by a handle, which would carry the
    /// number in base64: the file and the numbers in its text are withheld.
    #[test]
    fn a_file_whose_source_names_a_number_is_withheld() {
        let named = HeldFile::Drive {
            path: format!("/my-files/Docs/SSN {SSN}.pdf")
                .parse()
                .expect("a path"),
        };
        assert!(!Holder::File(named.clone()).named());
        let file = Placeholder::file(&named, "w2.pdf", 4096).print();
        assert_eq!(file, "[file w2.pdf, 4.0 KB · withheld]");
        let masked = Masked::of(&named, &format!("ITIN {ITIN}"));
        assert_eq!(masked.as_str(), "ITIN [ITIN ending 1234 · withheld]");
    }

    #[test]
    fn a_file_s_numbers_name_the_file_and_a_foreign_placeholder_is_withheld() {
        let other = Handle::new("other@example.test", 0)
            .expect("a handle")
            .print();
        let file = Placeholder::file(&notice(), "w2.pdf", 4096).print();
        let text = format!(
            "Employee SSN {SSN}. Also [ITIN ending 1234 · tap to show · {other}] and {file}."
        );
        // Character for character the spelling `web/test/heldvalue.test.ts` reads.
        assert_eq!(
            file,
            "[file w2.pdf, 4.0 KB · tap to show · held:file:\
             RnJvbU1haWwobWVzc2FnZT0nZm9ybS0xQGV4YW1wbGUudGVzdCcsIG5hbWU9J3cyLnBkZicp.file]"
        );
        let masked = Masked::of(&notice(), &text);
        let place = Place::new(Holder::File(notice()), 0).expect("a place");
        assert_eq!(
            masked.as_str(),
            format!(
                "Employee SSN [SSN ending 3456 · tap to show · {}]. Also [ITIN ending 1234 · \
                 withheld] and [file w2.pdf, 4.0 KB · withheld].",
                Handle::Value(place).print()
            )
        );
    }

    /// The value, a body that holds it and a file's bytes print as none of them in a `{:?}`.
    #[test]
    fn a_read_value_and_a_filled_body_never_print() {
        struct Holds;
        impl Reveal for Holds {
            fn value(
                &self,
                _: &str,
                _: u16,
            ) -> Result<Option<Shown>, crate::capability::MailError> {
                Ok(Some(Shown::new(Kind::Ssn, SSN)))
            }
            fn original(&self, _: &str, _: u64) -> Result<Reopened, crate::capability::MailError> {
                Ok(Reopened::Gone)
            }
        }
        struct Pdf;
        impl Files for Pdf {
            fn open(&self, _: &HeldFile) -> Result<Option<Opened>, String> {
                Ok(Some(Opened {
                    name: "w2.pdf".to_owned(),
                    media: "application/pdf".to_owned(),
                    bytes: format!("%PDF-1.7 SSN {SSN}").into_bytes(),
                }))
            }
            fn layer(&self, _: &[u8]) -> Result<String, String> {
                Ok(format!("SSN {SSN}"))
            }
        }
        let holders = Holders {
            mail: &Holds,
            files: &Pdf,
        };
        let in_mail = Handle::new("x@example.test", 0).expect("a handle");
        let in_file = Place::new(Holder::File(notice()), 0).expect("a place");
        let body = format!(
            "It is {} and {}, in {}.",
            Placeholder::of(
                Kind::Ssn,
                SSN,
                Reach::Revealable(Place::new(message("x@example.test"), 0).expect("a place"))
            )
            .print(),
            Placeholder::of(Kind::Ssn, SSN, Reach::Revealable(in_file.clone())).print(),
            Placeholder::file(&notice(), "w2.pdf", 4096).print(),
        );
        let w2 = crate::secretary::attached::Attached::of(
            notice().source(),
            "w2.pdf",
            "application/pdf",
            &[0; 4096],
        )
        .expect("a record");
        let written = Written::parse(&body).expect("written");
        let filled = fill(&written, &holders, std::slice::from_ref(&w2)).expect("filled");
        // A file placeholder that no longer prints as its record, or names no record, is refused.
        let renamed = crate::secretary::attached::Attached {
            name: "w2-new.pdf".to_owned(),
            ..w2
        };
        assert_eq!(
            fill(&written, &holders, &[renamed]).map(|_| ()),
            Err(Unfilled::Changed("file w2.pdf, 4.0 KB".to_owned()))
        );
        assert_eq!(
            fill(&written, &holders, &[]).map(|_| ()),
            Err(Unfilled::Gone("file w2.pdf, 4.0 KB".to_owned()))
        );
        let printed = format!(
            "{filled:?} {:?} {:?} {:?}",
            holders.read(&in_mail),
            holders.read(&Handle::Value(in_file)),
            holders.read(&Handle::File(notice())),
        );
        assert!(!printed.contains(SSN), "{printed}");
        assert_eq!(
            filled.into_string(),
            format!("It is {SSN} and {SSN}, in w2.pdf.")
        );
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
        let mail = |text: &str| base64(text);
        for text in [
            String::new(),
            "held:".to_owned(),
            "held:.0".to_owned(),
            "held:YQ".to_owned(),
            "held:YQ.x".to_owned(),
            "held:YQ.01".to_owned(),
            "held:YQ.70000".to_owned(),
            "held:YQ.file".to_owned(),
            "mail:YQ.0".to_owned(),
            "held:!!.0".to_owned(),
            format!("held:file:{}.file", mail("FromUnknown(id='a')")),
            format!("held:file:{}.0", mail("not a literal")),
            format!("held:{}.0", "A".repeat(MAX_HANDLE)),
        ] {
            assert!(Handle::parse(&text).is_err(), "{text}");
        }
        assert!(Handle::new("", 0).is_err());
        assert!(Handle::new(&"a".repeat(MAX_MESSAGE + 1), 0).is_err());
        let file = Placeholder::file(&notice(), "w2.pdf", 4096).print();
        let number = Handle::new("x@example.test", 0).expect("a handle").print();
        for crossed in [
            file.replace("tap to show · held:file:", "tap to show · held:file:x"),
            format!("[file w2.pdf, 4.0 KB · tap to show · {number}]"),
            format!(
                "[SSN ending 3456 · tap to show · {}]",
                Handle::File(notice()).print()
            ),
        ] {
            assert!(Written::parse(&crossed).is_err(), "{crossed}");
        }
    }

    /// Every source a file may be read from, from a name drawn by the test.
    fn source(at: usize, name: &str) -> HeldFile {
        let id = crate::attachment::AttachmentId::of(name.as_bytes());
        match at % 3 {
            0 => HeldFile::Mail {
                message: format!("{name}@example.test"),
                name: name.to_owned(),
            },
            1 => HeldFile::Drive {
                path: format!("/my-files/f {}", name.replace('/', "-"))
                    .parse()
                    .expect("a path"),
            },
            _ => HeldFile::Upload { id },
        }
    }

    proptest! {
        /// A placeholder's text form is read back as the same placeholder, whatever its kind.
        #[test]
        fn a_placeholder_round_trips(
            at in 0..=Kind::ALL.len(),
            value in "[0-9]{9,19}",
            message in "[ -~]{1,80}",
            name in "[a-z][a-z .]{0,30}",
            ordinal in any::<u16>(),
            size in any::<u32>(),
            revealable in any::<bool>(),
            in_file in any::<bool>(),
        ) {
            let holder = if in_file {
                Holder::File(source(usize::from(ordinal), &name))
            } else {
                Holder::Message(message)
            };
            let placeholder = match Kind::ALL.get(at) {
                Some(kind) => Placeholder::of(*kind, &value, if revealable {
                    Reach::Revealable(Place::new(holder, ordinal).expect("a place"))
                } else {
                    Reach::Withheld
                }),
                None => {
                    match Placeholder::file(
                        &source(usize::from(ordinal), &name), &name, u64::from(size),
                    ) {
                        Placeholder::File { name, size, .. } if !revealable => {
                            Placeholder::File { name, size, reach: Reach::Withheld }
                        }
                        file => file,
                    }
                }
            };
            let text = format!("a {} b", placeholder.print());
            prop_assert_eq!(
                Written::parse(&text).expect("written").held().cloned().collect::<Vec<_>>(),
                vec![placeholder]
            );
        }

        /// The text form prints and parses back to the same handle, of a value in a message or a
        /// file, or of a file itself.
        #[test]
        fn a_handle_round_trips(
            message in "[ -~]{1,200}",
            name in "[ -~]{1,40}",
            ordinal in any::<u16>(),
            at in 0usize..3,
        ) {
            for handle in [
                Handle::new(&message, ordinal).expect("a handle"),
                Handle::Value(
                    Place::new(Holder::File(source(at, &name)), ordinal).expect("a place"),
                ),
                Handle::File(source(at, &name)),
            ] {
                prop_assert_eq!(Handle::parse(&handle.print()), Ok(handle));
            }
        }
    }
}
