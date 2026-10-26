//! Held numbers never reach a transcript, and neither does a link's credential.
//!
//! [`Redacted`] is the only type a document read answers in, and [`redact`] is its only
//! constructor, so text that skipped the pass does not typecheck. [`Scrubbed`] is the same
//! promise for a stored conversation line: every string in it went through [`redact`] and the
//! link pass, and [`Scrubbed::of`] is its only constructor. [`Rules`] names the patterns a scrub
//! applies, so a stored line can record which rules its words passed.
//!
//! These shapes are held numbers, replaced with [`REDACTED`]:
//!
//! - an SSN or ITIN written out, `ddd-dd-dddd` or `ddd dd dddd`, anywhere;
//! - nine digits in a row (single hyphens or spaces between them allowed, so
//!   an EIN's `dd-ddddddd` counts) within [`NEAR`] characters of a label:
//!   SSN, social security, TIN, ITIN or EIN;
//! - a card number anywhere: 13 to 19 digits, in groups or not, with an issuer's prefix at one
//!   of its lengths and a valid Luhn check digit, never after a `+`;
//! - an IBAN anywhere, in either case, whose mod-97 check holds;
//! - a verification code: four to eight digits, or `ddd ddd`, after a label that names the code
//!   with its noun (verification code, security code, one-time password, and the words for a
//!   verification code in Portuguese, Spanish, French, German and Arabic) within [`STRONG_NEAR`]
//!   characters on its line, or within [`NEAR`] of such a label or of verification, verify,
//!   passcode, OTP, 2FA or MFA; four or five digits right after `code` or `code:`; six to eight
//!   digits within [`NEAR`] of code, codes or PIN, unless the word before makes it a postal, zip,
//!   promo, error or dialling code. A label claims only the first number beside it, a closer
//!   order, invoice, reference, flight, room, fee or total label claims the number instead, and a
//!   year or a `yyyymmdd` date is not a code;
//! - a bank account number: six to seventeen digits, in a row or in groups, within [`NEAR`]
//!   characters of account, acct, a/c, checking or savings, unless an order, invoice, tracking,
//!   booking, telephone or help label is closer, and not grouped as a phone number or a date;
//! - a routing number: nine digits whose ABA checksum holds, within [`NEAR`] characters of
//!   routing, ABA, RTN or transit.
//!
//! A labelled code, account or routing number stands alone: no `+` or currency beside it, no
//! digit beside it across a hyphen, dot, comma or slash, and no run of three digits or more
//! across one space. When matches overlap, a code wins, then a kind whose checksum held, then the
//! closest label. No label is read inside a placeholder `crate::held_value` printed.
//!
//! The link pass replaces, anywhere in the text:
//!
//! - the value of a query or fragment parameter whose name ends in one of
//!   [`CREDENTIAL_PARAMETERS`], percent-encoded separators included, since a
//!   sign-in link is often wrapped inside a tracking link's own parameter;
//! - any JWT, three or more base64url segments the first of which opens `eyJ`;
//! - the password of a `scheme://user:password@host` link.
//!
//! A document read keeps its links: a sign-in link is not an identity number, and a PDF is
//! withheld on what [`redact`] finds. A conversation line is stored and pushed, so it loses them;
//! the mail it quotes keeps the working link.
//!
//! Over-redaction is the safe failure: a number that looks like one of these
//! and is not costs the owner a look at the file; one that slips through costs
//! the number. A `zipcode=` parameter loses its value for the same reason.

use std::borrow::Cow;
use std::cell::LazyCell;
use std::sync::LazyLock;

use crate::rule_version::{Version, Versioned};
use regex::Regex;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// What a redacted number becomes.
pub const REDACTED: &str = "[redacted]";

/// How close, in characters, a nine-digit run must be to a label.
pub const NEAR: usize = 20;

/// How far, in characters and on the same line, a label that names a code with its noun
/// ("verification code") reaches forward to the code: a message often names the service between
/// the two.
pub const STRONG_NEAR: usize = 60;

/// Text with every identity number this module recognises replaced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redacted(String);

impl Redacted {
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

/// `ddd-dd-dddd` or `ddd dd dddd`, one separator kind throughout, not part
/// of a longer run of digits.
static WRITTEN_OUT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b[0-9]{3}(?:-[0-9]{2}-|\x20[0-9]{2}\x20)[0-9]{4}\b").expect("a valid pattern")
});

/// Nine digits, single hyphens or spaces allowed between them, not part of
/// a longer run.
static NINE_DIGITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[0-9](?:[- ]?[0-9]){8}\b").expect("a valid pattern"));

/// The labels that make a nearby nine-digit run an identity number.
static LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:ssn|social\s+security|itin|tin|ein)\b").expect("a valid pattern")
});

/// The endings, compared without case, of a parameter name that carries a credential: `token`
/// covers `access_token`, `id_token` and `X-Amz-Security-Token`, `code` an OAuth or reset code.
pub const CREDENTIAL_PARAMETERS: &[&str] = &[
    "token",
    "code",
    "key",
    "sig",
    "signature",
    "secret",
    "password",
    "passwd",
    "pwd",
    "auth",
    "session",
    "sessionid",
    "session_id",
    "sid",
    "nonce",
    "otp",
    "ticket",
    "jwt",
    "hash",
    "hmac",
    "credential",
    "assertion",
];

/// A credential parameter: a separator (`?`, `&`, `#`, `;`, or one percent-encoded), the name,
/// `=` (or `%3D`), then the value up to the next separator, quote, bracket or space.
static CREDENTIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"(?i)((?:[?&#;]|%3f|%26|%23)[a-z0-9_.\-]*(?:{})(?:=|%3d))[^&#;\s"'`<>()\[\]\\]+"#,
        CREDENTIAL_PARAMETERS.join("|")
    ))
    .expect("a valid pattern")
});

/// A JWT, or the first three parts of a JWE: base64url segments joined by dots, the header a JSON
/// object, so it opens `eyJ`.
static JWT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\beyJ[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+").expect("a valid pattern")
});

/// The password of a link that carries one: `scheme://user:` kept, the password replaced.
static USERINFO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b([a-z][a-z0-9+.\-]*://[^\s/:@]+:)[^\s/@]+@").expect("a valid pattern")
});

/// Replace every link credential this module recognises in `text`.
///
/// Each pattern runs only on a text holding a substring every match of it holds, so a text
/// without one costs a substring search, not a scan by the regex crate's slower engines, which a
/// Unicode `\b` sends a non-ASCII text to.
pub(crate) fn unlink(text: &str) -> String {
    let mut text = Cow::Borrowed(text);
    if text.contains("://") && text.contains('@') {
        text = Cow::Owned(
            USERINFO
                .replace_all(&text, format!("${{1}}{REDACTED}@").as_str())
                .into_owned(),
        );
    }
    if text.contains('=')
        || text
            .as_bytes()
            .windows(3)
            .any(|three| three.eq_ignore_ascii_case(b"%3d"))
    {
        text = Cow::Owned(
            CREDENTIAL
                .replace_all(&text, format!("${{1}}{REDACTED}").as_str())
                .into_owned(),
        );
    }
    if text.contains("eyJ") {
        text = Cow::Owned(JWT.replace_all(&text, REDACTED).into_owned());
    }
    text.into_owned()
}

/// The scrub as a versioned rule: its version is the SHA-256, in lowercase hex, of every pattern
/// it applies in the order it applies them, then its distances, its currency words, the
/// replacement and [`Versioned::REVISION`]. A stored line records it, so a change to the rules
/// can find every line scrubbed under older ones.
#[derive(Debug)]
pub struct Scrub;

/// Which rules a text was scrubbed under.
pub type Rules = Version<Scrub>;

static RULES: LazyLock<Rules> = LazyLock::new(|| Version::of_parts(()));

impl Versioned for Scrub {
    type Case = ();

    const WHAT: &'static str = "the redaction rules";

    /// The revision of the code the patterns' matches pass through (the checksums, the dates,
    /// the standing-alone test and the overlap rule). Raise it when that code changes what is
    /// redacted, since no pattern's text shows such a change.
    const REVISION: u32 = 1;

