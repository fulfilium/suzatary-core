//! A model's answer as every page draws it: block by block, typst's HTML through the tripwire or
//! the source as text, each with its Copy, and the references it names linked under it.

use std::collections::BTreeMap;

use serde::Deserialize;
use suzatary_base::reference::mentions;
use suzatary_topics::wire::topics::{BlockRow, ProposalRow, Speaker, TurnRow};
use ts_rs::TS;
use viewtree::Attr::{AriaLabel, Class, Href, Open, Title, Type};
use viewtree::{Node, view};

use crate::chrome::rich_block;
use crate::places::Place;

/// Copies words or Typst, never markup.
pub(crate) fn copy<M: Clone>(label: &str, class: &'static str, m: M) -> Node<M> {
    view! { Button {Type => "button", Class => class, Title => format!("{label} to the clipboard")} @click(m) [label] }
}

pub(crate) fn blocks<M: Clone>(
    text: &str,
    blocks: &[BlockRow],
    copied: impl Fn(String) -> M,
) -> Node<M> {
    let found = mentions(text);
    view! { Div "turnanswer" [
        blocks.iter().map(|b| view! { Div "answerblockwrap" [
            rich_block(b.html.as_deref(), &b.source, b.kind),
            copy("Copy", "copyblock", copied(b.plain.clone())),
        ] }).collect::<Vec<_>>(),
        (!found.is_empty()).then(|| view! { Div "turnrefs" [found.iter().map(|r| view! { A {Class => format!("reflink turnref ref-{}", r.scheme()), Href => Place::Reference(r).href()} [r.to_string()] }).collect::<Vec<_>>()] }),
    ] }
}

/// The bytes of the words a turn draws: a model's blocks as typst's HTML or their source,
/// anything else as its text. What bounds a slice (`slices::WORDS`).
#[must_use]
pub(crate) fn words(t: &TurnRow) -> usize {
    match &t.who {
        Speaker::Model { blocks } if !blocks.is_empty() => blocks
            .iter()
            .map(|b| b.html.as_ref().map_or(b.source.len(), String::len))
            .sum(),
        Speaker::Model { .. }
        | Speaker::Person { .. }
        | Speaker::Verb { .. }
        | Speaker::Act { .. }
        | Speaker::Box { .. } => t.text.len(),
    }
}

/// What Copy copies.
#[must_use]
pub(crate) fn plain(blocks: &[BlockRow]) -> String {
    blocks
        .iter()
        .map(|b| b.plain.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The answer's copies, and its whole source behind a disclosure, for the menu under it.
pub(crate) fn copies<M: Clone>(
    text: &str,
    blocks: &[BlockRow],
    copied: impl Fn(String) -> M,
) -> Vec<Node<M>> {
    vec![
        copy("Copy", "copyanswer", copied(plain(blocks))),
        copy("Copy Typst", "copyanswer", copied(text.to_owned())),
        view! { Details "turnsource" [view! { Summary ["Show as text"] }, view! { Div "turntext" [text] }] },
    ]
}

/// The rarely used controls behind one "…": nothing when there are none, open while one of them
/// is mid-act.
#[must_use]
pub(crate) fn more<M>(items: Vec<Node<M>>, open: bool) -> Option<Node<M>> {
    (!items.is_empty()).then(|| {
        view! { Details "turnmore" {Open => open} [
            view! { Summary "turnmorebutton" {Title => "More", AriaLabel => "More"} ["…"] },
            view! { Div "turnmenu" [items] },
        ] }
    })
}

/// A reading as the host dressed it with what only the box says (`web/src/chat/dressed.ts`): the
/// HTML each model block renders to, by its source, and each proposal it holds as the box drew it
/// against its chain, by its index. What the box did not say stays as the replicas read it.
#[derive(Debug, Clone, Deserialize, TS)]
pub(crate) struct Dressed<T> {
    pub(crate) read: T,
    pub(crate) html: BTreeMap<String, String>,
    pub(crate) drawn: Vec<ProposalRow>,
}

/// A reading that holds model turns, and proposals the box draws.
pub(crate) trait Dressable {
    fn turns(&mut self) -> impl Iterator<Item = &mut TurnRow>;
    fn proposals(&mut self) -> impl Iterator<Item = &mut ProposalRow>;
}

impl<T: Dressable> Dressed<T> {
    /// The reading with its blocks' HTML and its proposals' rows in place.
    pub(crate) fn worn(self) -> T {
        let Dressed {
            mut read,
            html,
            drawn,
        } = self;
        for t in read.turns() {
            match &mut t.who {
                Speaker::Model { blocks } => {
                    for b in blocks {
                        if let Some(h) = html.get(&b.source) {
                            b.html = Some(h.clone());
                        }
                    }
                }
                Speaker::Person { .. }
                | Speaker::Verb { .. }
                | Speaker::Act { .. }
                | Speaker::Box { .. } => {}
            }
        }
        let mut drawn: BTreeMap<usize, ProposalRow> =
            drawn.into_iter().map(|p| (p.index, p)).collect();
        for p in read.proposals() {
            if let Some(row) = drawn.remove(&p.index) {
                *p = row;
            }
        }
        read
    }
}

/// A dressed reading as a page holds it: worn once, as it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Worn<T>(pub(crate) Box<T>);

impl<'de, T: Dressable + Deserialize<'de>> Deserialize<'de> for Worn<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Dressed::<T>::deserialize(d).map(|t| Worn(Box::new(t.worn())))
    }
}
