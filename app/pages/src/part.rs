//! The parts a page names and the host draws (`web/src/pages/parts.ts`).

use serde::{Deserialize, Serialize};
use suzatary_core::answer::Kind;
use suzatary_core::wire::todo::Entry;
use suzatary_core::wire::topics::Speaker;
use ts_rs::TS;
use viewtree::Node;
use viewtree::node::foreign;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Part {
    Mark {
        mark: Mark,
    },
    /// A fraction in [0, 1] on the fulfillment scale.
    Pie {
        share: f64,
    },
    /// Typst's HTML through the tripwire (`web/src/rich.ts`), or its source as text.
    Rich {
        html: Option<String>,
        text: String,
        class: String,
        kind: Option<Kind>,
    },
    /// A stranger's or a model's text, drawn as text nodes and built links only.
    Prose {
        tag: Tag,
        class: String,
        text: String,
        who: Option<Who>,
        held: bool,
        conversation: Option<String>,
    },
    Elapsed {
        lead: String,
    },
    Editor {
        field: String,
        class: String,
        label: String,
        rows: u32,
        text: String,
    },
    Todos {
        entries: Vec<Entry>,
    },
    Curve {
        span: Span,
    },
    TypstView {
        entries: Vec<Entry>,
        at: String,
        chain: String,
        open: String,
        writes: bool,
    },
}

/// A mark the host draws in SVG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    Outline,
    Chevron,
    Rule,
    Loadmark,
    Search,
    Diamond,
    Send,
    Clip,
    Check,
    Cross,
    Plus,
    Doc,
    Lock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "ProseTag")]
pub enum Tag {
    Div,
    P,
    Pre,
    Span,
}

/// A turn's speaker, as the host marks its words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Who {
    Person,
    Model,
    Verb,
    Act,
    Box,
}

impl Who {
    #[must_use]
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Who::Person => "person",
            Who::Model => "model",
            Who::Verb => "verb",
            Who::Act => "act",
            Who::Box => "box",
        }
    }

    #[must_use]
    pub(crate) const fn of(s: &Speaker) -> Who {
        match s {
            Speaker::Person { .. } => Who::Person,
            Speaker::Model { .. } => Who::Model,
            Speaker::Verb { .. } => Who::Verb,
            Speaker::Act { .. } => Who::Act,
            Speaker::Box { .. } => Who::Box,
        }
    }
}

/// A todo's thirty days as the core in the worker samples them (`web/src/todo/thirty.ts`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum Thirty {
    Sampling,
    Unheld,
    Unpriced,
    Drawn { span: Span },
}

/// `web/src/todo/curve.ts`'s `Span`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Span {
    Unsampled,
    Sampled { drawn: Drawn },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "CurveDrawn")]
pub struct Drawn {
    window: Window,
    past: Vec<Sample>,
    ahead: Vec<Sample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "CurveWindow")]
pub struct Window {
    from: f64,
    now: f64,
    to: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Sample {
    t: f64,
    v: f64,
}

/// `key` names the part among its siblings, so new props reach the same part.
#[must_use]
pub(crate) fn part<M>(key: impl Into<String>, p: &Part) -> Node<M> {
    foreign(
        "part",
        key,
        serde_json::to_string(p).expect("a part's fields are strings, numbers and lists"),
    )
}