    fn parts((): ()) -> Vec<String> {
        let patterns: [&Regex; 20] = [
            &WRITTEN_OUT,
            &NINE_DIGITS,
            &LABEL,
            &NAMED_CODE_LABEL,
            &STRONG_CODE_LABEL,
            &CODE_LABEL,
            &NOT_CODE_LABEL,
            &CODE,
            &ACCOUNT_LABEL,
            &NOT_ACCOUNT_LABEL,
            &ACCOUNT,
            &ROUTING_LABEL,
            &ROUTING,
            &GROUPS,
            &IBAN,
            &HINT,
            &PLACEHOLDER,
            &USERINFO,
            &CREDENTIAL,
            &JWT,
        ];
        patterns
            .iter()
            .map(|pattern| pattern.as_str().to_owned())
            .chain([NEAR.to_string(), STRONG_NEAR.to_string()])
            .chain(CURRENCIES.iter().map(|word| (*word).to_owned()))
            .chain([REDACTED.to_owned()])
            .collect()
    }

    fn current((): ()) -> &'static Rules {
        &RULES
    }
}

/// A value every string of which, object keys included, went through [`redact`] and the link
/// pass: what a conversation line must be before a store takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scrubbed<T>(T);

/// A value whose scrubbed strings no longer make one: a replacement landed in a field that
/// refuses it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the value cannot be stored with its credentials redacted: {0}")]
pub struct Unscrubbable(pub String);

impl<T: Serialize + DeserializeOwned> Scrubbed<T> {
    /// Scrub every string of `value`, through its own serde shape.
    ///
    /// # Errors
    /// [`Unscrubbable`] when the scrubbed strings do not parse back, which refuses the value
    /// rather than store it unscrubbed.
    pub fn of(value: &T) -> Result<Self, Unscrubbable> {
        #[cfg(test)]
        SCRUBS.with(|count| count.set(count.get() + 1));
        let tree = serde_json::to_value(value).map_err(|e| Unscrubbable(e.to_string()))?;
        serde_json::from_value(scrubbed(tree))
            .map(Self)
            .map_err(|e| Unscrubbable(e.to_string()))
    }
}

impl<T> Scrubbed<T> {
    /// The value, safe to store.
    #[must_use]
    pub fn get(&self) -> &T {
        &self.0
    }

    /// The value, owned.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

#[cfg(test)]
thread_local! {
    static SCRUBS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many values this thread has scrubbed, so a test can show a path scrubs none.
#[cfg(test)]
pub(crate) fn scrubs() -> usize {
    SCRUBS.with(std::cell::Cell::get)
}

fn scrubbed(tree: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    let text = |text: &str| unlink(redact(text).as_str());
    match tree {
        Value::String(string) => Value::String(text(&string)),
        Value::Array(items) => Value::Array(items.into_iter().map(scrubbed).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (text(&key), scrubbed(value)))
                .collect(),
        ),
        other => other,
    }
}

/// Which held number a span of text is, by the label beside it or, with none, by its shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A Social Security number.
    Ssn,
    /// An Individual Taxpayer Identification Number.
    Itin,
    /// A Taxpayer Identification Number of no narrower kind.
    Tin,
    /// An Employer Identification Number.
    Ein,
    /// A verification or two-factor code.
    Code,
    /// A payment card number.
    Card,
    /// A bank account number.
    Account,
    /// A US bank routing number.
    Routing,
    /// An International Bank Account Number.
    Iban,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Kind; 9] = [
        Kind::Ssn,
        Kind::Itin,
        Kind::Tin,
        Kind::Ein,
        Kind::Code,
        Kind::Card,
        Kind::Account,
        Kind::Routing,
        Kind::Iban,
    ];

    /// The kind as a person writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Kind::Ssn => "SSN",
            Kind::Itin => "ITIN",
            Kind::Tin => "TIN",
            Kind::Ein => "EIN",
            Kind::Code => "verification code",
            Kind::Card => "card",
            Kind::Account => "account",
            Kind::Routing => "routing number",
            Kind::Iban => "IBAN",
        }
    }

    /// The kind as its stored form spells it, the same word its serde form reads.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Kind::Ssn => "ssn",
            Kind::Itin => "itin",
            Kind::Tin => "tin",
            Kind::Ein => "ein",
            Kind::Code => "code",
            Kind::Card => "card",
            Kind::Account => "account",
            Kind::Routing => "routing",
            Kind::Iban => "iban",
        }
    }

    fn of_identity_label(label: &str) -> Self {
        let label = label.to_ascii_lowercase();
        if label == "itin" {
            Kind::Itin
        } else if label == "tin" {
            Kind::Tin
        } else if label == "ein" {
            Kind::Ein
        } else {
            Kind::Ssn
        }
    }
}

crate::literal::text_literal!(Kind => |kind| kind.word().to_owned());

/// One held number in a text: its byte range and its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// Where it starts.
    pub start: usize,
    /// Where it ends.
    pub end: usize,
    /// What it is.
    pub kind: Kind,
}

/// How strongly a match claims its kind, strongest first: when matches overlap, the strongest
/// decides the kind. A code wins, since its last digits are most of it and no other kind hides
/// them; a checksum that held beats a label; a closer label beats a farther one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Strength {
    Code,
    Checked,
    Labelled(usize),
    Shaped,
}

/// A match before overlaps are joined.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    found: Found,
    strength: Strength,
}

/// The labels that name a code with its noun ("verification code", "security code",
/// "one-time password", and the words for a verification code in Portuguese, Spanish, French,
/// German and Arabic): before a code, they reach [`STRONG_NEAR`] characters on its line.
static NAMED_CODE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:(?:verification|verify|otp|one[- ]time|2fa|mfa|security|login|sign[- ]in|access)\s+(?:code|passcode|password|pin)|c[oó]digo\s+de\s+(?:verifica[cç][aã]o|verificaci[oó]n|seguran[cç]a|seguridad)|code\s+de\s+(?:v[ée]rification|s[ée]curit[ée])|best[aä]tigungscode|verifizierungscode|sicherheitscode|رمز\s+التحقق)\b",
    )
    .expect("a valid pattern")
});

/// The labels a code of four digits or more is held beside within [`NEAR`] characters.
static STRONG_CODE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:verification|verify|passcode|otp|one[- ]time\s+(?:password|passcode|code|pin)|2fa|mfa)\b",
    )
    .expect("a valid pattern")
});

/// The labels a code of six digits or more is held beside, unless a word before says the code is
/// a postal, promotional, error or dialling code (the first group). Right before the number, as
/// in "code: 4821", `code` holds four or five digits too.
static CODE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:(postal|zip|promo|promotional|error|discount|coupon|area|country|source|status|tracking)\s+)?(?:codes?|pin|c[oó]digos?|رمز)\b",
    )
    .expect("a valid pattern")
});

/// The labels that claim a number beside them for something other than a code.
static NOT_CODE_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:order|invoice|reference|ref|flight|room|fee|total|amount|price|ticket|booking)\b",
    )
    .expect("a valid pattern")
});

/// Four to eight digits, or three and three with one space or hyphen: the codes services send.
static CODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[0-9]{4,8}|[0-9]{3}[ -][0-9]{3})\b").expect("a valid pattern")
});

/// The labels of a bank account number.
static ACCOUNT_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:account|acct|a/c|checking|savings)\b").expect("a valid pattern")
});

/// The labels that claim a number beside them for something other than an account: an order, an
/// invoice, a parcel, a booking, a case, a telephone or a help line.
static NOT_ACCOUNT_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:order|invoice|tracking|reference|ref|confirmation|receipt|booking|case|ticket|phone|tel|call|fax|help|support|service|customer|contact)\b",
    )
    .expect("a valid pattern")
});

/// Six to seventeen digits with nothing between them, or groups of two to six digits joined by
/// single spaces or hyphens: a bank account number, checked by [`account`].
static ACCOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:[0-9]{6,17}|[0-9]{2,6}(?:[ -][0-9]{2,6}){1,4})\b").expect("a valid pattern")
});

/// The labels of a routing number.
static ROUTING_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:routing|aba|rtn|transit)\b").expect("a valid pattern"));

