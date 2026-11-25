//! The digest's view as a reader is shown it: a tainted part by its size and kinds alone, never
//! its words.
//!
//! Every part prints on exactly one line, so a part's words cannot reach past its own line, and
//! a marker at the head of a line fences what follows it on that line.

use std::collections::BTreeMap;

use prodrome_data::event::Hash;
use prodrome_digest::{Measure as _, Node, Part};

use crate::secretary::digest::Digest;
use crate::secretary::digest_label::{Kind, Label};
use crate::secretary::digest_line::Line;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::store::{Entry, Retired};
use suzatary_conversations::secretary::tool_name::ToolName;
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_inbox::secretary::stamp::Stamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub part: Part,
    /// The join of what the part stands for.
    pub label: Label,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Said(String),
    /// A summary: words a compactor derived from the lines beneath it, nobody's own.
    Derived(Line),
    /// A part its reader is not shown the words of, by its kinds.
    Hidden(Label),
    Unsummarised(Label),
}

/// The most bytes one rendered or opened line holds; a longer one is cut and says so.
pub const LINE_BYTES: usize = 4_096;

pub const VIEW_BYTES: usize = 131_072;

/// The view of `digest` under `budget` bytes: a part `shows` reads as its words, any other
/// hidden by its kinds.
#[must_use]
pub fn view(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    budget: usize,
    shows: fn(&Label) -> bool,
) -> Vec<Shown> {
    let budget = u64::try_from(budget).unwrap_or(u64::MAX);
    let Some(view) = digest.view(budget) else {
        return Vec::new();
    };
    view.parts
        .into_iter()
        .filter_map(|part| {
            let node = digest.node(&part.node)?;
            let label = digest.label(node);
            let body = shown_as(label, shows, || said(digest, entries, node))?;
            Some(Shown { part, label, body })
        })
        .collect()
}

/// What lies beneath the lines `handle` names (`first+lines`), a part `reads` admits as it is
/// and any other hidden by its kinds, with the join of the shown parts' labels. `None` for a
/// handle naming no lines.
#[must_use]
pub fn opened(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    handle: &str,
    reads: fn(&Label) -> bool,
) -> Option<(Label, Vec<Shown>)> {
    let nodes = digest.tree()?.cover(span_of(handle)?)?;
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
        .map(|child| digest.label(child))
        .filter(&reads)
        .fold(Label::empty(), |label, child| label.join(&child));
    let shown = beneath
        .into_iter()
        .map(|child| {
            let label = digest.label(child);
            let body = shown_as(label, reads, || {
                Some(if child.is_leaf() {
                    Body::Said(cut(&whole(entries.get(digest.line(child.name()))?)))
                } else {
                    said(digest, entries, child).unwrap_or(Body::Unsummarised(*child.measure()))
                })
            })?;
            Some(Shown {
                part: part(child),
                label,
                body,
            })
        })
        .collect::<Option<_>>()?;
    Some((label, shown))
}

