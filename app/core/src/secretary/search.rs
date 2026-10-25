//! Finding a turn across every conversation, for the search route and `recall`.
//!
//! A case-folded substring over what the owner said and what the secretary answered, never a
//! verb's answer, which quotes mail: neither a list nor `recall` should surface a line an email
//! wrote.

use std::borrow::Borrow;

use crate::capability::Turn;
use crate::secretary::store::{ConversationId, Kind, Stored};

/// What to look for: trimmed, never empty, at most [`Needle::MAX`] characters, and kept
/// case-folded because that is the only way it is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle(Vec<char>);

/// Text that cannot be searched for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotANeedle {
    /// Nothing but whitespace.
    #[error("say what to search for")]
    Empty,
    /// Longer than [`Needle::MAX`] characters.
    #[error("a search is at most {} characters", Needle::MAX)]
    Long,
}

impl Needle {
    /// The longest search, in characters.
    pub const MAX: usize = 200;

    /// Parse one.
    ///
    /// # Errors
    /// [`NotANeedle`] naming which rule it broke.
    pub fn new(text: &str) -> Result<Self, NotANeedle> {
        let text = text.trim();
        if text.is_empty() {
            Err(NotANeedle::Empty)
        } else if text.chars().count() > Self::MAX {
            Err(NotANeedle::Long)
        } else {
            Ok(Self(text.chars().map(fold).collect()))
        }
    }

    /// Whether `text` holds it.
    #[must_use]
    pub fn found_in(&self, text: &str) -> bool {
        excerpt(text, self, 0).is_some()
    }

    /// Its first match in `text` with `context` characters either side, on one line; `None`
    /// when `text` does not hold it.
    #[must_use]
    pub fn excerpt(&self, text: &str, context: usize) -> Option<String> {
        excerpt(text, self, context)
    }
}

/// Which side of the conversation a hit is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "SearchSide")]
pub enum Side {
    /// The owner's own words.
    Person,
    /// The secretary's answer.
    Answer,
}

/// One turn that matched.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(rename = "SearchHit")]
pub struct Hit {
    /// The conversation.
    #[ts(type = "string")]
    pub conversation: ConversationId,
    /// Its heading ([`Stored::heading`]).
    pub title: String,
    /// What it is for.
    pub kind: Kind,
    /// The turn's absolute index, which a window opens at.
    pub turn: usize,
    /// When it was said.
    pub at: String,
    /// Who said it.
    pub side: Side,
    /// The match with `context` characters either side, on one line.
    pub excerpt: String,
}

/// How much a list's hit keeps either side of the match, in characters:
/// enough to recognise the line.
pub const LIST_CONTEXT: usize = 48;

/// How much `recall`'s hit keeps: enough to read what was decided, never a whole transcript
/// through a side door.
pub const RECALL_CONTEXT: usize = 280;

/// How many hits `recall` answers when asked for `asked`: ten when asked for no number, and
/// between one and fifty.
#[must_use]
pub fn recall_limit(asked: Option<usize>) -> usize {
    asked.unwrap_or(10).clamp(1, 50)
}

/// `hits` for `query` as `recall` answers them.
#[must_use]
pub fn recalled(query: &str, hits: &[Hit]) -> serde_json::Value {
    serde_json::json!({
        "query": query,
        "hits": hits.iter().map(|hit| serde_json::json!({
            "conversation": hit.conversation.as_str(),
            "title": hit.title,
            "turn": hit.turn,
            "who": match hit.side {
                Side::Person => "person",
                Side::Answer => "model",
            },
            "at": hit.at,
            "text": hit.excerpt,
        })).collect::<Vec<_>>(),
    })
}

/// Every turn that holds `needle`, newest first, at most `limit`, each cut to `context`
/// characters either side of the match.
///
/// Ties on `at` break by conversation and index, so the order is stable. An answer is a finished
/// one (a model turn with no calls); text on the way to a verb is part of the trace.
#[must_use]
pub fn search<S: Borrow<Stored>>(
    conversations: &[S],
    needle: &Needle,
    context: usize,
    limit: usize,
) -> Vec<Hit> {
    let mut hits: Vec<Hit> = conversations
        .iter()
        .map(Borrow::borrow)
        .flat_map(|stored: &Stored| {
            stored
                .turns
                .iter()
                .enumerate()
                // Set-aside turns are never delivered, and `recall` is a delivery.
                .filter(|(index, _)| stored.is_live(*index))
                .filter_map(move |(index, said)| {
                    let (side, text) = match &said.turn {
                        Turn::Person { text, .. } => (Side::Person, text),
                        Turn::Model { text, calls } if calls.is_empty() => (Side::Answer, text),
                        Turn::Model { .. }
                        | Turn::Tool { .. }
                        | Turn::Act { .. }
                        | Turn::Erased { .. } => return None,
                    };
                    excerpt(text, needle, context).map(|excerpt| Hit {
                        conversation: stored.id.clone(),
                        title: stored.heading().into_owned(),
                        kind: stored.kind,
                        turn: index,
                        at: crate::secretary::store::stamp(said.at),
                        side,
                        excerpt,
                    })
                })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| a.conversation.cmp(&b.conversation))
            .then_with(|| b.turn.cmp(&a.turn))
    });
    hits.truncate(limit);
    hits
}