/// Nine digits, checked by [`aba`].
static ROUTING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[0-9]{9}\b").expect("a valid pattern"));

/// Runs of digit groups with one space or hyphen between groups; [`cards`] tries each stretch of
/// groups in a run, since a card's groups often sit beside an expiry date or a phone number.
static GROUPS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[0-9]+(?:[ -][0-9]+)*\b").expect("a valid pattern"));

/// A country code, two check digits and up to thirty letters or digits, in groups of four or
/// none, in either case, checked by [`iban`].
static IBAN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b[A-Z]{2}[0-9]{2}(?:[A-Z0-9]{11,30}|(?: [A-Z0-9]{4}){2,7}(?: [A-Z0-9]{1,3})?)\b",
    )
    .expect("a valid pattern")
});

/// The hint of a placeholder `crate::held_value` printed: at most four digits, which the
/// placeholder already shows, so they are never read as a number again.
static HINT: LazyLock<Regex> = LazyLock::new(|| {
    let names: Vec<String> = Kind::ALL
        .iter()
        .map(|kind| regex::escape(kind.name()))
        .collect();
    Regex::new(&format!(
        r"\[(?:{}) ending ([0-9]{{1,4}}) · ",
        names.join("|")
    ))
    .expect("a pattern built from the kinds' literal names")
});

/// A whole placeholder `crate::held_value` printed, whose words are never read as a label.
static PLACEHOLDER: LazyLock<Regex> = LazyLock::new(|| {
    let names: Vec<String> = Kind::ALL
        .iter()
        .map(|kind| regex::escape(kind.name()))
        .collect();
    Regex::new(&format!(
        r"\[(?:{})(?: ending [0-9]{{1,4}})? · (?:tap to show · [^\]]*|withheld)\]",
        names.join("|")
    ))
    .expect("a pattern built from the kinds' literal names")
});

/// Every label in a text, each a byte range and the kind it makes a number beside it; a label
/// inside a placeholder is none.
struct Labels {
    identity: Vec<Label>,
    named: Vec<Label>,
    strong: Vec<Label>,
    weak: Vec<Label>,
    not_codes: Vec<Label>,
    accounts: Vec<Label>,
    not_accounts: Vec<Label>,
    routings: Vec<Label>,
}

impl Labels {
    fn of(text: &str) -> Self {
        let held: Vec<std::ops::Range<usize>> = PLACEHOLDER
            .find_iter(text)
            .map(|found| found.range())
            .collect();
        let outside =
            |(start, end, _): &Label| !held.iter().any(|h| *start < h.end && h.start < *end);
        let labels = |pattern: &Regex, kind: fn(&str) -> Kind| -> Vec<Label> {
            pattern
                .find_iter(text)
                .map(|found| (found.start(), found.end(), kind(found.as_str())))
                .filter(outside)
                .collect()
        };
        let identity = labels(&LABEL, Kind::of_identity_label);
        let named = labels(&NAMED_CODE_LABEL, |_| Kind::Code);
        let strong = labels(&STRONG_CODE_LABEL, |_| Kind::Code);
        let weak: Vec<Label> = CODE_LABEL
            .captures_iter(text)
            .filter(|captures| captures.get(1).is_none())
            .filter_map(|captures| captures.get(0))
            .map(|found| (found.start(), found.end(), Kind::Code))
            .filter(outside)
            .collect();
        let not_codes = labels(&NOT_CODE_LABEL, |_| Kind::Code);
        let accounts = labels(&ACCOUNT_LABEL, |_| Kind::Account);
        let not_accounts = labels(&NOT_ACCOUNT_LABEL, |_| Kind::Account);
        let routings = labels(&ROUTING_LABEL, |_| Kind::Routing);
        Self {
            identity,
            named,
            strong,
            weak,
            not_codes,
            accounts,
            not_accounts,
            routings,
        }
    }
}

/// Every code in `text`, by [`Labels`]' code labels, the pattern matched over `digits`
/// ([`identity_numbers`]).
fn codes(
    text: &str,
    digits: &str,
    labels: &LazyCell<Labels, impl FnOnce() -> Labels>,
) -> Vec<Candidate> {
    let right_before = LazyCell::new(|| -> Vec<Label> {
        labels
            .weak
            .iter()
            .copied()
            .filter(|&(label_start, label_end, _)| {
                text[label_start..label_end].eq_ignore_ascii_case("code")
            })
            .collect()
    });
    CODE.find_iter(digits)
        .filter_map(|found| {
            let Labels {
                named,
                strong,
                weak,
                not_codes,
                ..
            } = &**labels;
            let (start, end) = (found.start(), found.end());
            let digits = digits_of(found.as_str());
            let code = |labels: &[Label], reach: usize, before: bool| {
                claims(text, labels, (start, end), reach, before)
            };
            let gap = [
                code(named, STRONG_NEAR, true),
                code(named, NEAR, false),
                code(strong, NEAR, false),
                code(&right_before, 2, true),
                (digits.len() >= 6)
                    .then(|| code(weak, NEAR, false))
                    .flatten(),
            ]
            .into_iter()
            .flatten()
            .min()?;
            let other = closest(text, not_codes, start, end).map_or(usize::MAX, |(_, other)| other);
            (gap < other && alone(text, start, end) && !dated(&digits))
                .then(|| candidate(start, end, Kind::Code, Strength::Code))
        })
        .collect()
}

/// Every held number this module recognises in `text`, in order and never overlapping: two
/// matches that overlap are one, from the first one's start to the later end, of the kind of
/// the strongest: a code, then a kind whose checksum held, then the closest label.
///
/// Every kind holds an ASCII digit, so a text with none holds no number; a text with some is
/// matched by the digit patterns over its `ascii` copy, and its labels are read only when a
/// pattern found something they could claim.
#[must_use]
pub fn identity_numbers(text: &str) -> Vec<Found> {
    if !text.bytes().any(|byte| byte.is_ascii_digit()) {
        return Vec::new();
    }
    numbers(text, &ascii(text))
}

/// `text` as the digit patterns read it: each character outside ASCII replaced, byte for byte,
/// by `_` where Unicode counts it part of a word and by `#` where it does not.
///
/// No digit pattern matches either byte, and `\b` falls where it falls in `text`, so every match
/// is the same, at the same byte range; the IBAN pattern, whose letters `(?i)` folds onto two
/// characters outside ASCII, reads the copy only of a text holding neither. The copy is ASCII,
/// which lets the regex crate's fast engine run: that engine gives up at the first non-ASCII byte
/// under a Unicode `\b`.
fn ascii(text: &str) -> Cow<'_, str> {
    if text.is_ascii() {
        return Cow::Borrowed(text);
    }
    let mut copy = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            copy.push(c);
        } else {
            let fill = if regex_syntax::is_word_character(c) {
                '_'
            } else {
                '#'
            };
            copy.extend(std::iter::repeat_n(fill, c.len_utf8()));
        }
    }
    Cow::Owned(copy)
}

