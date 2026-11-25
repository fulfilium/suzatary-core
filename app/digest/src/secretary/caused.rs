//! What a forgetting takes from the conversation a fact was said in, read from the lines and
//! never searched for: the copies the remembering provably made, which it erases, and the turns
//! around them, which it can only offer.

use std::collections::BTreeSet;

use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::store::Entry;
use suzatary_conversations::secretary::tool_name;
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_conversations::secretary::verb_name::VerbName;
use suzatary_lines::secretary::line_event;
use suzatary_lines::secretary::line_shape::LineShape;
use suzatary_lines::secretary::transcript::Transcript;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_memory::secretary::memory::{FactId, Origin};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Caused {
    /// The remember call's arguments and its answer.
    pub erased: BTreeSet<KeyName>,
    /// The turn that said the fact and the model's prose of the round.
    pub offered: BTreeSet<KeyName>,
}

impl Caused {
    /// What forgetting `fact`, said at `origin`, takes from `transcript`: keys an erasure already
    /// names are neither erased nor offered again.
    #[must_use]
    pub fn of(transcript: &Transcript, fact: FactId, origin: &Origin) -> Self {
        let lines = transcript.lines();
        let Some(answer) = lines
            .iter()
            .position(|written| receipt(&written.entry, fact, origin))
        else {
            return Self::default();
        };
        let mut caused = Self::default();
        caused.erased.extend(lines[answer].line.keys().cloned());
        let model = lines[..answer]
            .iter()
            .rposition(|written| model_calls(&written.line).is_some());
        if let Some(model) = model {
            let call = lines[model + 1..answer]
                .iter()
                .filter(|written| matches!(said(&written.line), Some(TurnShape::Tool { .. })))
                .count();
            let calls = model_calls(&lines[model].line).unwrap_or_default();
            if matches!(
                calls.get(call).and_then(tool_name::ToolName::verb),
                Some(VerbName::Remember | VerbName::Consolidate)
            ) {
                caused
                    .erased
                    .extend(lines[model].line.keys().nth(1 + call).cloned());
            }
        }
        let round = match origin {
            Origin::Turn { turn, .. } => transcript.line_of(*turn),
            Origin::Tapped { .. } => model,
        };
        if let Some(from) = round {
            caused
                .offered
                .extend(lines[from].line.keys().take(1).cloned());
            let prose = lines[from..]
                .iter()
                .enumerate()
                .take_while(|(at, written)| {
                    *at == 0 || !matches!(said(&written.line), Some(TurnShape::Person { .. }))
                })
                .filter(|(_, written)| model_calls(&written.line).is_some())
                .filter_map(|(_, written)| written.line.keys().next().cloned());
            caused.offered.extend(prose);
        }
        let erased = suzatary_lines::secretary::transcript::erasures(
            lines.iter().map(|written| (&written.name, &written.line)),
        );
        caused.erased.retain(|key| !erased.contains_key(key));
        caused
            .offered
            .retain(|key| !erased.contains_key(key) && !caused.erased.contains(key));
        caused
    }
}

/// Whether `entry` is the answer of the `remember` call that wrote `fact` at `origin`: for a
/// fact written in a turn, the turn's receipt names it; for a tapped card, the answer that filed
/// the card names the card.
fn receipt(entry: &Entry, fact: FactId, origin: &Origin) -> bool {
    let Entry::Said {
        turn:
            Turn::Tool {
                verb,
                answer,
                outcome,
                ..
            },
        ..
    } = entry
    else {
        return false;
    };
    match origin {
        Origin::Turn { .. } => *outcome == Outcome::Remembered { fact },
        Origin::Tapped { proposal, .. } => {
            verb.verb() == Some(VerbName::Remember)
                && *outcome == Outcome::Answered
                && serde_json::from_str::<serde_json::Value>(answer)
                    .ok()
                    .and_then(|answered| answered.get("proposed")?.as_u64())
                    == u64::try_from(*proposal).ok()
        }
    }
}

fn said(line: &line_event::LineEvent) -> Option<&TurnShape> {
    if let LineShape::Said { turn, .. } = line.shape() {
        Some(turn)
    } else {
        None
    }
}

fn model_calls(line: &line_event::LineEvent) -> Option<&[tool_name::ToolName]> {
    if let Some(TurnShape::Model { calls, .. }) = said(line) {
        Some(calls)
    } else {
        None
    }
}
