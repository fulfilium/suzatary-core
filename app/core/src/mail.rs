//! The mail corpus's values: bodies are always text, a cut body says so, and triage's
//! disposition is a badge, never a filter.

pub mod account;
pub mod account_email;
pub mod address;
pub mod cache;
pub mod dedup_key;
pub mod folders;
pub mod gone;
pub mod grants;
pub mod idle;
pub mod imap;
pub mod junk;
pub mod known;
pub mod local_state;
pub mod mailroom;
pub mod oauth;
pub mod older;
pub mod outbox;
pub mod puller;
pub mod removal;
pub mod send;
pub mod state;
pub mod sync_now;
pub mod transport;
pub mod triage;
pub mod verbs;
pub mod watch;

use serde::Serialize;
use ts_rs::TS;

use crate::capability::Datetime;

/// A link as it appeared in the mail; the label is what tells lookalike links apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MailLink {
    /// Where it goes, filtered to http/https/mailto before it reaches a page.
    pub url: String,
    /// The text it stood behind, "" for a bare URL.
    pub label: String,
}

/// One header of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MailHeader {
    /// The header's name.
    pub name: String,
    /// Its value.
    pub value: String,
}

/// One row of the app's mail list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
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
    /// The maildir folder it lives in.
    pub container: String,
    /// The account's canonical address, "" for a maildir not of this deployment.
    pub account: String,
    /// What triage decided about this hash.
    pub disposition: Disposed,
}

/// What triage decided about a message, as a row shows it: nothing yet, or its decision, the
/// todo only beside the one decision that names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Disposed {
    /// Triage has decided nothing about it.
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
}

impl From<Option<&state::Decision>> for Disposed {
    fn from(decision: Option<&state::Decision>) -> Self {
        match decision {
            None => Self::Undecided,
            Some(state::Decision::Todo { todo_id }) => Self::Todo {
                todo: todo_id.as_str().to_owned(),
            },
            Some(state::Decision::Noise) => Self::Noise,
            Some(state::Decision::Skip) => Self::Skip,
        }
    }
}

/// One message as a page; which todos it produced is the chain's fact, joined by the route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct MailMessage {
    /// Its list row.
    pub row: MailRow,
    /// notmuch's conversation id, "" when the corpus names none.
    pub thread: String,
    /// The whitelisted headers it carries, in reading order.
    pub headers: Vec<MailHeader>,
    /// The body as text.
    pub body: String,
    /// Whether the body was cut at the cap.
    pub truncated: bool,
    /// The links it holds.
    pub links: Vec<MailLink>,
}

/// The cap on a body's characters; past it the wire says `truncated`.
pub const MAX_BODY_CHARS: usize = 200_000;

/// One attachment's identity (name, MIME type, decoded size) with no content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentMeta {
    /// Its filename from the disposition or the content type, `""` when neither names one.
    pub name: String,
    /// `type/subtype`, lower-cased, `""` when the part carried none.
    pub mime: String,
    /// Its decoded size in bytes, not the wire encoding's length.
    pub size: usize,
}

/// One attachment whole: what the message calls it, its type and its decoded bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFile {
    /// Its filename, as [`AttachmentMeta::name`] lists it.
    pub name: String,
    /// `type/subtype`, lower-cased, `""` when the part carried none.
    pub mime: String,
    /// Its decoded bytes.
    pub content: Vec<u8>,
}

/// What recording a message file for a forward found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// The file is larger than the cap asked for, so it was not read.
    TooLarge {
        /// Its size in bytes.
        size: u64,
    },
    /// The file, read once and recorded.
    Recorded(StoredMessage),
}

/// A message file as a forward records it: its digest, the held values in its text and its
/// attachments, never its words. The tap sends these bytes or nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StoredMessage {
    /// Its notmuch id.
    pub id: String,
    /// The file's size in bytes.
    pub size: u64,
    /// The SHA-256 of the file.
    pub sha256: crate::attachment::AttachmentId,
    /// The held values in its text, in order.
    pub held: Vec<crate::held_value::Held>,
    /// Its attachments, in the file's order.
    pub parts: Vec<StoredPart>,
}

/// One attachment of a stored message, by its place in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct StoredPart {
    /// Its filename, or `attachment-<n>` for the `n`th when it names none.
    pub name: String,
    /// Its `type/subtype`.
    pub media: String,
    /// Its decoded size in bytes.
    pub size: u64,
    /// The SHA-256 of its decoded bytes.
    pub sha256: crate::attachment::AttachmentId,
}

/// `StoredMessage(id, size, sha256, held, parts)`.
pub const STORED_MESSAGE: prodrome_core::literal::codec::Constructor =
    ("StoredMessage", &["id", "size", "sha256", "held", "parts"]);
/// `StoredPart(name, media, size, sha256)`.
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

/// The cap on an attachment's extracted characters, read for one fact rather than whole.
pub const MAX_ATTACHMENT_CHARS: usize = 100_000;