/// Every held number in `text`, the digit patterns matched over `digits`, which has `text`'s
/// byte ranges and is `text` itself or its [`ascii`] copy.
fn numbers(text: &str, digits: &str) -> Vec<Found> {
    let labels = LazyCell::new(|| Labels::of(text));
    let mut spans: Vec<Candidate> = WRITTEN_OUT
        .find_iter(digits)
        .map(|found| {
            let identity = &labels.identity;
            let (kind, strength) = closest(text, identity, found.start(), found.end()).map_or(
                (
                    if found.as_str().starts_with('9') {
                        Kind::Itin
                    } else {
                        Kind::Ssn
                    },
                    Strength::Shaped,
                ),
                |(kind, gap)| (kind, Strength::Labelled(gap)),
            );
            candidate(found.start(), found.end(), kind, strength)
        })
        .collect();
    spans.extend(NINE_DIGITS.find_iter(digits).filter_map(|found| {
        closest(text, &labels.identity, found.start(), found.end())
            .map(|(kind, gap)| candidate(found.start(), found.end(), kind, Strength::Labelled(gap)))
    }));
    spans.extend(codes(text, digits, &labels));
    spans.extend(ACCOUNT.find_iter(digits).filter_map(|found| {
        let (start, end) = (found.start(), found.end());
        if !account(found.as_str()) || !alone(text, start, end) {
            return None;
        }
        let other =
            closest(text, &labels.not_accounts, start, end).map_or(usize::MAX, |(_, gap)| gap);
        closest(text, &labels.accounts, start, end)
            .filter(|&(_, gap)| gap < other)
            .map(|(kind, gap)| candidate(start, end, kind, Strength::Labelled(gap)))
    }));
    spans.extend(ROUTING.find_iter(digits).filter_map(|found| {
        let (start, end) = (found.start(), found.end());
        (aba(found.as_str())
            && alone(text, start, end)
            && closest(text, &labels.routings, start, end).is_some())
        .then(|| candidate(start, end, Kind::Routing, Strength::Checked))
    }));
    spans.extend(cards(text, digits));
    // Under `(?i)`, `ſ` matches `S` and the Kelvin sign `K`, each a letter the copy has no
    // spelling of, so a text holding either is matched as written.
    let letters = if text.contains(['\u{17f}', '\u{212a}']) {
        text
    } else {
        digits
    };
    spans.extend(
        IBAN.find_iter(letters)
            .filter(|found| iban(found.as_str()))
            .map(|found| candidate(found.start(), found.end(), Kind::Iban, Strength::Checked)),
    );
    if spans.is_empty() {
        return Vec::new();
    }
    let hints: Vec<std::ops::Range<usize>> = HINT
        .captures_iter(text)
        .filter_map(|captures| captures.get(1))
        .map(|hint| hint.range())
        .collect();
    spans.retain(|one| {
        !hints
            .iter()
            .any(|hint| one.found.start < hint.end && hint.start < one.found.end)
    });
    spans.sort_unstable_by_key(|one| (one.found.start, one.found.end));
    let mut merged: Vec<Candidate> = Vec::with_capacity(spans.len());
    for one in spans {
        match merged.last_mut() {
            Some(last) if one.found.start < last.found.end => {
                last.found.end = last.found.end.max(one.found.end);
                if one.strength < last.strength {
                    last.found.kind = one.found.kind;
                    last.strength = one.strength;
                }
            }
            Some(_) | None => merged.push(one),
        }
    }
    merged.into_iter().map(|one| one.found).collect()
}

fn candidate(start: usize, end: usize, kind: Kind, strength: Strength) -> Candidate {
    Candidate {
        found: Found { start, end, kind },
        strength,
    }
}

/// A label's byte range and the kind it makes a number beside it.
type Label = (usize, usize, Kind);

/// How far the closest of `labels` that claims a code at `start..end` is: within `reach`
/// characters, on its line, with no digit between, so a label claims the first number beside it;
/// before the code alone when `before`.
fn claims(
    text: &str,
    labels: &[Label],
    (start, end): (usize, usize),
    reach: usize,
    before: bool,
) -> Option<usize> {
    labels
        .iter()
        .filter_map(|&(label_start, label_end, _)| {
            let gap = if label_end <= start {
                &text[label_end..start]
            } else if end <= label_start && !before {
                &text[end..label_start]
            } else {
                return None;
            };
            let count = gap.chars().count();
            (count <= reach && !gap.contains('\n') && !gap.chars().any(|c| c.is_ascii_digit()))
                .then_some(count)
        })
        .min()
}

/// The kind of the closest of `labels` within [`NEAR`] characters of `start..end` on its line,
/// and how far.
fn closest(text: &str, labels: &[Label], start: usize, end: usize) -> Option<(Kind, usize)> {
    labels
        .iter()
        .filter_map(|&(label_start, label_end, kind)| {
            let gap = if label_end <= start {
                &text[label_end..start]
            } else if end <= label_start {
                &text[end..label_start]
            } else {
                ""
            };
            (!gap.contains('\n') && gap.chars().count() <= NEAR)
                .then(|| (kind, gap.chars().count()))
        })
        .min_by_key(|&(_, gap)| gap)
}

fn digits_of(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// The currencies an amount is written beside.
const CURRENCIES: &[&str] = &[
    "usd", "eur", "gbp", "cad", "aud", "chf", "jpy", "lbp", "dollars", "euros",
];

/// Whether a labelled match stands alone, so it is not a piece of a phone number, a price, an
/// amount or a date: no `+` or currency symbol right before it, no currency word beside it, no
/// digit beside it across a hyphen, dot, comma or slash, and no run of three digits or more beside
/// it across one space ("code 482913 10 minutes" stays a code, "617 555 1234" a phone number).
fn alone(text: &str, start: usize, end: usize) -> bool {
    let joins = |c: char| matches!(c, '-' | '.' | ',' | '/');
    let money = |c: char| matches!(c, '$' | '€' | '£' | '¥' | '₹' | '+');
    let run =
        |chars: &mut dyn Iterator<Item = char>| chars.take_while(char::is_ascii_digit).count();
    let side = |mut chars: Box<dyn Iterator<Item = char> + '_>| -> bool {
        let first = chars.next();
        let mut rest = chars.peekable();
        match first {
            Some(c) if money(c) => false,
            Some(c) if joins(c) => !rest.peek().is_some_and(char::is_ascii_digit),
            Some(' ') => match rest.peek() {
                Some(c) if money(*c) => false,
                Some(c) if c.is_ascii_digit() => run(&mut rest) < 3,
                _ => true,
            },
            _ => true,
        }
    };
    let word = |words: &mut dyn Iterator<Item = &str>| {
        words
            .next()
            .is_some_and(|word| CURRENCIES.contains(&word.to_lowercase().as_str()))
    };
    side(Box::new(text[..start].chars().rev()))
        && side(Box::new(text[end..].chars()))
        && !word(&mut text[..start].split_whitespace().rev())
        && !word(&mut text[end..].split_whitespace())
}

/// Whether `digits` reads as a date: `yyyymmdd` or a year, from 1900 to 2099.
fn dated(digits: &str) -> bool {
    let number =
        |range: std::ops::Range<usize>| digits.get(range).and_then(|n| n.parse::<u32>().ok());
    match digits.len() {
        4 => number(0..4).is_some_and(|year| (1900..=2099).contains(&year)),
        8 => {
            number(0..4).is_some_and(|year| (1900..=2099).contains(&year))
                && number(4..6).is_some_and(|month| (1..=12).contains(&month))
                && number(6..8).is_some_and(|day| (1..=31).contains(&day))
        }
        _ => false,
    }
}

/// Whether a match of [`ACCOUNT`] is an account number: six to seventeen digits that read as no
/// date, and in groups, eight or more that are not grouped as a phone number (`ddd-ddd-dddd`,
/// with or without a leading `1`) or a date (`dddd-dd-dd`, `dd-dd-dddd`). Eleven digits in a row
/// that open `1` and a North American area code are a phone number too.
fn account(text: &str) -> bool {
    let digits = digits_of(text);
    let groups: Vec<usize> = text.split([' ', '-']).map(str::len).collect();
    let phone = matches!(groups.as_slice(), [3, 3, 4] | [1, 3, 3, 4])
        || (digits.len() == 11
            && digits.starts_with('1')
            && digits.chars().nth(1).is_some_and(|c| c > '1'));
    let date = matches!(groups.as_slice(), [4, 2, 2] | [2, 2, 4]) || dated(&digits);
    let enough = if groups.len() > 1 { 8 } else { 6 };
    (enough..=17).contains(&digits.len()) && !phone && !date
}

/// A routing number's checksum, from the ABA's own rule: weights 3, 7 and 1, repeated, sum to a
/// multiple of ten. Nine zeros pass that rule and are no routing number.
fn aba(text: &str) -> bool {
    let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(10)).collect();
    let sum: u32 = digits
        .iter()
        .zip([3, 7, 1].iter().cycle())
        .map(|(digit, weight)| digit * weight)
        .sum();
    digits.len() == 9 && sum.is_multiple_of(10) && sum > 0
}

