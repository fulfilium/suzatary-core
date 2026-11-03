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
