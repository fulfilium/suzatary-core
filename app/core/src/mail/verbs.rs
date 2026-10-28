//! What the mechanical mail verbs answer, and their JSON.
//!
//! These records are part of the versioned MCP surface, which a stateless agent
//! re-binds to every run: field order is declaration order, absence has one
//! spelling, and `timestamp` is always [`shown_timestamp`]'s
//! `YYYY-MM-DD HH:MM:SS[±HH:MM]` (a space, not a `T`), one grammar for an agent
//! to parse.

use crate::source_key::SourceKey;
use serde::{Deserialize, Serialize};

/// One queue or search row, enough for an agent to decide whether to `show`.
/// `hash` is what `mark` takes and `id` what `show` takes; they are not
/// interchangeable.
///
/// `Deserialize` lets `suzatary_core::secretary::sweep::candidates::candidates` read `Corpus::search`'s
/// JSON back into this type rather than a second row shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// The canonical content hash — the dedup identity.
    pub hash: String,
    /// The notmuch locator.
    pub id: String,
    /// The From address, lower-cased.
    pub sender: String,
    /// The Subject, RFC 2047 decoded.
    pub title: String,
    /// When it arrived, as [`shown_timestamp`] prints it, or `null` when the
    /// `Date:` header would not parse.
    pub timestamp: Option<String>,
    /// The maildir folder it lives in.
    pub container: String,
    /// How many places this content hash was seen: one message on three
    /// accounts is one row with `sources: 3`.
    pub sources: usize,
}

/// A link as it appeared in the mail; the label is what tells "Modify
/// reservation" apart from forty tracking-wrapped lookalikes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    /// Where it goes.
    pub url: String,
    /// The text it stood behind, `""` for a bare URL.
    pub label: String,
}

/// One message in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// The notmuch locator.
    pub id: String,
    /// Its notmuch tags.
    pub tags: Vec<String>,
    /// Every header, not the page's whitelist: an agent judging a message
    /// wants `List-Id` and `Reply-To`.
    pub headers: std::collections::BTreeMap<String, String>,
    /// The body as text, HTML converted, never truncated (the cap is the page's).
    pub body: String,
    /// Every link in every part.
    pub links: Vec<Link>,
}

/// What `show` answers: the message, or why there is none to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    /// The message.
    Shown(Message),
    /// No message by that locator or hash is stored, which is an answer, not a failure.
    NotFound {
        /// What was asked for.
        id: String,
    },
    /// A hash prefix that names more than one decided message.
    Ambiguous(crate::mail::local_state::Ambiguous),
}

/// The three answers as the agent reads them: the message itself, `{gone, id}`, or
/// `{error, id, matches}`, naming the colliding hashes so it can correct itself.
#[derive(Serialize)]
#[serde(untagged)]
enum Printed<'a> {
    Shown(&'a Message),
    NotFound {
        gone: &'static str,
        id: &'a str,
    },
    Ambiguous {
        error: String,
        id: &'a str,
        matches: &'a [String],
    },
}

impl Serialize for Show {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Show::Shown(message) => Printed::Shown(message),
            Show::NotFound { id } => Printed::NotFound {
                gone: super::gone::SAID,
                id,
            },
            Show::Ambiguous(ambiguous) => Printed::Ambiguous {
                error: ambiguous.to_string(),
                id: &ambiguous.prefix,
                matches: &ambiguous.matches,
            },
        }
        .serialize(serializer)
    }
}

/// What `mark` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Marked {
    /// The hash, exactly as it was passed in (a prefix stays a prefix).
    pub marked: String,
    /// The disposition recorded.
    pub disposition: super::state::Disposition,
}

/// What `noise add` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoiseAdded {
    /// The sender as stored (lower-cased), not as typed.
    pub noise_added: String,
}

