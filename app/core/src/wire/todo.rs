use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::record::SubTodo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Link {
    /// `literal` rehashes to it.
    pub hash: String,
    /// None at a genesis.
    pub parents: Vec<String>,
    pub literal: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub holds: Holds,
}

/// An envelope (a merge, a genesis, a signature) claims no todo, writer or instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "holds", rename_all = "lowercase")]
pub enum Holds {
    Envelope {
        kind: String,
    },
    Event {
        kind: String,
        todo: String,
        actor: String,
        /// ISO 8601.
        at: String,
        note: String,
        reverses: Reverses,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Reverses {
    Nothing,
    Undo {
        /// By hash.
        of: String,
    },
    /// Re-applies what the undo `of` undid.
    Redo {
        /// By hash.
        of: String,
    },
}

impl From<Option<crate::authoring::Reversal>> for Reverses {
    fn from(reversal: Option<crate::authoring::Reversal>) -> Self {
        use crate::authoring::Reversal;
        match reversal {
            None => Self::Nothing,
            Some(Reversal::Undo { of }) => Self::Undo {
                of: of.as_str().to_owned(),
            },
            Some(Reversal::Redo { of }) => Self::Redo {
                of: of.as_str().to_owned(),
            },
        }
    }
}

/// The message a todo was authored from; `""` where its era recorded nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AuthoredSource {
    pub sender: String,
    pub subject: String,
    /// ISO 8601, date only.
    pub date: String,
    /// A prefix of the canonical hash; `/api/mail/message/<hash>` resolves it.
    pub hash: String,
    pub thread_id: String,
    pub message_id: String,
}

/// Typst's HTML for the prose fields; `null` when absent, not rendered, or untrusted, and the page
/// shows the source beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Rich {
    pub body: Option<String>,
    pub detail: Option<String>,
    pub rationale: Option<String>,
    /// From string source.
    pub waiting_on: Option<String>,
    /// Each present field's HTML key; `null` for an absent or untrusted one.
    pub keys: RichKeys,
}

/// Each field's key in the rendered-HTML tabulation ([`crate::template::Template::key`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RichKeys {
    pub body: Option<String>,
    pub detail: Option<String>,
    pub rationale: Option<String>,
    /// Escaped.
    pub waiting_on: Option<String>,
}

/// A decision is a choice awaiting the owner's tap, never a todo on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    Decision,
    /// `todo`, or one whose name files it (`ekaTodo`).
    Todo {
        name: String,
    },
}

impl Block {
    #[must_use]
    pub fn of(name: &str) -> Self {
        match name {
            "decision" => Self::Decision,
            other => Self::Todo {
                name: other.to_owned(),
            },
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Decision => "decision",
            Self::Todo { name } => name,
        }
    }
}

/// A todo's content as the confirmed fold sees it. `""` is absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Authored {
    pub rich: Rich,
    pub block: Block,
    /// ISO 8601 date only.
    pub created: String,
    /// Typst markup.
    pub body: String,
    /// [`crate::escape::markup_text`] of the markup, never rendered.
    pub text: String,
    /// Typst markup.
    pub detail: String,
    pub category: String,
    /// Typst string source.
    pub waiting_on: String,
    /// The `//` comment above the priority, line by line.
    pub rationale: Vec<String>,
    /// The authored FPL spec, printed by [`crate::term_json::to_json`].
    #[ts(type = "SpecNode | null")]
    pub spec: Option<serde_json::Value>,
    pub actor: String,
    /// ISO 8601.
    pub at: String,
    pub source: Option<AuthoredSource>,
    pub subtodos: Vec<SubTodo>,
}

/// The `From` matches `Outcome` with no wildcard, so a new binding fails the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    Open,
    Completed,
    Cancelled,
}

impl From<Option<prodrome_core::fpl::Outcome>> for EntryState {
    fn from(outcome: Option<prodrome_core::fpl::Outcome>) -> Self {
        match outcome {
            None => EntryState::Open,
            Some(prodrome_core::fpl::Outcome::Completed(_)) => EntryState::Completed,
            Some(prodrome_core::fpl::Outcome::Cancelled(_)) => EntryState::Cancelled,
        }
    }
}

/// What an untrusted actor's event would bind where the confirmed fold disagrees; `""` agrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ClaimedState {
    #[serde(rename = "")]
    Agrees,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
}

impl From<Option<prodrome_core::fpl::Outcome>> for ClaimedState {
    fn from(claim: Option<prodrome_core::fpl::Outcome>) -> Self {
        match claim {
            None => ClaimedState::Agrees,
            Some(prodrome_core::fpl::Outcome::Completed(_)) => ClaimedState::Completed,
            Some(prodrome_core::fpl::Outcome::Cancelled(_)) => ClaimedState::Cancelled,
        }
    }
}

/// A number in [0, 1] (low is urgent), or `"absent"` for `∅`; `null` where it does not link.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Fulfillment {
    Number(f64),
    Absent(Absent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Absent {
    #[serde(rename = "absent")]
    Absent,
}

impl Fulfillment {
    /// `None` where it does not link.
    #[must_use]
    pub fn of<E>(value: Result<Option<f64>, E>) -> Option<Fulfillment> {
        value.ok().map(Fulfillment::of_value)
    }

    #[must_use]
    pub fn of_value(value: Option<f64>) -> Fulfillment {
        value.map_or(Fulfillment::Absent(Absent::Absent), Fulfillment::Number)
    }

    /// `None` for `∅`.
    #[must_use]
    pub fn number(self) -> Option<f64> {
        match self {
            Fulfillment::Number(value) => Some(value),
            Fulfillment::Absent(Absent::Absent) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Entry {
    pub todo: String,
    /// The binding a list files it under: open while any candidate is.
    pub state: EntryState,
    /// One unless the state is in conflict.
    pub states: Vec<EntryState>,
    /// `""` while open.
    pub at: String,
    /// Shown, never applied: the containment asymmetry made visible.
    pub claimed: ClaimedState,
    /// `null` where it does not link.
    pub value: Option<Fulfillment>,
    /// A pass restarts its price (`Tended`, "Did it") rather than completing it.
    pub recurring: bool,
    /// Provisional content, or a claim the trusted fold refuses.
    pub unconfirmed: bool,
    /// Registers with more than one live write, by hash.
    #[ts(type = "Record<string, string[]>")]
    pub conflicts: BTreeMap<String, Vec<String>>,
    /// `null` for a todo never authored; under a conflict, the first of `candidates`.
    pub authored: Option<Authored>,
    /// By object name, where the content is in conflict; else empty.
    pub candidates: Vec<Authored>,
    /// In causal order, by hash.
    pub stream: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Neighbour {
    pub todo: String,
    pub state: EntryState,
    pub value: Option<Fulfillment>,
    pub unconfirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FieldChange {
    pub field: String,
    /// `""` for the first record.
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GroupBy {
    All,
    Category,
    Urgency,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Worth {
    Value(f64),
    Absent,
    /// A `Ref` to a todo the chain has never seen.
    Unknown(String),
    /// A reference loop, its first todo repeated at the end.
    Cycle(Vec<String>),
}

/// One node of the list's tree; `term` prints what was evaluated, children ascend.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TreeNode {
    Group {
        by: GroupBy,
        /// `""` for the root.
        label: String,
        worth: Worth,
        term: String,
        children: Vec<TreeNode>,
    },
    /// `Ref(todo)`, its subtodos as children.
    Todo {
        todo: String,
        /// Plain text.
        title: String,
        category: String,
        worth: Worth,
        term: String,
        children: Vec<TreeNode>,
    },
}
