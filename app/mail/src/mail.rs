//! The mail corpus's values: bodies are always text, a cut body says so, and triage's
//! disposition is a badge, never a filter.

pub mod account;
pub mod account_email;
pub mod address;
pub mod cache;
pub mod declared;
pub mod dedup_key;
pub mod fault;
pub mod folders;
pub mod grants;
pub mod hash_prefix;
pub mod idle;
pub mod imap;
pub mod lacking;
pub mod mail_sync;
pub mod mailbox;
pub mod mailroom;
pub mod oauth;
pub mod outbox;
pub mod puller;
pub mod removal;
pub mod send;
pub mod sync_now;
pub mod sync_report;
pub mod transport;

use serde::{Deserialize, Serialize};
use suzatary_held::message_part::MailHeader;
use ts_rs::TS;

use suzatary_base::capability::Datetime;

/// A link as it appeared in the mail; the label is what tells lookalike links apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailLink {
    /// Where it goes, filtered to http/https/mailto before it reaches a page.
    pub url: String,
    /// The text it stood behind, "" for a bare URL.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailRow {
    /// The canonical content hash, of which a todo's `source.hash` is a prefix.
    pub hash: String,
    /// The notmuch locator, for a second read.
    pub id: String,
    /// The From address, lower-cased.
    pub sender: String,
    /// The Subject, RFC 2047 decoded.
    pub subject: String,
    /// When it arrived, `None` when the Date header would not parse (`""` on the wire).
    #[serde(with = "date")]
    #[ts(type = "string")]
    pub date: Option<Datetime>,
    pub container: String,
    /// The account's canonical address, "" for a maildir not of this deployment.
    pub account: String,
    pub disposition: Disposed,
    /// The kinds of held value it names, which the row says and never their values.
    #[ts(as = "Vec<suzatary_held::redact::Kind>")]
    pub held: suzatary_held::held_value::Kinds,
}

/// What triage decided about a message, as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Disposed {
    Undecided,
    /// Actionable: a todo was authored for it.
    Todo {
        /// The todo, by id.
        todo: String,
    },
    /// Automated, marketing or bot mail.
    Noise,
    /// Seen, not actionable, not noise.
    Skip,
    /// Unknown here: this device holds no dedup key, which triage's record is read by.
    Unkeyed,
}

/// One message as a page; which todos it produced is the chain's fact, joined by the route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MailMessage {
    pub row: MailRow,
    /// notmuch's conversation id, "" when the corpus names none.
    pub thread: String,
    /// The whitelisted headers it carries, in reading order.
    pub headers: Vec<MailHeader>,
    pub body: String,
    pub truncated: bool,
    pub links: Vec<MailLink>,
}

/// The cap on a body's characters; past it the wire says `truncated`.
pub const MAX_BODY_CHARS: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMeta {
    /// Its filename from the disposition or the content type, `""` when neither names one.
    pub name: String,
    /// `type/subtype`, lower-cased, `""` when the part carried none.
    pub mime: String,
    /// Its decoded size in bytes, not the wire encoding's length.
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFile {
    /// Its filename, as [`AttachmentMeta::name`] lists it.
    pub name: String,
    /// `type/subtype`, lower-cased, `""` when the part carried none.
    pub mime: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// The file is larger than the cap asked for, so it was not read.
    TooLarge {
        /// Its size in bytes.
        size: u64,
    },
    Recorded(StoredMessage),
}

/// A message file as a forward records it: its digest, the held values in its text and its
/// attachments, never its words. The tap sends these bytes or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StoredMessage {
    pub id: String,
    /// The file's size in bytes.
    pub size: u64,
    pub sha256: suzatary_held::attachment::AttachmentId,
    /// The held values in its text, in order.
    pub held: Vec<suzatary_held::held_value::Held>,
    /// Its attachments, in the file's order.
    pub parts: Vec<StoredPart>,
}

/// One attachment of a stored message, by its place in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StoredPart {
    /// Its filename, or `attachment-<n>` for the `n`th when it names none.
    pub name: String,
    pub media: String,
    /// Its decoded size in bytes.
    pub size: u64,
    /// The SHA-256 of its decoded bytes.
    pub sha256: suzatary_held::attachment::AttachmentId,
}

pub const STORED_MESSAGE: prodrome_core::literal::codec::Constructor =
    ("StoredMessage", &["id", "size", "sha256", "held", "parts"]);
pub const STORED_PART: prodrome_core::literal::codec::Constructor =
    ("StoredPart", &["name", "media", "size", "sha256"]);

impl prodrome_core::literal::codec::Literal for StoredMessage {
    fn to_value(&self) -> prodrome_core::literal::Value {
        prodrome_core::literal::codec::record(
            STORED_MESSAGE,
            vec![
                self.id.to_value(),
                self.size.to_value(),
                self.sha256.to_value(),
                self.held.to_value(),
                self.parts.to_value(),
            ],
        )
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        let fields = prodrome_core::literal::codec::Fields::of(value, STORED_MESSAGE)?;
        Ok(StoredMessage {
            id: fields.get("id")?,
            size: fields.get("size")?,
            sha256: fields.get("sha256")?,
            held: fields.get("held")?,
            parts: fields.get("parts")?,
        })
    }
}