/// One account's sync outcome. A failure is an outcome, never a stop: one
/// expired token must not cost the other accounts their sync.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSync {
    /// The account's canonical address.
    pub account: crate::mail::account_email::AccountEmail,
    /// Whether it synced, and the class of its failure where it did not.
    pub outcome: crate::health::Outcome,
    /// The last line of what it said.
    pub tail: String,
}

/// What `sync` answers. The wrapper persists it at the last-sync path, where
/// `suzatary gate` (the triage rule, [`super::triage`]) and `sync_mail` read it back, so a renamed field is renamed
/// for both; each account's health over time is [`crate::health`]'s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncReport {
    /// Every account, in the roster's order.
    pub synced: Vec<AccountSync>,
    /// The last line `notmuch new` printed.
    pub notmuch: String,
    /// Why the sync ran without its hold, `None` when it took it: a removed account's mail
    /// deleted meanwhile may have been pulled back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unheld: Option<String>,
}

impl SyncReport {
    /// What it says of each account's health, every account at `at`.
    #[must_use]
    pub fn healths(&self, at: crate::health::Instant) -> crate::health::Healths<SourceKey> {
        self.synced
            .iter()
            .map(|account| {
                let key = SourceKey::Mail(account.account.clone());
                crate::health::Healths::one(key, at, account.outcome, &account.tail)
            })
            .fold(crate::health::Healths::default(), |sum, one| sum.join(&one))
    }
}

/// What `backfill` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backfilled {
    /// The account.
    pub account: String,
    /// The server folder.
    pub folder: String,
    /// How many messages were newly written.
    pub fetched: usize,
}

/// A parsed `Date:` header as `YYYY-MM-DD HH:MM:SS`, with `±HH:MM` when it named
/// a zone. `offset_minutes` is `None` for no resolvable zone (including
/// `-0000`, "unknown"), and the instant then prints with no offset.
#[must_use]
pub fn shown_timestamp(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    offset_minutes: Option<i32>,
) -> String {
    let naive = format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}");
    match offset_minutes {
        None => naive,
        Some(offset) => {
            let sign = if offset < 0 { '-' } else { '+' };
            let magnitude = offset.abs();
            format!("{naive}{sign}{:02}:{:02}", magnitude / 60, magnitude % 60)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A space, not a `T`, and the offset is `±HH:MM`.
    #[test]
    fn a_timestamp_is_cpythons_str_and_not_an_iso_spelling() {
        assert_eq!(
            shown_timestamp(2026, 9, 1, 14, 22, 33, Some(-4 * 60)),
            "2026-09-01 14:22:33-04:00"
        );
        assert_eq!(
            shown_timestamp(2026, 9, 1, 14, 22, 33, Some(0)),
            "2026-09-01 14:22:33+00:00"
        );
        // A half-hour zone spells its minutes.
        assert_eq!(
            shown_timestamp(2026, 9, 1, 14, 22, 33, Some(5 * 60 + 30)),
            "2026-09-01 14:22:33+05:30"
        );
        // No resolvable zone prints bare.
        assert_eq!(
            shown_timestamp(2026, 9, 1, 14, 22, 33, None),
            "2026-09-01 14:22:33"
        );
        // Padded, so the strings sort.
        assert_eq!(
            shown_timestamp(2026, 1, 2, 3, 4, 5, None),
            "2026-01-02 03:04:05"
        );
    }

    #[test]
    fn a_sync_report_carries_the_two_fields_three_consumers_read() {
        let report = SyncReport {
            unheld: None,
            synced: vec![AccountSync {
                account: crate::mail::account_email::AccountEmail::new("her@example.test")
                    .expect("an address"),
                outcome: crate::health::Outcome::Failed(crate::connect::Failure::Unclassified),
                tail: "connection refused".to_owned(),
            }],
            notmuch: "No new mail.".to_owned(),
        };
        assert_eq!(
            serde_json::to_string(&report).expect("serialises"),
            r#"{"synced":[{"account":"her@example.test","outcome":{"failed":{"kind":"unclassified"}},"tail":"connection refused"}],"notmuch":"No new mail."}"#
        );
    }
}