/// Every card number in `text`: a stretch of digit groups of a card's length that starts with an
/// issuer's prefix and passes the Luhn check, the groups matched over `digits`. A run after a `+`
/// is a phone number.
fn cards(text: &str, digits: &str) -> Vec<Candidate> {
    let mut found = Vec::new();
    for run in GROUPS.find_iter(digits) {
        if text[..run.start()].ends_with('+') {
            continue;
        }
        let groups: Vec<(usize, usize)> = digit_groups(run.as_str())
            .map(|(start, end)| (run.start() + start, run.start() + end))
            .collect();
        let mut first = 0;
        while first < groups.len() {
            let card = (first..groups.len()).find(|&last| {
                let digits: String = groups[first..=last]
                    .iter()
                    .map(|&(start, end)| &text[start..end])
                    .collect();
                card(&digits)
            });
            match card {
                Some(last) => {
                    found.push(candidate(
                        groups[first].0,
                        groups[last].1,
                        Kind::Card,
                        Strength::Checked,
                    ));
                    first = last + 1;
                }
                None => first += 1,
            }
        }
    }
    found
}

/// The byte ranges of the digit groups of a run [`GROUPS`] matched.
fn digit_groups(run: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    run.split([' ', '-']).scan(0, |at, group| {
        let start = *at;
        *at += group.len() + 1;
        Some((start, start + group.len()))
    })
}

/// Whether `digits` is a card number: an issuer's prefix at one of its lengths, and the Luhn
/// check. Maestro is taken at sixteen to nineteen digits only, since its shorter lengths match too
/// many order and tracking numbers.
fn card(digits: &str) -> bool {
    let prefix = |n: usize| digits.get(..n).and_then(|p| p.parse::<u32>().ok());
    let length = digits.len();
    let issued = match (prefix(1), prefix(2), prefix(3), prefix(4)) {
        // Visa.
        (Some(4), ..) => matches!(length, 13 | 16 | 19),
        // Mir.
        (.., Some(2200..=2204)) => (16..=19).contains(&length),
        // Mastercard.
        (_, Some(51..=55), ..) | (.., Some(2221..=2720)) => length == 16,
        // American Express.
        (_, Some(34 | 37), ..) => length == 15,
        // Discover, UnionPay, RuPay, Maestro and JCB.
        (.., Some(6011 | 3528..=3589))
        | (_, _, Some(644..=649 | 639), _)
        | (_, Some(50 | 56..=58 | 60 | 62 | 65 | 67 | 81 | 82), ..) => (16..=19).contains(&length),
        // Diners Club.
        (_, Some(36), ..) | (_, _, Some(300..=305), _) => length == 14,
        _ => false,
    };
    issued && luhn(digits)
}

