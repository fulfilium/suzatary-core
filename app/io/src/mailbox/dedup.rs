//! The canonical hash every `state.json` entry and todo `source.hash` is keyed on: sha256 of the
//! provider-stable headers, never the body, `Message-ID` or a transit header, which a re-fetch
//! mutates. A wrong hash silently unlinks a todo from its mail.
//!
//! Each normalisation follows `CPython`'s `email` module rather than RFC 5322, because that
//! module produced every hash already on disk; where `mail-parser` disagrees, `CPython` wins.

use std::sync::LazyLock;

use mail_parser::{HeaderName, HeaderValue, Message, MessageParser};
use regex::Regex;
use sha2::{Digest, Sha256};

/// One of the five headers hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashHeader {
    /// `Date:`, as its calendar day.
    Date,
    From,
    To,
    Cc,
    /// `Subject:`, without its reply and list markers.
    Subject,
}

impl HashHeader {
    /// Every header hashed, in order; the order is part of the canonical form.
    pub const ALL: [HashHeader; 5] = [
        HashHeader::Date,
        HashHeader::From,
        HashHeader::To,
        HashHeader::Cc,
        HashHeader::Subject,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            HashHeader::Date => "date",
            HashHeader::From => "from",
            HashHeader::To => "to",
            HashHeader::Cc => "cc",
            HashHeader::Subject => "subject",
        }
    }

    /// Its value in `message`, normalised by its kind; an absent header and an empty one both
    /// hash as `""`.
    fn field(self, message: &Message<'_>) -> String {
        match self {
            HashHeader::Date => normalize_date(message.header_raw(HeaderName::Date)),
            HashHeader::From => normalize_addresses(message.header_values(HeaderName::From)),
            HashHeader::To => normalize_addresses(message.header_values(HeaderName::To)),
            HashHeader::Cc => normalize_addresses(message.header_values(HeaderName::Cc)),
            HashHeader::Subject => message.subject().map(normalize_subject).unwrap_or_default(),
        }
    }
}

fn collapse_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// One or more leading `Re:`/`Fwd:`/`[list]` markers, each followed by whitespace; `[...]`
/// admits only a `\w`-bounded tag.
static SUBJECT_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:(?:re|fwd?):\s+|\[\w[\w_-]*\w?\]\s+)+").expect("a literal pattern")
});

fn normalize_subject(decoded: &str) -> String {
    let collapsed = collapse_ws(decoded);
    SUBJECT_PREFIX.replace(&collapsed, "").trim().to_owned()
}

/// Every address across every occurrence of an address header, lower-cased, trimmed,
/// de-duplicated and sorted, display names dropped. UTF-8 byte order is scalar-value order, so
/// a `BTreeSet` sorts as Python's `sorted()` does.
fn normalize_addresses<'a>(occurrences: impl Iterator<Item = &'a HeaderValue<'a>>) -> String {
    let mut addrs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for value in occurrences {
        let Some(address) = value.as_address() else {
            // A garbled header: CPython's tokenizer would extract something, so this is a known,
            // rare divergence from the hashes on disk.
            continue;
        };
        for addr in address.iter() {
            if let Some(raw) = &addr.address {
                let normalized = raw.trim().to_lowercase();
                if !normalized.is_empty() {
                    addrs.insert(normalized);
                }
            }
        }
    }
    addrs.into_iter().collect::<Vec<_>>().join(", ")
}

// The hash takes the calendar date CPython's parser reads from the header, with no timezone
// shift; `mail_parser::DateTime` disagrees on two-digit years, so the grammar is ported here.

const DAYNAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// The month by its three-letter prefix, case-insensitively: looser than `CPython`'s table, and
/// in agreement on every real spelling.
fn month_number(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    let prefix = lower.get(0..3)?;
    Some(match prefix {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

/// The obsolete American zone abbreviations, as the "HHMM"-shaped integer [`hhmm_to_minutes`]
/// reads.
fn named_zone(token: &str) -> Option<i32> {
    Some(match token.to_ascii_uppercase().as_str() {
        "UT" | "UTC" | "GMT" | "Z" => 0,
        "AST" | "EDT" => -400,
        "ADT" => -300,
        "EST" | "CDT" => -500,
        "CST" | "MDT" => -600,
        "MST" | "PDT" => -700,
        "PST" => -800,
        _ => return None,
    })
}

/// An "HHMM"-style signed integer (`-500`, `330`) to minutes east of UTC.
fn hhmm_to_minutes(value: i32) -> i32 {
    let sign = if value < 0 { -1 } else { 1 };
    let magnitude = value.abs();
    sign * ((magnitude / 100) * 60 + magnitude % 100)
}

/// A `Date:` header's fields as `CPython` reads them, never shifted or validated against a
/// calendar ([`ParsedDate::to_naive_datetime`] does that).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedDate {
    /// The four-digit year, after `CPython`'s two-digit correction.
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// Minutes east of UTC, or `None` when the header named no zone the grammar resolves.
    pub tz_offset_minutes: Option<i32>,
}

impl ParsedDate {
    /// The header's numbers as a naive instant, no timezone applied; `None` when they form no
    /// real calendar instant (a day of 32).
    #[must_use]
    pub fn to_naive_datetime(&self) -> Option<prodrome_core::literal::Datetime> {
        prodrome_core::literal::Datetime::new(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            0,
        )
        .ok()
    }

    /// Seconds since the epoch, for newest-first ordering only, never the hash. A missing zone
    /// reads as UTC rather than the box's local zone, so the order does not depend on the box.
    #[must_use]
    pub fn epoch_seconds(&self) -> Option<i64> {
        let date = chrono::NaiveDate::from_ymd_opt(self.year, self.month, self.day)?;
        let naive = date.and_hms_opt(self.hour, self.minute, self.second)?;
        let offset_seconds = i64::from(self.tz_offset_minutes.unwrap_or(0)) * 60;
        Some(naive.and_utc().timestamp() - offset_seconds)
    }
}

/// `CPython`'s `_parsedate_tz` for the shapes real headers use (`"Mon, 2 Jan 2006 15:04:05
/// -0700"`, day name and zone space optional). Its RFC 850, transposed-year and dotted-time
/// fallbacks are not ported: such a header is `None` and hashes as its collapsed raw text.
#[must_use]
pub fn parse_rfc2822_date(text: &str) -> Option<ParsedDate> {
    let mut tokens: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    if tokens.is_empty() {
        return None;
    }

    let first_lower = tokens[0].to_ascii_lowercase();
    if tokens[0].ends_with(',') || DAYNAMES.contains(&first_lower.trim_end_matches(',')) {
        tokens.remove(0);
    } else if let Some(at) = tokens[0].rfind(',') {
        let after_comma = tokens[0][at + 1..].to_owned();
        tokens[0] = after_comma;
    }
    if tokens.is_empty() {
        return None;
    }

    // A time and zone glued together ("15:04:05-0700") come back as one token.
    if tokens.len() == 4 {
        let last = tokens[3].clone();
        let split_at = last
            .find('+')
            .or_else(|| last.rfind('-').filter(|&i| i > 0));
        if let Some(i) = split_at {
            let (time, zone) = last.split_at(i);
            time.clone_into(&mut tokens[3]);
            tokens.push(zone.to_owned());
        } else {
            tokens.push(String::new());
        }
    }
    if tokens.len() < 5 {
        return None;
    }
    tokens.truncate(5);
    let mut it = tokens.into_iter();
    let (mut dd, mut mm, mut yy, tm, mut tz) =
        (it.next()?, it.next()?, it.next()?, it.next()?, it.next()?);

    if month_number(&mm).is_none() {
        std::mem::swap(&mut mm, &mut dd);
    }
    let month = month_number(&mm)?;

    if !yy
        .trim_end_matches(',')
        .starts_with(|c: char| c.is_ascii_digit())
    {
        std::mem::swap(&mut yy, &mut tz);
    }
    let yy = yy.trim_end_matches(',');
    let dd = dd.trim_end_matches(',');
    let tm = tm.trim_end_matches(',');

    let time_parts: Vec<&str> = tm.split(':').collect();
    let (hh, min, ss) = match time_parts.as_slice() {
        [h, m] => (*h, *m, "0"),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };

    let day: u32 = dd.parse().ok()?;
    let mut year: i32 = yy.parse().ok()?;
    let hour: u32 = hh.parse().ok()?;
    let minute: u32 = min.parse().ok()?;
    let second: u32 = ss.parse().ok()?;

    // CPython's cutoff: 0..=68 is the 2000s, 69..=99 the 1900s. `mail_parser` splits at 49/50,
    // so the two disagree on 50 through 68.
    if year < 100 {
        year += if year > 68 { 1900 } else { 2000 };
    }

    let numeric_tz = tz.parse::<i32>().ok();
    let raw_offset = named_zone(&tz).or(numeric_tz);
    let tz_offset_minutes = match raw_offset {
        // "-0000" means unknown, not UTC.
        Some(0) if tz.starts_with('-') => None,
        Some(v) => Some(hhmm_to_minutes(v)),
        None => None,
    };

    Some(ParsedDate {
        year,
        month,
        day,
        hour,
        minute,
        second,
        tz_offset_minutes,
    })
}

/// The `date` field of [`canonical_form`]: `YYYY-MM-DD` when the header parses to a real
/// calendar instant, else the whitespace-collapsed raw text.
fn normalize_date(raw: Option<&str>) -> String {
    let parsed = raw
        .and_then(parse_rfc2822_date)
        .and_then(|p| p.to_naive_datetime());
    match parsed {
        Some(dt) => format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day()),
        None => raw.map(collapse_ws).unwrap_or_default(),
    }
}

/// The `name:value` blob over [`HashHeader::ALL`], in order, that [`canonical_hash`] hashes;
/// bytes that do not parse give five empty fields, never an error.
#[must_use]
pub fn canonical_form(raw: &[u8]) -> String {
    let message = MessageParser::default().parse(raw);
    HashHeader::ALL
        .iter()
        .map(|header| {
            let value = message
                .as_ref()
                .map(|m| header.field(m))
                .unwrap_or_default();
            format!("{}:{value}", header.name())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The dedup identity: sha256 of [`canonical_form`], as lowercase hex.
#[must_use]
pub fn canonical_hash(raw: &[u8]) -> String {
    hex::encode(Sha256::digest(canonical_form(raw).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(headers: &str, body: &str) -> Vec<u8> {
        format!("{headers}\r\n\r\n{body}").into_bytes()
    }

    #[test]
    fn the_canonical_form_is_pinned_to_an_exact_blob() {
        let raw = message(
            "Date: Mon, 7 Sep 2026 12:00:00 -0400\r\n\
             From: Bob <bob@example.com>\r\n\
             To: Alice <alice@example.com>\r\n\
             Subject: Re: hello",
            "hi",
        );
        assert_eq!(
            canonical_form(&raw),
            "date:2026-09-07\n\
             from:bob@example.com\n\
             to:alice@example.com\n\
             cc:\n\
             subject:hello"
        );
    }

    /// Markers strip repeatedly; a marker not followed by whitespace is not a marker:
    /// `"Re:hello"` is a subject.
    #[test]
    fn re_fwd_and_list_markers_strip_repeatedly() {
        for (subject, normal) in [
            ("Re: Fwd: [list] hello", "hello"),
            ("RE: re: hello", "hello"),
            ("[a-list] [another] hello", "hello"),
            ("Re:hello", "Re:hello"),
        ] {
            assert_eq!(normalize_subject(subject), normal, "{subject}");
        }
    }

    #[test]
    fn addresses_sort_lower_and_dedupe_across_two_headers() {
        let raw = message(
            "Date: Mon, 7 Sep 2026 12:00:00 -0400\r\n\
             From: a@x.test\r\n\
             To: \"Zed\" <ZED@X.test>\r\n\
             To: bob@x.test, zed@x.test\r\n\
             Subject: hi",
            "hi",
        );
        let form = canonical_form(&raw);
        let to_line = form
            .lines()
            .find(|l| l.starts_with("to:"))
            .expect("a to line");
        assert_eq!(to_line, "to:bob@x.test, zed@x.test");
    }

    #[test]
    fn a_date_that_will_not_parse_falls_back_to_collapsed_raw_text() {
        let raw = message(
            "Date: not   a   date\r\nFrom: a@x.test\r\nSubject: hi",
            "hi",
        );
        let form = canonical_form(&raw);
        let date_line = form
            .lines()
            .find(|l| l.starts_with("date:"))
            .expect("a date line");
        assert_eq!(date_line, "date:not a date");
    }

    #[test]
    fn a_missing_cc_hashes_as_empty() {
        let raw = message(
            "Date: Mon, 7 Sep 2026 12:00:00 -0400\r\nFrom: a@x.test\r\nSubject: hi",
            "hi",
        );
        let form = canonical_form(&raw);
        assert!(form.lines().any(|l| l == "cc:"));
    }

    #[test]
    fn an_rfc_2047_encoded_subject_hashes_the_same_as_its_decoded_form() {
        let encoded = message(
            "Date: Mon, 7 Sep 2026 12:00:00 -0400\r\n\
             From: a@x.test\r\n\
             Subject: =?UTF-8?B?aGVsbG8=?=",
            "hi",
        );
        let decoded = message(
            "Date: Mon, 7 Sep 2026 12:00:00 -0400\r\n\
             From: a@x.test\r\n\
             Subject: hello",
            "hi",
        );
        assert_eq!(canonical_hash(&encoded), canonical_hash(&decoded));
    }

    #[test]
    fn a_two_digit_year_takes_cpythons_cutoff_not_mail_parsers() {
        // 68 is 2068 by CPython's rule; `mail_parser` would read 1968.
        let parsed = parse_rfc2822_date("7 Sep 68 12:00:00 +0000").expect("parses");
        assert_eq!(parsed.year, 2068);
        let parsed = parse_rfc2822_date("7 Sep 69 12:00:00 +0000").expect("parses");
        assert_eq!(parsed.year, 1969);
    }

    #[test]
    fn a_named_us_zone_resolves_to_its_fixed_offset() {
        let parsed = parse_rfc2822_date("Mon, 7 Sep 2026 12:00:00 EST").expect("parses");
        assert_eq!(parsed.tz_offset_minutes, Some(-5 * 60));
    }
}