impl prodrome_core::literal::codec::Literal for StoredPart {
    fn to_value(&self) -> prodrome_core::literal::Value {
        prodrome_core::literal::codec::record(
            STORED_PART,
            vec![
                self.name.to_value(),
                self.media.to_value(),
                self.size.to_value(),
                self.sha256.to_value(),
            ],
        )
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        let fields = prodrome_core::literal::codec::Fields::of(value, STORED_PART)?;
        Ok(StoredPart {
            name: fields.get("name")?,
            media: fields.get("media")?,
            size: fields.get("size")?,
            sha256: fields.get("sha256")?,
        })
    }
}

/// The headers a message page prints, in reading order; a whitelist.
const SHOWN_HEADERS: &[&str] = &["From", "To", "Cc", "Subject", "Date", "Reply-To"];

const LINK_SCHEMES: &[&str] = &["http://", "https://", "mailto:"];

/// A source hash prefix shorter than this names todos by accident, so it is not evidence.
pub(crate) const MIN_SOURCE_PREFIX: usize = 6;

/// At most `len` characters of `text` starting at character `start`: one page of a longer text.
#[must_use]
pub fn page(text: &str, start: usize, len: usize) -> &str {
    let from = suzatary_base::spelling::shorten(text, start).len();
    suzatary_base::spelling::shorten(&text[from..], len)
}

#[must_use]
pub fn web_links(links: Vec<MailLink>) -> Vec<MailLink> {
    links
        .into_iter()
        .filter(|link| {
            let lowered = link.url.to_lowercase();
            LINK_SCHEMES
                .iter()
                .any(|scheme| lowered.starts_with(scheme))
        })
        .collect()
}

#[must_use]
pub fn headers_of(headers: &[(String, String)]) -> Vec<MailHeader> {
    SHOWN_HEADERS
        .iter()
        .filter_map(|name| {
            headers
                .iter()
                .find(|(held, value)| held == name && !value.is_empty())
                .map(|(held, value)| MailHeader {
                    name: held.clone(),
                    value: value.clone(),
                })
        })
        .collect()
}

mod date {
    use prodrome_core::fpl::{datetime_of, instant_of, iso, parse_iso};
    use serde::{Deserialize, Deserializer, Serializer};

    use suzatary_base::capability::Datetime;

    #[expect(clippy::ref_option, reason = "serde's `with` signature")]
    pub fn serialize<S: Serializer>(
        at: &Option<Datetime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&at.map_or_else(String::new, |at| iso(instant_of(at))))
    }

    /// `""` is no date; anything else is an instant `serialize` printed.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Datetime>, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty() {
            return Ok(None);
        }
        parse_iso(&s)
            .map_err(|e| e.to_string())
            .and_then(|t| datetime_of(t).map_err(|e| e.to_string()))
            .map(Some)
            .map_err(serde::de::Error::custom)
    }
}

/// Was this todo authored from the message with this canonical hash (the todo holds a prefix)?
#[must_use]
pub fn sourced_from(source_hash: &str, message_hash: &str) -> bool {
    source_hash.len() >= MIN_SOURCE_PREFIX && message_hash.starts_with(source_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_three_schemes_reach_a_page() {
        let links = vec![
            MailLink {
                url: "https://example.test/a".into(),
                label: "a".into(),
            },
            MailLink {
                url: "JAVASCRIPT:alert(1)".into(),
                label: "b".into(),
            },
            MailLink {
                url: "mailto:her@example.test".into(),
                label: String::new(),
            },
            MailLink {
                url: "data:text/html,<script>".into(),
                label: String::new(),
            },
        ];
        let kept: Vec<String> = web_links(links).into_iter().map(|l| l.url).collect();
        assert_eq!(
            kept,
            vec![
                "https://example.test/a".to_owned(),
                "mailto:her@example.test".to_owned()
            ]
        );
    }

    #[test]
    fn headers_come_back_whitelisted_and_in_reading_order() {
        let held = vec![
            ("Subject".to_owned(), "hello".to_owned()),
            ("X-Spam-Score".to_owned(), "9".to_owned()),
            ("From".to_owned(), "her@example.test".to_owned()),
            ("Cc".to_owned(), String::new()),
        ];
        assert_eq!(
            headers_of(&held),
            vec![
                MailHeader {
                    name: "From".to_owned(),
                    value: "her@example.test".to_owned(),
                },
                MailHeader {
                    name: "Subject".to_owned(),
                    value: "hello".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn the_reverse_index_is_one_directional_and_has_a_floor() {
        assert!(sourced_from("abc123", "abc123def456"));
        assert!(!sourced_from("abc12", "abc123def456"));
        assert!(!sourced_from("", "abc123def456"));
        assert!(!sourced_from("abc123def456", "abc123"));
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn page_never_panics_and_never_splits_a_char(
            text: String,
            start in 0usize..200,
            len in 0usize..200,
        ) {
            let piece = page(&text, start, len);
            prop_assert!(piece.chars().count() <= len);
        }

        #[test]
        fn paging_in_steps_reassembles_the_whole_text(text: String, step in 1usize..50) {
            let mut joined = String::new();
            let mut at = 0;
            loop {
                let piece = page(&text, at, step);
                if piece.is_empty() {
                    break;
                }
                joined.push_str(piece);
                at += step;
            }
            prop_assert_eq!(joined, text);
        }
    }
}