fn luhn(digits: &str) -> bool {
    let sum: u32 = digits
        .chars()
        .rev()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(at, digit)| match (at % 2, digit * 2) {
            (1, doubled) if doubled > 9 => doubled - 9,
            (1, doubled) => doubled,
            _ => digit,
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Whether `text` is an IBAN: fifteen to thirty-four ASCII letters and digits whose ISO 7064
/// remainder, mod 97, is one. The pattern matches `ſ` and the Kelvin sign as letters; neither is
/// a base-36 digit, so a match holding one is no IBAN (and is never split inside it).
fn iban(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| *c != ' ').collect();
    if !compact.is_ascii() || !(15..=34).contains(&compact.len()) {
        return false;
    }
    let (head, tail) = compact.split_at(4);
    let remainder = tail.chars().chain(head.chars()).try_fold(0u32, |acc, c| {
        let value = c.to_digit(36)?;
        Some(if value < 10 {
            (acc * 10 + value) % 97
        } else {
            (acc * 100 + value) % 97
        })
    });
    remainder == Some(1)
}

/// Replace every held number this module recognises in `text`.
#[must_use]
pub fn redact(text: &str) -> Redacted {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for found in identity_numbers(text) {
        out.push_str(&text[at..found.start]);
        out.push_str(REDACTED);
        at = found.end;
    }
    out.push_str(&text[at..]);
    Redacted(out)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn shown(text: &str) -> String {
        redact(text).into_string()
    }

    /// Each number found says what it is: by its label, the closest one, or by its shape.
    #[test]
    fn each_number_found_names_its_kind() {
        let kinds = |text: &str| -> Vec<(String, Kind)> {
            identity_numbers(text)
                .into_iter()
                .map(|found| (text[found.start..found.end].to_owned(), found.kind))
                .collect()
        };
        assert_eq!(
            kinds("000-12-3456"),
            vec![("000-12-3456".to_owned(), Kind::Ssn)]
        );
        assert_eq!(
            kinds("912-70-1234"),
            vec![("912-70-1234".to_owned(), Kind::Itin)]
        );
        assert_eq!(
            kinds("EIN: 00-0000001 and TIN 000000002"),
            vec![
                ("00-0000001".to_owned(), Kind::Ein),
                ("000000002".to_owned(), Kind::Tin)
            ]
        );
        assert_eq!(
            kinds("ITIN 000-12-3456"),
            vec![("000-12-3456".to_owned(), Kind::Itin)]
        );
        assert!(kinds("call 555 0100 on 2026-10-21").is_empty());
    }

    #[test]
    fn a_written_out_ssn_or_itin_is_redacted_anywhere() {
        assert_eq!(shown("123-45-6789"), "[redacted]");
        assert_eq!(
            shown("Number: 912 70 1234, issued 2019."),
            "Number: [redacted], issued 2019."
        );
        assert_eq!(
            shown("a 123-45-6789 and a 987-65-4321"),
            "a [redacted] and a [redacted]"
        );
    }

    #[test]
    fn a_nine_digit_run_is_redacted_next_to_a_label() {
        for (text, expected) in [
            ("SSN: 123456789", "SSN: [redacted]"),
            ("ssn 123456789.", "ssn [redacted]."),
            (
                "Social Security Number 123456789",
                "Social Security Number [redacted]",
            ),
            (
                "Social  security no. is 123456789",
                "Social  security no. is [redacted]",
            ),
            ("ITIN 912701234", "ITIN [redacted]"),
            ("TIN: 123456789", "TIN: [redacted]"),
            ("Employer EIN 12-3456789", "Employer EIN [redacted]"),
            ("EIN: 12 3456789", "EIN: [redacted]"),
            ("123456789 (SSN)", "[redacted] (SSN)"),
        ] {
            assert_eq!(shown(text), expected, "{text}");
        }
    }

    #[test]
    fn a_nine_digit_run_far_from_any_label_stays() {
        let far = format!("SSN{}123456789", " ".repeat(NEAR + 1));
        assert_eq!(shown(&far), far);
        let near = format!("SSN{}123456789", " ".repeat(NEAR));
        assert_eq!(shown(&near), format!("SSN{}[redacted]", " ".repeat(NEAR)));
        assert_eq!(shown("Order 123456789 shipped"), "Order 123456789 shipped");
    }

    /// Near-misses: shapes an official letter is full of that are not identity numbers.
    #[test]
    fn phone_numbers_dates_and_receipt_numbers_are_not_redacted() {
        for text in [
            "Call 617-555-1234 or (617) 555-1234",
            "+1 617 555 1234",
            "Dated 2026-09-25, due 09/30/2026",
            "Receipt IOE0912345678 and EAC2190012345",
            "Receipt number: IOE0912345678",
            "A-Number A123456789",
            "Case 1234567890 (ten digits)",
            "123-456-789",
            "12-345-6789",
            "1234-56-7890",
            "The tinted window, a Martin, being here",
            "SSN field left blank; ref 12345678",
            "ITIN: 1234567890",
        ] {
            assert_eq!(shown(text), text, "{text}");
        }
    }

    /// The wider kinds, in the shapes their senders write them, each with its kind.
    #[test]
    fn codes_cards_and_bank_numbers_are_found_with_their_kind() {
        for (text, held, kind) in [
            ("Your verification code is 482913.", "482913", Kind::Code),
            (
                "G-482913 is your Google verification code",
                "482913",
                Kind::Code,
            ),
            ("Use code 482 913 to sign in", "482 913", Kind::Code),
            ("Your PIN: 12345678", "12345678", Kind::Code),
            (
                "Card 4111 1111 1111 1111 exp 12/27",
                "4111 1111 1111 1111",
                Kind::Card,
            ),
            (
                "paid with 5555-5555-5555-4444",
                "5555-5555-5555-4444",
                Kind::Card,
            ),
            ("Amex 378282246310005", "378282246310005", Kind::Card),
            (
                "Account number: 000123456789",
                "000123456789",
                Kind::Account,
            ),
            ("acct # 12345678 at the bank", "12345678", Kind::Account),
            ("Routing number 021000021", "021000021", Kind::Routing),
            (
                "IBAN GB82 WEST 1234 5698 7654 32",
                "GB82 WEST 1234 5698 7654 32",
                Kind::Iban,
            ),
            (
                "wire to DE89370400440532013000 today",
                "DE89370400440532013000",
                Kind::Iban,
            ),
            (
                "Your verification code for your Example account is 482913",
                "482913",
                Kind::Code,
            ),
            ("Your verification code is 4821.", "4821", Kind::Code),
            ("Passcode: 48213", "48213", Kind::Code),
            ("OTP 4821 for your login", "4821", Kind::Code),
            ("Your 2FA code: 4821", "4821", Kind::Code),
            ("Your Uber code: 4821", "4821", Kind::Code),
            ("MFA 48213", "48213", Kind::Code),
            ("Your security code is 4821", "4821", Kind::Code),
            ("one time password 48213", "48213", Kind::Code),
            ("Your backup codes: 48213901", "48213901", Kind::Code),
            ("To verify, enter 4821", "4821", Kind::Code),
            ("Code 482913 expires in 10 minutes", "482913", Kind::Code),
            ("Seu código de verificação é 482913", "482913", Kind::Code),
            ("Tu código de verificación es 4821", "4821", Kind::Code),
            ("Votre code de vérification est 4821", "4821", Kind::Code),
            ("Ihr Bestätigungscode lautet 4821", "4821", Kind::Code),
            ("رمز التحقق الخاص بك هو 4821", "4821", Kind::Code),
            (
                "Account number 1234-5678-9012",
                "1234-5678-9012",
                Kind::Account,
            ),
            (
                "Account 0021 1234 5678 at the bank",
                "0021 1234 5678",
                Kind::Account,
            ),
            (
                "iban de89370400440532013000",
                "de89370400440532013000",
                Kind::Iban,
            ),
            (
                "iban gb82 west 1234 5698 7654 32",
                "gb82 west 1234 5698 7654 32",
                Kind::Iban,
            ),
            ("Mir 2200123456789019", "2200123456789019", Kind::Card),
            ("Maestro 5018123456789014", "5018123456789014", Kind::Card),
            ("RuPay 6070123456789017", "6070123456789017", Kind::Card),
            ("RuPay 8170123456789012", "8170123456789012", Kind::Card),
            (
                "Account and routing number 021000021",
                "021000021",
                Kind::Routing,
            ),
        ] {
            let found = identity_numbers(text);
            let spans: Vec<(&str, Kind)> = found
                .iter()
                .map(|one| (&text[one.start..one.end], one.kind))
                .collect();
            assert_eq!(spans, vec![(held, kind)], "{text}");
        }
    }

    /// The closest label decides between kinds that share a shape.
    #[test]
    fn an_account_and_its_routing_number_are_told_apart() {
        let text = "Account 000123456789, routing 021000021";
        let kinds: Vec<Kind> = identity_numbers(text).iter().map(|one| one.kind).collect();
        assert_eq!(kinds, vec![Kind::Account, Kind::Routing]);
    }

    /// Dates, prices, phone numbers and order numbers beside the new labels, or with a card's
    /// length, stay as written.
    #[test]
    fn dates_prices_and_phone_numbers_near_the_new_labels_are_not_held() {
        for text in [
            "Your code expires on 2026-10-23, or 10/23/2026.",
            "Promo code SAVE20 takes $1,234,567.89 off; total $123456.78",
            "Questions about your account? Call 1-800-555-0199.",
            "Your code line is (617) 555-1234 or 617 555 1234",
            "Account manager: +1 617 555 0123",
            "PIN reset: call 6175551234",
            "The code is 2026, the ZIP code 02139",
            "Card ending in 4242, account ending 6789",
            "Account 12345 is closed",
            "Routing 123456789 is not a routing number",
            "Order 4111111111111112 shipped",
            "Tracking 1Z999AA10123456784 and 9400111899223197428490",
            "Call 617-555-1234 or +1 617 555 1234 about card 12/27",
            "Code 123-456-7890 is a phone number",
            "Invoice INV12345678901234 and receipt AB12CDEFGHIJKLMNOP",
            "Your account? Call +1 617 555 0123 or 617-555-0123",
            "Order 123456789012 shipped to your account",
            "Account balance: $1234567, goal 2500000 USD",
            "Savings goal: € 2500000",
            "Your postal code 123456, zip code 021390",
            "Promo code 123456 and error code 654321",
            "Your code is valid until 20261023",
            "Please verify your shipping address. Your order 87654321",
            "Thanks for your verification. Your flight 4417",
            "A one-time fee of 1250",
            "OTP sent. Your order total is 15000",
            "Security code updated. Room 1204",
            "482913, sent 0412",
            "[verification code · withheld] valid for 1500 seconds",
            "Account help: 6175550123",
        ] {
            assert_eq!(shown(text), text, "{text}");
        }
    }

    #[test]
    fn labels_match_whole_words_only() {
        assert_eq!(shown("destination 123456789"), "destination 123456789");
        assert_eq!(shown("Martin 123456789"), "Martin 123456789");
    }

    #[test]
    fn text_with_nothing_to_redact_is_untouched() {
        let letter = "Dear Ada,\n\nYour permit renewal was approved.\n";
        assert_eq!(redact(letter).as_str(), letter);
        assert_eq!(shown(""), "");
    }

    #[test]
    fn multibyte_text_around_a_number_survives() {
        assert_eq!(
            shown("Numéro SSN — 123456789 — fin"),
            "Numéro SSN — [redacted] — fin"
        );
    }

    fn unlinked(text: &str) -> String {
        unlink(text)
    }

    /// The sign-in links a scan of the live transcripts found, in the shapes they arrive.
    #[test]
    fn a_links_credential_parameters_are_redacted() {
        for (text, expected) in [
            (
                "Reset: https://example.test/reset?token=a1b2c3d4&lang=en",
                "Reset: https://example.test/reset?token=[redacted]&lang=en",
            ),
            (
                "https://example.test/login?uid=42&access_token=abc.def",
                "https://example.test/login?uid=42&access_token=[redacted]",
            ),
            (
                "https://example.test/cb#id_token=xyz&state=s1",
                "https://example.test/cb#id_token=[redacted]&state=s1",
            ),
            (
                "https://example.test/auth?oobCode=Zz-9_&mode=resetPassword",
                "https://example.test/auth?oobCode=[redacted]&mode=resetPassword",
            ),
            (
                "https://bucket.example.test/f.pdf?X-Amz-Credential=AK%2F20260930&X-Amz-Signature=9f8e",
                "https://bucket.example.test/f.pdf?X-Amz-Credential=[redacted]&X-Amz-Signature=[redacted]",
            ),
            (
                "https://example.test/a?api_key=k1;session=s2",
                "https://example.test/a?api_key=[redacted];session=[redacted]",
            ),
            (
                "(see https://example.test/v?code=123456).",
                "(see https://example.test/v?code=[redacted]).",
            ),
            (
                r#"{"url":"https://example.test/i?sig=abc","n":1}"#,
                r#"{"url":"https://example.test/i?sig=[redacted]","n":1}"#,
            ),
        ] {
            assert_eq!(unlinked(text), expected, "{text}");
        }
    }

    /// A tracking link wraps the sign-in link percent-encoded; its tail goes with the token, which
    /// is the safe side.
    #[test]
    fn a_percent_encoded_credential_is_redacted_inside_another_link() {
        assert_eq!(
            unlinked(
                "https://l.example.test/?u=https%3A%2F%2Fexample.test%2Flogin%3Ftoken%3Dsecret1%26next%3D%2F&e=1"
            ),
            "https://l.example.test/?u=https%3A%2F%2Fexample.test%2Flogin%3Ftoken%3D[redacted]&e=1"
        );
    }

    #[test]
    fn a_jwt_is_redacted_anywhere() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEifQ.c2lnbmF0dXJl";
        assert_eq!(
            unlinked(&format!("Bearer {jwt} expires soon")),
            "Bearer [redacted] expires soon"
        );
        assert_eq!(
            unlinked(&format!("https://example.test/invite/{jwt}")),
            "https://example.test/invite/[redacted]"
        );
        assert_eq!(
            unlinked(&format!("https://example.test/v?t={jwt}&x=1")),
            "https://example.test/v?t=[redacted]&x=1"
        );
    }

    #[test]
    fn a_links_password_is_redacted() {
        assert_eq!(
            unlinked("fetch https://ada:hunter2@example.test/feed"),
            "fetch https://ada:[redacted]@example.test/feed"
        );
    }

    /// Links that carry nothing to redact, and words that look like a parameter but are not one.
    #[test]
    fn a_link_without_a_credential_is_untouched() {
        for text in [
            "https://example.test/permit?id=42&lang=en#section-2",
            "https://example.test/search?q=token+code+key",
            "https://example.test/a?tokenize=1&keyboard=us&signal=3",
            "The code is on the letter, and the key is under the mat.",
            "token=abc with no separator before it",
            "eyJ alone is not a token, nor is eyJabc",
            "mailto:ada@example.test",
            "p0123456789abcdef0123456789abcdef and 2026-09-30T09:00:00.123456",
        ] {
            assert_eq!(unlinked(text), text, "{text}");
        }
    }

    /// Every ending in the list is a credential, in any case, as a whole name or a suffix.
    #[test]
    fn every_listed_name_is_a_credential_whole_or_as_an_ending() {
        for name in CREDENTIAL_PARAMETERS {
            for spelled in [(*name).to_owned(), name.to_uppercase(), format!("x_{name}")] {
                assert_eq!(
                    unlinked(&format!("https://example.test/?a=1&{spelled}=v4lue&b=2")),
                    format!("https://example.test/?a=1&{spelled}=[redacted]&b=2"),
                    "{spelled}"
                );
            }
        }
    }

    /// Redacting twice is redacting once, so a line read back and stored again does not move.
    #[test]
    fn the_link_pass_is_idempotent() {
        let text = "https://ada:pw@example.test/r?token=a&code=b eyJa.eyJb.c";
        assert_eq!(unlinked(&unlinked(text)), unlinked(text));
    }

    // The scrub's number patterns as they were before the fast path, verbatim but for the names:
    // the reference the fast path is checked against.
    /// Every code in `text`, by [`Labels`]' code labels.
    fn codes_before(text: &str, labels: &Labels) -> Vec<Candidate> {
        let Labels {
            named,
            strong,
            weak,
            not_codes,
            ..
        } = labels;
        let right_before: Vec<Label> = weak
            .iter()
            .copied()
            .filter(|&(label_start, label_end, _)| {
                text[label_start..label_end].eq_ignore_ascii_case("code")
            })
            .collect();
        CODE.find_iter(text)
            .filter_map(|found| {
                let (start, end) = (found.start(), found.end());
                let digits = digits_of(found.as_str());
                let code = |labels: &[Label], reach: usize, before: bool| {
                    claims(text, labels, (start, end), reach, before)
                };
                let gap = [
                    code(named, STRONG_NEAR, true),
                    code(named, NEAR, false),
                    code(strong, NEAR, false),
                    code(&right_before, 2, true),
                    (digits.len() >= 6)
                        .then(|| code(weak, NEAR, false))
                        .flatten(),
                ]
                .into_iter()
                .flatten()
                .min()?;
                let other =
                    closest(text, not_codes, start, end).map_or(usize::MAX, |(_, other)| other);
                (gap < other && alone(text, start, end) && !dated(&digits))
                    .then(|| candidate(start, end, Kind::Code, Strength::Code))
            })
            .collect()
    }

    /// Every held number this module recognises in `text`, in order and never overlapping: two
    /// matches that overlap are one, from the first one's start to the later end, of the kind of
    /// the strongest: a code, then a kind whose checksum held, then the closest label.
    fn identity_numbers_before(text: &str) -> Vec<Found> {
        let labels = Labels::of(text);
        let Labels {
            identity,
            accounts,
            not_accounts,
            routings,
            ..
        } = &labels;
        let mut spans: Vec<Candidate> = WRITTEN_OUT
            .find_iter(text)
            .map(|found| {
                let (kind, strength) = closest(text, identity, found.start(), found.end()).map_or(
                    (
                        if found.as_str().starts_with('9') {
                            Kind::Itin
                        } else {
                            Kind::Ssn
                        },
                        Strength::Shaped,
                    ),
                    |(kind, gap)| (kind, Strength::Labelled(gap)),
                );
                candidate(found.start(), found.end(), kind, strength)
            })
            .collect();
        spans.extend(NINE_DIGITS.find_iter(text).filter_map(|found| {
            closest(text, identity, found.start(), found.end()).map(|(kind, gap)| {
                candidate(found.start(), found.end(), kind, Strength::Labelled(gap))
            })
        }));
        spans.extend(codes_before(text, &labels));
        spans.extend(ACCOUNT.find_iter(text).filter_map(|found| {
            let (start, end) = (found.start(), found.end());
            if !account(found.as_str()) || !alone(text, start, end) {
                return None;
            }
            let other = closest(text, not_accounts, start, end).map_or(usize::MAX, |(_, gap)| gap);
            closest(text, accounts, start, end)
                .filter(|&(_, gap)| gap < other)
                .map(|(kind, gap)| candidate(start, end, kind, Strength::Labelled(gap)))
        }));
        spans.extend(ROUTING.find_iter(text).filter_map(|found| {
            let (start, end) = (found.start(), found.end());
            (aba(found.as_str())
                && alone(text, start, end)
                && closest(text, routings, start, end).is_some())
            .then(|| candidate(start, end, Kind::Routing, Strength::Checked))
        }));
        spans.extend(cards_before(text));
        spans.extend(
            IBAN.find_iter(text)
                .filter(|found| iban(found.as_str()))
                .map(|found| candidate(found.start(), found.end(), Kind::Iban, Strength::Checked)),
        );
        let hints: Vec<std::ops::Range<usize>> = HINT
            .captures_iter(text)
            .filter_map(|captures| captures.get(1))
            .map(|hint| hint.range())
            .collect();
        spans.retain(|one| {
            !hints
                .iter()
                .any(|hint| one.found.start < hint.end && hint.start < one.found.end)
        });
        spans.sort_unstable_by_key(|one| (one.found.start, one.found.end));
        let mut merged: Vec<Candidate> = Vec::with_capacity(spans.len());
        for one in spans {
            match merged.last_mut() {
                Some(last) if one.found.start < last.found.end => {
                    last.found.end = last.found.end.max(one.found.end);
                    if one.strength < last.strength {
                        last.found.kind = one.found.kind;
                        last.strength = one.strength;
                    }
                }
                Some(_) | None => merged.push(one),
            }
        }
        merged.into_iter().map(|one| one.found).collect()
    }

    /// Every card number in `text`: a stretch of digit groups of a card's length that starts with an
    /// issuer's prefix and passes the Luhn check. A run after a `+` is a phone number.
    fn cards_before(text: &str) -> Vec<Candidate> {
        let mut found = Vec::new();
        for run in GROUPS.find_iter(text) {
            if text[..run.start()].ends_with('+') {
                continue;
            }
            let groups: Vec<(usize, usize)> = digit_groups(run.as_str())
                .map(|(start, end)| (run.start() + start, run.start() + end))
                .collect();
            let mut first = 0;
            while first < groups.len() {
                let card = (first..groups.len()).find(|&last| {
                    let digits: String = groups[first..=last]
                        .iter()
                        .map(|&(start, end)| &text[start..end])
                        .collect();
                    card(&digits)
                });
                match card {
                    Some(last) => {
                        found.push(candidate(
                            groups[first].0,
                            groups[last].1,
                            Kind::Card,
                            Strength::Checked,
                        ));
                        first = last + 1;
                    }
                    None => first += 1,
                }
            }
        }
        found
    }

    /// The scrub before the fast path, as it was.
    fn scrubbed_before(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for found in identity_numbers_before(text) {
            out.push_str(&text[at..found.start]);
            out.push_str(REDACTED);
            at = found.end;
        }
        out.push_str(&text[at..]);
        let out = USERINFO.replace_all(&out, format!("${{1}}{REDACTED}@").as_str());
        let out = CREDENTIAL.replace_all(&out, format!("${{1}}{REDACTED}").as_str());
        JWT.replace_all(&out, REDACTED).into_owned()
    }

    /// Mixed text, in the shapes a conversation holds: words of several scripts, the labels,
    /// numbers alone and in groups, links with credentials, tokens, placeholders and line breaks.
    fn mixed() -> impl Strategy<Value = String> {
        let piece = prop_oneof![
            Just("the ".to_owned()),
            Just("café ".to_owned()),
            Just("Numéro ".to_owned()),
            Just("رمز التحقق ".to_owned()),
            Just("الخاص بك هو ".to_owned()),
            Just("確認コード ".to_owned()),
            Just("😀".to_owned()),
            Just("é".to_owned()),
            Just("\u{a0}".to_owned()),
            Just("—".to_owned()),
            Just("€".to_owned()),
            Just("+".to_owned()),
            Just("\n".to_owned()),
            Just(" ".to_owned()),
            Just("-".to_owned()),
            Just("SSN ".to_owned()),
            Just("EIN: ".to_owned()),
            Just("verification code ".to_owned()),
            Just("código de verificação ".to_owned()),
            Just("Bestätigungscode ".to_owned()),
            Just("code: ".to_owned()),
            Just("PIN ".to_owned()),
            Just("zip code ".to_owned()),
            Just("Account number ".to_owned()),
            Just("routing ".to_owned()),
            Just("order ".to_owned()),
            Just("IBAN ".to_owned()),
            Just("GB82 WEST 1234 5698 7654 32".to_owned()),
            Just("de89370400440532013000".to_owned()),
            Just("4111 1111 1111 1111".to_owned()),
            Just("378282246310005".to_owned()),
            Just("021000021".to_owned()),
            Just("123-45-6789".to_owned()),
            Just("[verification code · withheld]".to_owned()),
            Just("[card ending 4242 · tap to show · x]".to_owned()),
            Just("https://ada:pw@example.test/r?token=a1&lang=é".to_owned()),
            Just("https://example.test/a?x=1%26code%3Dzz9".to_owned()),
            Just("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEifQ.c2ln".to_owned()),
            "[0-9]{1,19}",
            "[0-9]{2,4}([ -][0-9]{2,4}){0,4}",
            Just("https://example.test/a?code%3Dzz9&x".to_owned()),
            Just("ſ".to_owned()),
            Just("\u{212a}".to_owned()),
            Just("ſSN ".to_owned()),
            Just("٤٨٢٩١٣".to_owned()),
            Just("４８２９１３".to_owned()),
            Just("4111\u{a0}1111 1111 1111".to_owned()),
            Just("4111 1111 é111 1111".to_owned()),
            Just("GB82 WÉST 1234 5698 7654 32".to_owned()),
            Just("GB82 ſEST 1234 5698 7654 32".to_owned()),
            Just("DE89 \u{212a}704 0044 0532 0130 00".to_owned()),
            Just("ſE12 GB82 WEST 1234 5698 7654 32".to_owned()),
            "[a-zA-Z_]{1,8}",
            "\\PC{1,6}",
        ];
        proptest::collection::vec(piece, 0..24).prop_map(|pieces| pieces.concat())
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(2048))]
        /// The fast path changes no result: the ASCII copy, the prefilters and the labels read
        /// lazily scrub every mixed text as the scrub before them did.
        #[test]
        fn the_fast_scrub_is_the_scrub_before_it(text in mixed()) {
            proptest::prop_assert_eq!(unlink(redact(&text).as_str()), scrubbed_before(&text));
        }
    }

    /// An IBAN-shaped match holding a letter outside ASCII is no IBAN, and is never split inside
    /// that letter (the scrub once panicked on this text, the property's shrunk case).
    #[test]
    fn an_iban_match_holding_a_letter_outside_ascii_is_none() {
        let text = "\u{17f}\u{212a}378282246310005code: ";
        assert_eq!(unlink(redact(text).as_str()), scrubbed_before(text));
    }

    /// The rules' name moves whenever what the scrub does to this corpus moves
    /// ([`Versioned::pinned`]).
    #[test]
    fn the_rules_move_when_the_scrub_moves() {
        let corpus = [
            "SSN 123-45-6789, EIN: 12-3456789, ITIN 912701234",
            "Your verification code is 482913. Seu código de verificação é 4821",
            "Card 4111 1111 1111 1111, Amex 378282246310005, IBAN GB82 WEST 1234 5698 7654 32",
            "Account number: 000123456789, routing 021000021, order 123456789012",
            "https://ada:pw@example.test/r?token=a1&code=b2 eyJa.eyJb.c and 2026-10-25",
            "Call 617-555-1234; code expires 2026-10-23; promo code 123456",
        ];
        let scrubbed = corpus.map(|text| unlink(redact(text).as_str()));
        if let Err(moved) = Scrub::pinned((), scrubbed, PINNED_RULES, PINNED_CORPUS) {
            panic!("{moved}");
        }
    }

    const PINNED_RULES: &str = "a0d5cffe58839d79401ab650f475384a5eb7fbc064b46cfb3b7a3b25ac91f143";
    const PINNED_CORPUS: &str = "afe74587a557301a25355154fa7c82b8f0dc73fdeb3b2b8d168c00f50ca6b2f3";

    #[test]
    fn rules_print_and_read_back() {
        let rules = Rules::current(()).clone();
        assert_eq!(Rules::new(rules.as_str()), Ok(rules.clone()));
        assert!(Rules::new("not hex").is_err());
        let json = serde_json::to_string(&rules).expect("prints");
        assert_eq!(json, format!("\"{rules}\""));
        assert_eq!(serde_json::from_str::<Rules>(&json).expect("reads"), rules);
        assert!(serde_json::from_str::<Rules>("\"not hex\"").is_err());
    }

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Said {
        text: String,
        args: std::collections::BTreeMap<String, Vec<String>>,
        count: u32,
    }

    #[test]
    fn a_scrubbed_value_has_every_string_scrubbed_and_keys_too() {
        let said = Said {
            text: "SSN 123-45-6789, link https://example.test/?token=t".to_owned(),
            args: [(
                "https://example.test/?key=k".to_owned(),
                vec!["eyJh.eyJp.sig".to_owned(), "plain".to_owned()],
            )]
            .into(),
            count: 3,
        };
        let scrubbed = Scrubbed::of(&said).expect("scrubs").into_inner();
        assert_eq!(
            scrubbed,
            Said {
                text: "SSN [redacted], link https://example.test/?token=[redacted]".to_owned(),
                args: [(
                    "https://example.test/?key=[redacted]".to_owned(),
                    vec!["[redacted]".to_owned(), "plain".to_owned()],
                )]
                .into(),
                count: 3,
            }
        );
    }

    /// A field whose own constructor refuses the replacement refuses the value, never stores it.
    #[test]
    fn a_value_that_cannot_hold_its_redaction_is_refused() {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        #[serde(try_from = "String", into = "String")]
        struct Plain(String);
        impl TryFrom<String> for Plain {
            type Error = String;
            fn try_from(text: String) -> Result<Self, String> {
                if text.contains('[') {
                    Err("no brackets".to_owned())
                } else {
                    Ok(Self(text))
                }
            }
        }
        impl From<Plain> for String {
            fn from(plain: Plain) -> String {
                plain.0
            }
        }
        assert!(Scrubbed::of(&Plain("https://example.test/?token=t".to_owned())).is_err());
        assert!(Scrubbed::of(&Plain("nothing here".to_owned())).is_ok());
    }
}
