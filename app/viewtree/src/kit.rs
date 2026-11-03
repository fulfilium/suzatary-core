//! Parts every page draws the same way, as functions of their values: a reading on its way, a
//! paragraph, a button that may be busy, a disclosure.

use crate::node::{Child, Node};
use crate::{Attr, view};

/// A value the page reads: on its way, failed with a reason, or there; the web's `lo` printed
/// the same way, so a host hands one over as it holds it.
#[cfg(feature = "host")]
#[derive(Debug, Clone, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "type")]
pub enum Reading<T> {
    /// On its way.
    Loading,
    /// It could not be read.
    Error {
        /// Why.
        e: String,
    },
    /// Read.
    Val {
        /// What was read.
        v: T,
    },
}

#[cfg(feature = "host")]
impl<T> Reading<T> {
    /// The value read, made into another; on its way or failed as it was.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Reading<U> {
        match self {
            Reading::Loading => Reading::Loading,
            Reading::Error { e } => Reading::Error { e },
            Reading::Val { v } => Reading::Val { v: f(v) },
        }
    }

    /// Two readings as one: failed when either failed (the first's reason first), on its way
    /// when either is, else both values.
    pub fn both<'a, U>(&'a self, other: &'a Reading<U>) -> Reading<(&'a T, &'a U)> {
        match (self, other) {
            (Reading::Error { e }, _) | (_, Reading::Error { e }) => {
                Reading::Error { e: e.clone() }
            }
            (Reading::Loading, _) | (_, Reading::Loading) => Reading::Loading,
            (Reading::Val { v: a }, Reading::Val { v: b }) => Reading::Val { v: (a, b) },
        }
    }

    /// Whether it has settled, as a reading of nothing: what a page waits for and never shows.
    #[must_use]
    pub fn settled(&self) -> Reading<()> {
        match self {
            Reading::Loading => Reading::Loading,
            Reading::Error { .. } | Reading::Val { .. } => Reading::Val { v: () },
        }
    }
}

/// Words in a paragraph of a class.
pub fn para<M>(class: &str, words: impl Child<M>) -> Node<M> {
    view! { P {Attr::Class => class} [words] }
}

/// A button of a class that sends `m`, disabled while `busy`.
pub fn button<M>(class: &str, label: &str, m: M, busy: bool) -> Node<M> {
    view! { Button {Attr::Type => "button", Attr::Class => class, Attr::Disabled => busy} @click(m) [label] }
}

/// A summary that opens onto `body`.
pub fn disclosure<M>(class: &str, summary: impl Child<M>, body: impl Child<M>) -> Node<M> {
    view! { Details {Attr::Class => class} [view! { Summary [summary] }, body] }
}
