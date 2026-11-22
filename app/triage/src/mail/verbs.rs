//! What the mechanical mail verbs answer: part of the versioned MCP surface, so field order is
//! declaration order, absence has one spelling, and `timestamp` is always [`shown_timestamp`]'s.

use serde::{Deserialize, Serialize};

/// One queue or search row: `hash` is what `mark` takes and `id` what `show` takes; they are not
/// interchangeable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    /// The canonical content hash, the dedup identity.
    pub hash: String,
    /// The notmuch locator.
    pub id: String,
    /// Lower-cased.
    pub sender: String,
    /// The Subject, RFC 2047 decoded.
    pub title: String,
    /// `null` when the `Date:` header would not parse.
    pub timestamp: Option<String>,
    pub container: String,
    /// One message on three accounts is one row with `sources: 3`.
    pub sources: usize,
    /// The kinds of held value it names, never a value or its digits.
    pub held: suzatary_held::held_value::Kinds,
}

/// The label is what tells "Modify reservation" apart from forty tracking-wrapped lookalikes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub url: String,
    /// The text it stood behind, `""` for a bare URL.
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// The notmuch locator.
    pub id: String,
    pub tags: Vec<String>,
    /// Every header, not the page's whitelist: an agent judging a message wants `List-Id`.
    pub headers: std::collections::BTreeMap<String, String>,
    /// HTML converted, never truncated (the cap is the page's).
    pub body: String,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    Shown(Message),
    /// No message by that locator or hash is stored, which is an answer, not a failure.
    NotFound {
        /// What was asked for.
        id: String,
    },
    Ambiguous(suzatary_mail::mail::hash_prefix::Ambiguous),
}

/// `{error, id, matches}` names the colliding hashes so the agent can correct itself.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Marked {
    /// Exactly as passed in: a prefix stays a prefix.
    pub marked: String,
    pub disposition: super::state::Disposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NoiseAdded {
    /// As stored (lower-cased), not as typed.
    pub noise_added: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Backfilled {
    pub account: String,
    /// The server folder.
    pub folder: String,
    /// Messages newly written.
    pub fetched: usize,
}

/// A parsed `Date:` header as `YYYY-MM-DD HH:MM:SS[±HH:MM]` (a space, not a `T`);
/// `offset_minutes` is `None` for no resolvable zone, `-0000` ("unknown") included.
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
        assert_eq!(
            shown_timestamp(2026, 9, 1, 14, 22, 33, Some(5 * 60 + 30)),
            "2026-09-01 14:22:33+05:30"
        );
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
}