/// What stands for a part labelled `label` to a reader that `reads`: its words, or else its
/// kinds alone. The one rule every rendering of the digest shows a part by.
pub(crate) fn shown_as(
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

/// The node whose summary `handle` names. `None` for a handle naming no node that shows a
/// summary, so nothing a reader is not shown can be named.
#[must_use]
pub fn summarized(digest: &Digest, handle: &str) -> Option<Hash> {
    let &[mut node] = digest.tree()?.cover(span_of(handle)?)?.as_slice() else {
        return None;
    };
    loop {
        if digest.summary(node.name()).is_some() {
            return Some(node.name().clone());
        }
        let [only] = node.children() else {
            return None;
        };
        node = only;
    }
}

fn span_of(handle: &str) -> Option<std::ops::Range<usize>> {
    let (first, lines) = handle.trim().split_once('+')?;
    let first: usize = first.parse().ok()?;
    Some(first..first.checked_add(lines.parse().ok()?)?)
}

fn part(node: &Node<Label>) -> Part {
    Part {
        node: node.name().clone(),
        span: node.span(),
    }
}

fn said(digest: &Digest, entries: &BTreeMap<&Hash, &Entry>, node: &Node<Label>) -> Option<Body> {
    if node.is_leaf() {
        entries
            .get(digest.line(node.name()))
            .map(|entry| Body::Said(rendered(entry)))
    } else {
        digest.summary(node.name()).cloned().map(Body::Derived)
    }
}

/// The earliest of `shown` within `budget` bytes, a prefix a model's cache keeps; a line says how
/// many were left out. The compactor's view; a reader's is `told`'s, which names what it cuts by a
/// handle.
#[must_use]
pub fn fit_earliest(shown: Vec<String>, budget: usize) -> Vec<String> {
    let mut bytes = 0;
    let kept = shown
        .iter()
        .take_while(|line| {
            bytes += line.len() + 1;
            bytes <= budget
        })
        .count();
    let left = shown.len() - kept;
    let mut lines: Vec<String> = shown.into_iter().take(kept).collect();
    if left > 0 {
        lines.push(format!(
            "({left} later parts of the log are not shown here.)"
        ));
    }
    lines
}

impl Body {
    /// Its one line, with no handle.
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

#[must_use]
pub fn kinds(label: Label) -> String {
    label.kinds.iter().map(word).collect::<Vec<_>>().join(", ")
}

#[must_use]
pub fn word(kind: Kind) -> &'static str {
    match kind {
        Kind::Opened => "an opening",
        Kind::Titled => "a title",
        Kind::Person => "the owner's words",
        Kind::Model => "the secretary's answers",
        Kind::Tool => "tool answers",
        Kind::Act => "the owner's acts",
        Kind::Noted => "the box's notes",
        Kind::Proposed => "proposals",
        Kind::Reviewed => "reviews",
        Kind::ReviewFailed => "failed reviews",
        Kind::Stopped => "stops",
        Kind::Rewound => "retries",
        Kind::MemorySeen => "retired notes",
        Kind::Session => "the box's bookkeeping",
        Kind::Coding => "coding sessions",
        Kind::Erased => "erased words",
    }
}

/// One stored line as one line of text, its spaces joined and cut at [`LINE_BYTES`].
#[must_use]
pub fn rendered(entry: &Entry) -> String {
    cut(&whole(entry)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

/// One stored line whole, with no id, hash or object name.
#[must_use]
pub fn whole(entry: &Entry) -> String {
    match entry {
        Entry::Opened { title, .. } if title.get().is_some_and(|title| title.trim().is_empty()) => {
            "A conversation opened.".to_owned()
        }
        Entry::Opened { title, .. } => format!("A conversation opened: {title}"),
        Entry::SetUp { title, .. } => format!("A conversation set up: {title}"),
        Entry::Titled { title, .. } => format!("The conversation was named: {title}"),
        Entry::Said { turn, by, .. } => said_turn(turn, by.as_ref()),
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
        Entry::Rewound { .. } | Entry::Again { .. } => "The owner asked again.".to_owned(),
        Entry::Retired {
            retired: Retired::MemorySeen | Retired::Session,
            ..
        } => "A line of retired bookkeeping.".to_owned(),
        Entry::Scoped { scope, .. } => format!("The model is asked over its {}.", scope.as_str()),
        Entry::Retired {
            retired: Retired::Unheard { model },
            ..
        } => format!("A line of retired bookkeeping about a turn {model} said."),
        Entry::Coding { .. } => "A coding session moved on.".to_owned(),
        Entry::Began { covers, .. } => format!(
            "The secretary began answering {} turn{}.",
            covers.len(),
            if covers.len() == 1 { "" } else { "s" }
        ),
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

/// A turn, a call as its verb's name and never its arguments, so a remember card's words are
/// never shown.
fn said_turn(turn: &Turn, by: Option<&Stamp>) -> String {
    let acted = if Kind::act(by) == Kind::Noted {
        "The box noted"
    } else {
        "Owner did"
    };
    match turn {
        Turn::Person {
            text, attachments, ..
        } if attachments.is_empty() => format!("Owner: {text}"),
        Turn::Person {
            text, attachments, ..
        } => format!(
            "Owner, with {} attached ({}): {text}",
            attachments.len(),
            attachments
                .iter()
                .map(|attachment| attachment.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
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
        Turn::Act { text, .. } => format!("{acted}: {text}"),
        Turn::Erased { shape, words } => {
            let prose = suzatary_conversations::secretary::turn_shape::prose(words);
            match shape {
                TurnShape::Person { .. } => format!("Owner: {prose}"),
                TurnShape::Model { calls, .. } => {
                    let verbs: Vec<&str> = calls.iter().map(ToolName::as_str).collect();
                    format!("Secretary: {prose} [asked {}]", verbs.join(", "))
                }
                TurnShape::Tool { verb, .. } => format!("{} answered: {prose}", verb.as_str()),
                TurnShape::Act { .. } => format!("{acted}: {prose}"),
            }
        }
    }
}

/// `text` within [`LINE_BYTES`], cut at a character's edge.
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
    use suzatary_conversations::secretary::model::ToolCall;

    #[test]
    fn a_call_renders_as_its_verb_and_never_its_words() {
        let mut args = Map::new();
        args.insert(
            "fact".to_owned(),
            serde_json::json!("the owner's sister lives in London"),
        );
        let line = rendered(&Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 10, 6, 9, 0, 0, 0)
                .expect("an instant"),
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