/// The headers a message page prints, in reading order; a whitelist.
pub const SHOWN_HEADERS: &[&str] = &["From", "To", "Cc", "Subject", "Date", "Reply-To"];

/// The schemes a link may carry to a page.
pub const LINK_SCHEMES: &[&str] = &["http://", "https://", "mailto:"];

/// A source hash prefix shorter than this names todos by accident, so it is not evidence.
pub const MIN_SOURCE_PREFIX: usize = 6;

/// `text` cut to at most `chars` characters, on a char boundary, never panicking.
#[must_use]
pub fn shorten(text: &str, chars: usize) -> &str {
    match text.char_indices().nth(chars) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

/// `text` cut to `cap` characters, and whether it was cut.
#[must_use]
pub fn clip(text: &str, cap: usize) -> (String, bool) {
    let short = shorten(text, cap);
    let truncated = short.len() != text.len();
    (short.to_owned(), truncated)
}

/// At most `len` characters of `text` starting at character `start`: one page of a longer text.
#[must_use]
pub fn page(text: &str, start: usize, len: usize) -> &str {
    let from = shorten(text, start).len();
    shorten(&text[from..], len)
}

/// Only the links a page may carry.
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

/// The whitelisted headers this message actually carries, in reading order.
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
    use prodrome_core::fpl::{instant_of, iso};
    use serde::Serializer;

    use crate::capability::Datetime;

    #[expect(clippy::ref_option, reason = "serde's `with` signature")]
    pub fn serialize<S: Serializer>(
        at: &Option<Datetime>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&at.map_or_else(String::new, |at| iso(instant_of(at))))
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

    /// Reading order, and nothing outside the whitelist.
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

    /// A row with no date prints it as `""`; its disposition is a tagged value, and only a todo
    /// decision carries a todo.
    #[test]
    fn a_row_prints_its_disposition_as_a_tagged_value() {
        let row = MailRow {
            hash: "abc".to_owned(),
            id: "id@x".to_owned(),
            sender: "her@example.test".to_owned(),
            subject: "hello".to_owned(),
            date: None,
            container: "inbox".to_owned(),
            account: "me@example.test".to_owned(),
            disposition: Disposed::Undecided,
        };
        assert_eq!(
            serde_json::to_value(&row).expect("serialises"),
            serde_json::json!({"hash": "abc", "id": "id@x", "sender": "her@example.test",
                "subject": "hello", "date": "", "container": "inbox",
                "account": "me@example.test", "disposition": {"kind": "undecided"}})
        );
        let decided = |decision: &state::Decision| {
            serde_json::to_value(MailRow {
                date: Some(Datetime::new(2026, 9, 1, 8, 30, 0, 0).expect("a real instant")),
                disposition: Disposed::from(Some(decision)),
                ..row.clone()
            })
            .expect("serialises")
        };
        let json = decided(&state::Decision::Noise);
        assert_eq!(json["date"], "2026-09-01T08:30:00");
        assert_eq!(json["disposition"], serde_json::json!({"kind": "noise"}));
        let todo = state::Decision::parse("todo", Some("renewcard")).expect("a decision");
        assert_eq!(
            decided(&todo)["disposition"],
            serde_json::json!({"kind": "todo", "todo": "renewcard"})
        );
    }

    /// A prefix shorter than six names no todo.
    #[test]
    fn the_reverse_index_is_one_directional_and_has_a_floor() {
        assert!(sourced_from("abc123", "abc123def456"));
        assert!(!sourced_from("abc12", "abc123def456"));
        assert!(!sourced_from("", "abc123def456"));
        assert!(!sourced_from("abc123def456", "abc123"));
    }

    use proptest::prelude::*;

    proptest! {
        /// `shorten` never panics, never splits a `char` over arbitrary text, and answers at
        /// most `chars` characters.
        #[test]
        fn shorten_never_panics_and_never_splits_a_char(text: String, chars in 0usize..200) {
            let short = shorten(&text, chars);
            prop_assert!(text.is_char_boundary(short.len()));
            prop_assert!(short.chars().count() <= chars);
        }

        /// `clip`'s flag is exactly whether `shorten` cut anything.
        #[test]
        fn clip_flags_truncation_exactly_when_shorten_cuts(text: String, cap in 0usize..200) {
            let (clipped, truncated) = clip(&text, cap);
            prop_assert_eq!(truncated, clipped != text);
            prop_assert_eq!(clipped.as_str(), shorten(&text, cap));
        }

        /// `page` never panics, including a `start` past the end.
        #[test]
        fn page_never_panics_and_never_splits_a_char(
            text: String,
            start in 0usize..200,
            len in 0usize..200,
        ) {
            let piece = page(&text, start, len);
            prop_assert!(piece.chars().count() <= len);
        }

        /// Paging in fixed steps from 0 recovers the whole text.
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
