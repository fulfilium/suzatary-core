//! The digest's view as a reader is shown it: each part of the cut ([`Digest::view`]) one line,
//! a line as itself, a summary as derived words, and a tainted part by its size and kinds alone,
//! never its words. The compactor's log and the secretary's context are the same rendering,
//! printed two ways.
//!
//! Every part prints on exactly one line: a line's spaces are joined, and a summary is one line
//! by its type ([`Line`]). So a part's words cannot reach past its own line, and a marker at the
//! head of a line fences what follows it on that line.

use std::collections::BTreeMap;

use prodrome_core::digest::{Measure as _, Node, Part};
use prodrome_core::event::Hash;

use crate::capability::Turn;
use crate::secretary::digest::Digest;
use crate::secretary::digest_label::{Kind, Label};
use crate::secretary::digest_line::Line;
use crate::secretary::outcome::Outcome;
use crate::secretary::store::Entry;
use crate::secretary::tool_name::ToolName;
use crate::secretary::turn_shape::TurnShape;

/// One part as shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The part: its node, and the lines it spans.
    pub part: Part,
    /// What stands for it.
    pub body: Body,
}

/// What stands for a part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// One line, as it was said.
    Said(String),
    /// A summary: words a compactor derived from the lines beneath it, nobody's own.
    Derived(Line),
    /// A part its reader is not shown the words of, by its kinds.
    Hidden(Label),
    /// A node no summary stands for yet, by its kinds.
    Unsummarised(Label),
}

/// The most bytes one rendered line holds; a longer one is cut and says so.
pub const LINE_BYTES: usize = 4_096;

/// The most bytes a view is shown in: the view's budget (the owner's answer 8, 128 KB).
pub const VIEW_BYTES: usize = 131_072;

/// Which end of a view over its budget is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// The oldest parts: a prefix a model's cache keeps.
    Earliest,
    /// The newest parts: what was said last.
    Latest,
}

/// The view of `digest` under `budget` bytes, shown: a part `shows` reads as its words, any other
/// hidden by its kinds. A part whose words do not read is left out.
#[must_use]
pub fn view(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    budget: usize,
    shows: fn(&Label) -> bool,
) -> Vec<Shown> {
    let budget = u64::try_from(budget).unwrap_or(u64::MAX);
    let (Some(tree), Some(view)) = (digest.tree(), digest.view(budget)) else {
        return Vec::new();
    };
    view.parts
        .into_iter()
        .filter_map(|part| {
            let node = tree.node(&part.node)?;
            let body = shown_as(*node.measure(), shows, || said(digest, entries, node))?;
            Some(Shown { part, body })
        })
        .collect()
}

/// What lies beneath the lines `handle` names (`first+lines`, as [`Part::handle`] prints it), a
/// part `reads` admits as it is, a line whole, and any other hidden by its kinds: the children of
/// the node spanning them, the deepest of a chain of single children, or the line itself;
/// otherwise the nodes that tile them, so a run of parts a view merged opens too. With the join
/// of the shown parts' labels, which is what the answer can carry. `None` for a handle naming no
/// lines.
#[must_use]
pub fn opened(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    handle: &str,
    reads: fn(&Label) -> bool,
) -> Option<(Label, Vec<Shown>)> {
    let nodes = digest.cover(span_of(handle)?)?;
    let beneath: Vec<&Node<Label>> = match nodes.as_slice() {
        [node] => {
            let mut node = *node;
            while let [only] = node.children() {
                node = only;
            }
            if node.is_leaf() {
                vec![node]
            } else {
                node.children().iter().collect()
            }
        }
        many => many.to_vec(),
    };
    let label = beneath
        .iter()
        .map(|child| child.measure())
        .filter(|label| reads(label))
        .fold(Label::empty(), |label, child| label.join(child));
    let shown = beneath
        .into_iter()
        .map(|child| {
            let body = shown_as(*child.measure(), reads, || {
                Some(if child.is_leaf() {
                    Body::Said(whole(entries.get(digest.line(child.name()))?))
                } else {
                    said(digest, entries, child).unwrap_or(Body::Unsummarised(*child.measure()))
                })
            })?;
            Some(Shown {
                part: part(child),
                body,
            })
        })
        .collect::<Option<_>>()?;
    Some((label, shown))
}

