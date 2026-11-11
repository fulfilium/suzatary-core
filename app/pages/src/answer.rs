//! A model's answer as every page draws it: block by block, typst's HTML through the tripwire or
//! the source as text, each with its Copy, and the references it names linked under it.

use suzatary_base::reference::mentions;
use suzatary_core::wire::topics::BlockRow;
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
