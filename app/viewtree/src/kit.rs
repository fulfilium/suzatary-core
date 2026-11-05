//! Parts every page draws the same way, as functions of their values.

use crate::node::{Child, Node};
use crate::{Attr, view};

/// Printed the same way as the web's `lo`, so a host hands one over as it holds it.
#[cfg(feature = "host")]
#[derive(Debug, Clone, PartialEq, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "type")]
pub enum Reading<T> {
    Loading,
    Error { e: String },
    Val { v: T },
}

#[cfg(feature = "host")]
impl<T> Reading<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Reading<U> {
        match self {
            Reading::Loading => Reading::Loading,
            Reading::Error { e } => Reading::Error { e },
            Reading::Val { v } => Reading::Val { v: f(v) },
        }
    }

    /// An error wins over loading; the first's error wins over the second's.
    pub fn both<'a, U>(&'a self, other: &'a Reading<U>) -> Reading<(&'a T, &'a U)> {
        match (self, other) {
            (Reading::Error { e }, _) | (_, Reading::Error { e }) => {
                Reading::Error { e: e.clone() }
            }
            (Reading::Loading, _) | (_, Reading::Loading) => Reading::Loading,
            (Reading::Val { v: a }, Reading::Val { v: b }) => Reading::Val { v: (a, b) },
        }
    }

    /// What a page waits for and never shows.
    #[must_use]
    pub fn settled(&self) -> Reading<()> {
        match self {
            Reading::Loading => Reading::Loading,
            Reading::Error { .. } | Reading::Val { .. } => Reading::Val { v: () },
        }
    }
}

pub fn para<M>(class: &str, words: impl Child<M>) -> Node<M> {
    view! { P {Attr::Class => class} [words] }
}

pub fn button<M>(class: &str, label: &str, m: M, busy: bool) -> Node<M> {
    view! { Button {Attr::Type => "button", Attr::Class => class, Attr::Disabled => busy} @click(m) [label] }
}

pub fn disclosure<M>(class: &str, summary: impl Child<M>, body: impl Child<M>) -> Node<M> {
    view! { Details {Attr::Class => class} [view! { Summary [summary] }, body] }
}