/// What stands for a part labelled `label` to a reader that `reads`: its words, as `words`
/// gives them, when the reader reads the label, and otherwise its kinds alone. `None` when its
/// words do not read. The one rule every rendering of the digest shows a part by.
pub fn shown_as(
    label: Label,
    reads: fn(&Label) -> bool,
    words: impl FnOnce() -> Option<Body>,
) -> Option<Body> {
    if reads(&label) {
        words()
    } else {
        Some(Body::Hidden(label))
    }
}

/// The lines a handle names: `first+lines`.
fn span_of(handle: &str) -> Option<std::ops::Range<usize>> {
    let (first, lines) = handle.trim().split_once('+')?;
    let first: usize = first.parse().ok()?;
    Some(first..first.checked_add(lines.parse().ok()?)?)
}

/// The part a node is.
fn part(node: &Node<Label>) -> Part {
    Part {
        node: node.name().clone(),
        span: node.span(),
    }
}

/// A node's words: a line as itself, an inner node as its summary; `None` for one with none.
fn said(digest: &Digest, entries: &BTreeMap<&Hash, &Entry>, node: &Node<Label>) -> Option<Body> {
    if node.is_leaf() {
        entries
            .get(digest.line(node.name()))
            .map(|entry| Body::Said(rendered(entry)))
    } else {
        digest.summary(node.name()).cloned().map(Body::Derived)
    }
}

/// `shown` within `budget` bytes, a line each, the end `keep` names kept; when parts are left
/// out, a line where they were says how many.
#[must_use]
pub fn fit(shown: Vec<String>, budget: usize, keep: Keep) -> Vec<String> {
    let mut bytes = 0;
    let mut room = |line: &String| {
        bytes += line.len() + 1;
        bytes <= budget
    };
    let kept = match keep {
        Keep::Earliest => shown.iter().take_while(|line| room(line)).count(),
        Keep::Latest => shown.iter().rev().take_while(|line| room(line)).count(),
    };
    let left = shown.len() - kept;
    let note = |which| format!("({left} {which} parts of the log are not shown here.)");
    let mut lines: Vec<String> = Vec::with_capacity(kept + 1);
    match keep {
        Keep::Earliest => {
            lines.extend(shown.into_iter().take(kept));
            if left > 0 {
                lines.push(note("later"));
            }
        }
        Keep::Latest => {
            if left > 0 {
                lines.push(note("earlier"));
            }
            lines.extend(shown.into_iter().skip(left));
        }
    }
    lines
}

impl Body {
    /// Its one line, with no handle: a line or a summary as its words, a part with none by its
    /// size and kinds.
    #[must_use]
    pub fn text(&self, lines: usize) -> String {
        match self {
            Body::Said(text) => text.clone(),
            Body::Derived(line) => line.as_str().to_owned(),
            Body::Hidden(label) => format!(
                "({lines} lines that may hold others' words, not shown here: {})",
                kinds(*label)
            ),
            Body::Unsummarised(label) => {
                format!("({lines} lines not summarised yet: {})", kinds(*label))
            }
        }
    }
}

/// A label's kinds, in words.
#[must_use]
pub fn kinds(label: Label) -> String {
    label.kinds.iter().map(word).collect::<Vec<_>>().join(", ")
}

fn word(kind: Kind) -> &'static str {
    match kind {
        Kind::Opened => "an opening",
        Kind::Titled => "a title",
        Kind::Person => "the owner's words",
        Kind::Model => "the secretary's answers",
        Kind::Tool => "tool answers",
        Kind::Act => "the owner's acts",
        Kind::Proposed => "proposals",
        Kind::Reviewed => "reviews",
        Kind::ReviewFailed => "failed reviews",
        Kind::Stopped => "stops",
        Kind::Rewound => "retries",
        Kind::MemorySeen => "memory notes",
        Kind::Session => "session notes",
        Kind::Coding => "coding sessions",
        Kind::Erased => "erased words",
    }
}