/// One character, case-folded to one character, so a match's position in the folded text is its
/// position in the original (`str::to_lowercase` can lengthen a string).
pub(crate) fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The first match in `text`, with `context` characters either side and
/// every run of whitespace one space; `None` when there is none.
fn excerpt(text: &str, needle: &Needle, context: usize) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let width = needle.0.len();
    let at = (0..=chars.len().checked_sub(width)?).find(|&start| {
        chars[start..start + width]
            .iter()
            .zip(&needle.0)
            .all(|(&c, &n)| fold(c) == n)
    })?;
    Some(around(&chars, at, width, context))
}

/// The `width` characters at `at` with `context` characters either side, every run of whitespace
/// one space, and an ellipsis where the text was cut.
pub(crate) fn around(chars: &[char], at: usize, width: usize, context: usize) -> String {
    let from = at.saturating_sub(context);
    let to = (at + width + context).min(chars.len());
    let body = chars[from..to]
        .iter()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "{}{body}{}",
        if from > 0 { "…" } else { "" },
        if to < chars.len() { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::store::Entry;

    fn conversation(slug: &str, title: &str, turns: &[(&str, Turn)]) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            title: crate::secretary::words::Words::kept(title.to_owned()),
            model: "m".to_owned(),
            kind: if slug == "default" {
                crate::secretary::store::Kind::Default
            } else {
                crate::secretary::store::Kind::Topic
            },
        }];
        entries.extend(turns.iter().map(|(at, turn)| Entry::Said {
            at: crate::secretary::store::instant(at),
            turn: turn.clone(),
            by: None,
        }));
        Stored::of(&ConversationId::new(slug).expect("a slug"), &entries).expect("a conversation")
    }

    fn person(text: &str) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }
    }

    fn answer(text: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        }
    }

    fn calling(text: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: vec![crate::capability::ToolCall {
                verb: "show".into(),
                args: std::collections::BTreeMap::new(),
            }],
        }
    }

    fn tool(text: &str) -> Turn {
        Turn::Tool {
            verb: "show".into(),
            answer: text.to_owned(),
            outcome: crate::secretary::outcome::Outcome::Answered,
            files: Vec::new(),
        }
    }

    fn needle(text: &str) -> Needle {
        Needle::new(text).expect("a needle")
    }

    /// Person turns and finished answers match, case-folded; a verb's answer never does, because it
    /// quotes mail, nor the text on the way to a verb.
    #[test]
    fn it_reads_what_was_said_and_answered_and_never_a_verb() {
        let all = [
            conversation(
                "default",
                "Secretary",
                &[
                    ("2026-09-25T09:01:00", person("When is the PERMIT due?")),
                    ("2026-09-25T09:01:30", calling("Checking the permit mail")),
                    ("2026-09-25T09:02:00", tool("Subject: permit notice")),
                    (
                        "2026-09-25T09:03:00",
                        answer("The permit is due on the 3rd."),
                    ),
                ],
            ),
            conversation(
                "2026-09-24-1",
                "",
                &[("2026-09-24T10:00:00", person("Nothing here"))],
            ),
        ];
        let hits = search(&all, &needle("permit"), LIST_CONTEXT, 10);
        assert_eq!(
            hits.iter()
                .map(|hit| (hit.turn, hit.side))
                .collect::<Vec<_>>(),
            vec![(3, Side::Answer), (0, Side::Person)],
            "newest first, the verb's answer and the calling turn skipped"
        );
        assert_eq!(hits[0].title, "Secretary");
        assert_eq!(hits[0].kind, Kind::Default);
        assert!(search(&all, &needle("notice"), LIST_CONTEXT, 10).is_empty());
        assert_eq!(
            search(&all, &needle("nothing"), LIST_CONTEXT, 10)[0].title,
            "New topic"
        );
        assert_eq!(
            search(&all, &needle("e"), LIST_CONTEXT, 1).len(),
            1,
            "the limit holds"
        );
    }
}
