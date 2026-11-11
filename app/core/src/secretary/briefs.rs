//! The morning briefs, derived from the transcripts: a brief is the question [`BRIEF`] asked
//! on a schedule and the answer to it, so nothing is stored twice.
//!
//! The answer is what the model said after its last read, up to the owner's next turn or tap;
//! a turn that reads, or a one-paragraph turn beside a proposal with more words after it, is a
//! status line.

use chrono::NaiveDate;

use super::store::{Said, Stored};
use super::verb_name::Effect;
use crate::secretary::model::Turn;
use suzatary_base::conversation_id::ConversationId;

/// The question the morning brief asks: a person turn, not a system instruction, since the
/// secretary answers questions somebody asked.
pub const BRIEF: &str = "What changed since the last briefing? Call briefing_delta first. \
                     Tell me what moved, what became urgent, and any good news — not the \
                     priced list, which I can already see. Then propose the two or three \
                     moves that would make the most progress, with the drafts written out.";

/// One morning's brief, where it was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    /// The conversation it is in.
    pub conversation: ConversationId,
    /// The day it was asked for.
    pub date: NaiveDate,
    /// The index of the answering turn.
    pub turn: usize,
    /// The answer, as the secretary wrote it.
    pub text: String,
}

/// Every brief in one conversation, oldest first. A brief whose round never
/// produced an answer (the model was unreachable) is not listed.
#[must_use]
pub fn briefs(stored: &Stored) -> Vec<Brief> {
    let mut found = Vec::new();
    // An answer a regenerate set aside is not the brief.
    let live = stored.live();
    for (at, (_, said)) in live.iter().enumerate() {
        let Turn::Person { text, .. } = &said.turn else {
            continue;
        };
        if text != BRIEF {
            continue;
        }
        let date = prodrome_core::fpl::instant_of(said.at).date();
        let round: Vec<&(usize, &Said)> = live[at + 1..]
            .iter()
            .take_while(|(_, later)| !matches!(later.turn, Turn::Person { .. } | Turn::Act { .. }))
            .collect();
        let after_reads = round
            .iter()
            .rposition(|(_, later)| reads(&later.turn))
            .map_or(0, |last| last + 1);
        let mut words: Vec<(usize, &str, bool)> = round[after_reads..]
            .iter()
            .filter_map(|(index, later)| match &later.turn {
                Turn::Model { text, calls } if !text.trim().is_empty() => {
                    let status =
                        !calls.is_empty() && suzatary_base::answer::blocks(text).len() == 1;
                    Some((*index, text.trim(), status))
                }
                _ => None,
            })
            .collect();
        let leading = words.iter().take_while(|(_, _, status)| *status).count();
        words.drain(..leading.min(words.len().saturating_sub(1)));
        if let Some(&(turn, _, _)) = words.first() {
            found.push(Brief {
                conversation: stored.id.clone(),
                date,
                turn,
                text: words
                    .iter()
                    .map(|(_, text, _)| *text)
                    .collect::<Vec<_>>()
                    .join("\n\n"),
            });
        }
    }
    found
}

/// Whether a turn is a status line: the model asked for something read. A
/// verb this build no longer names reads, since what it did is unknown.
fn reads(turn: &Turn) -> bool {
    match super::turn_shape::shape(turn) {
        super::turn_shape::TurnShape::Model { calls } => calls.iter().any(|verb| {
            verb.verb()
                .is_none_or(|verb| verb.powers().effect == Effect::Reads)
        }),
        super::turn_shape::TurnShape::Person { .. }
        | super::turn_shape::TurnShape::Tool { .. }
        | super::turn_shape::TurnShape::Act { .. } => false,
    }
}

/// A brief's line in a list: the first line of its words as Typst parses
/// them, so a link reads as its label and never as `#link(…)[…]`.
#[must_use]
pub fn headline(text: &str) -> String {
    suzatary_base::answer::blocks(text)
        .iter()
        .flat_map(|block| block.plain.lines())
        .map(|line| line.trim_start_matches(['•', ' ']).trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// How many words a brief holds, as Typst parses them.
#[must_use]
pub fn words(text: &str) -> usize {
    suzatary_base::answer::blocks(text)
        .iter()
        .map(|block| block.plain.split_whitespace().count())
        .sum()
}