/// One stored line as one line of text: who said what, with no id, hash or object name, its
/// spaces joined and cut at [`LINE_BYTES`].
#[must_use]
pub fn rendered(entry: &Entry) -> String {
    cut(&whole(entry)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

/// One stored line whole: who said what, with no id, hash or object name.
#[must_use]
pub fn whole(entry: &Entry) -> String {
    match entry {
        Entry::Opened { title, .. } if title.get().is_some_and(|title| title.trim().is_empty()) => {
            "A conversation opened.".to_owned()
        }
        Entry::Opened { title, .. } => format!("A conversation opened: {title}"),
        Entry::Titled { title, .. } => format!("The conversation was named: {title}"),
        Entry::Said { turn, .. } => said_turn(turn),
        Entry::Proposed { revises: None, .. } => {
            "The secretary proposed a card for the owner to tap.".to_owned()
        }
        Entry::Proposed {
            revises: Some(_), ..
        } => "The secretary revised a card it had proposed.".to_owned(),
        Entry::Reviewed { decision, why, .. } => format!(
            "The secretary reviewed a batch of the triage agent's ({}): {why}",
            decision.word()
        ),
        Entry::ReviewFailed { reason, .. } => format!("A review could not run: {reason}"),
        Entry::Stopped { .. } => "The owner stopped the answer.".to_owned(),
        Entry::Rewound { .. } => "The owner asked again.".to_owned(),
        Entry::MemorySeen { .. } => "The secretary was told the memory.".to_owned(),
        Entry::Session { .. } | Entry::Kept { .. } => "A model session opened.".to_owned(),
        Entry::Scoped { scope, .. } => format!("The model is asked over its {}.", scope.as_str()),
        Entry::Unheard { model, .. } => {
            format!("The model session ended: {model} said a turn the session never heard.")
        }
        Entry::Coding { .. } => "A coding session moved on.".to_owned(),
        Entry::Resealed { .. } => {
            "A part of this conversation was sealed again under the current redaction rules."
                .to_owned()
        }
        Entry::Erased { keys, erasure, .. } => format!(
            "The words of {} part{} of this conversation went: {}.",
            keys.len(),
            if keys.len() == 1 { "" } else { "s" },
            erasure.sentence()
        ),
    }
}

/// A turn, a call as its verb's name and never its arguments, so a remember card's words, which
/// a `remember` call carries, are never shown.
fn said_turn(turn: &Turn) -> String {
    match turn {
        Turn::Person {
            text, attachments, ..
        } if attachments.is_empty() => format!("Owner: {text}"),
        Turn::Person {
            text, attachments, ..
        } => format!("Owner, with {} attached: {text}", attachments.len()),
        Turn::Model { text, calls } if calls.is_empty() => format!("Secretary: {text}"),
        Turn::Model { text, calls } => {
            let verbs: Vec<&str> = calls.iter().map(|call| call.verb.as_str()).collect();
            format!("Secretary: {text} [asked {}]", verbs.join(", "))
        }
        Turn::Tool {
            verb,
            answer,
            outcome,
            ..
        } => match outcome {
            Outcome::Answered | Outcome::Remembered { .. } => {
                format!("{} answered: {answer}", verb.as_str())
            }
            Outcome::Refused => format!("{} refused: {answer}", verb.as_str()),
        },
        Turn::Act { text, .. } => format!("Owner did: {text}"),
        Turn::Erased { shape, words } => {
            let prose = crate::secretary::turn_shape::prose(words);
            match shape {
                TurnShape::Person { .. } => format!("Owner: {prose}"),
                TurnShape::Model { calls } => {
                    let verbs: Vec<&str> = calls.iter().map(ToolName::as_str).collect();
                    format!("Secretary: {prose} [asked {}]", verbs.join(", "))
                }
                TurnShape::Tool { verb, .. } => format!("{} answered: {prose}", verb.as_str()),
                TurnShape::Act { .. } => format!("Owner did: {prose}"),
            }
        }
    }
}

/// `text` within [`LINE_BYTES`], cut at a character's edge, the cut said.
fn cut(text: &str) -> String {
    if text.len() <= LINE_BYTES {
        return text.to_owned();
    }
    let edge = text.floor_char_boundary(LINE_BYTES);
    format!("{} … ({} bytes more)", &text[..edge], text.len() - edge)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap as Map;

    use super::*;
    use crate::capability::ToolCall;

    /// A call is its verb's name: a `remember` call's words are never shown.
    #[test]
    fn a_call_renders_as_its_verb_and_never_its_words() {
        let mut args = Map::new();
        args.insert(
            "fact".to_owned(),
            serde_json::json!("the owner's sister lives in London"),
        );
        let line = rendered(&Entry::Said {
            at: crate::capability::Datetime::new(2026, 10, 6, 9, 0, 0, 0).expect("an instant"),
            turn: Turn::Model {
                text: "Noting it.".to_owned(),
                calls: vec![ToolCall {
                    verb: "remember".into(),
                    args,
                }],
            },
            by: None,
        });
        assert_eq!(line, "Secretary: Noting it. [asked remember]");
    }
}
