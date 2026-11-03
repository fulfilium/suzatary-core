//! What every mail page says of a message: the box's decisions in the words a reader sees, and
//! the parts the list, a message and a thread draw alike. Every string here came out of mail and
//! is set as text; a body is the host's part (`mailbody`), which draws a held value's chip in it.

use suzatary_core::mail::{Disposed, MailRow};
use viewtree::Attr::{Class, Href};
use viewtree::node::foreign;
use viewtree::{Node, view};

use crate::chrome::{chip, mark};
use crate::places::Place;
use crate::zone::{Offset, minute};

/// What triage made of a message, as a chip's label and kind; `None` when it made nothing of it
/// yet. `skip` prints as the agent moving on.
#[must_use]
pub fn badge(d: &Disposed) -> Option<(&'static str, &'static str)> {
    match d {
        Disposed::Undecided => None,
        Disposed::Todo { .. } => Some(("todo", "ok")),
        Disposed::Noise => Some(("noise", "")),
        Disposed::Skip => Some(("seen", "")),
    }
}

/// A missing subject says so, rather than leaving a gap.
#[must_use]
pub fn subject(row: &MailRow) -> &str {
    if row.subject.trim().is_empty() {
        "(no subject)"
    } else {
        &row.subject
    }
}

/// The local part of an address; the whole address stays on the row's `title`.
#[must_use]
pub fn sender(row: &MailRow) -> &str {
    match row.sender.find('@') {
        Some(at) if at > 0 => &row.sender[..at],
        Some(_) | None => &row.sender,
    }
}

/// When it arrived, on the owner's clock; `""` when its Date would not parse.
#[must_use]
pub fn arrived(offsets: &[Offset], row: &MailRow) -> String {
    row.date
        .map_or_else(String::new, |d| minute(offsets, &d.isoformat()))
}

/// The account's chip, for a row of an account of this deployment.
#[must_use]
pub fn account<M>(row: &MailRow) -> Option<Node<M>> {
    (!row.account.is_empty()).then(|| chip(&row.account, "cat"))
}

/// The folder's chip.
#[must_use]
pub fn folder<M>(row: &MailRow) -> Node<M> {
    chip(&row.container, "kind")
}

/// What triage made of it, as a chip.
#[must_use]
pub fn decided<M>(row: &MailRow) -> Option<Node<M>> {
    badge(&row.disposition).map(|(label, kind)| chip(label, kind))
}

/// The todo it became, by id.
#[must_use]
pub fn todo<M>(row: &MailRow) -> Option<Node<M>> {
    match &row.disposition {
        Disposed::Todo { todo } => Some(view! { Code "cardid" [todo.as_str()] }),
        Disposed::Undecided | Disposed::Noise | Disposed::Skip => None,
    }
}

/// Ask the secretary about a reference: a link, so it middle-clicks and Back returns here.
#[must_use]
pub fn ask<M>(reference: &str) -> Node<M> {
    view! { Div "askrow" [view! { A "asksecretary" {Href => Place::Ask(reference).href()} ["Ask the secretary"] }] }
}

/// A body as text, drawn by the host's part so a held value's placeholder is its chip.
#[must_use]
pub fn body<M>(row: &MailRow, body: &str) -> Node<M> {
    let text = if body.is_empty() {
        "(this message has no readable body)"
    } else {
        body
    };
    foreign("mailbody", &row.hash, text)
}

/// A link that reads as a disclosure: words, then the chevron.
#[must_use]
pub fn onward<M>(class: &str, place: Place<'_>, words: &str) -> Node<M> {
    view! { A {Class => class, Href => place.href()} [words, mark("chevron")] }
}

/// What a row says of the held values a message names: one phrase per kind, never a value.
#[must_use]
pub fn held<M>(row: &MailRow) -> Vec<Node<M>> {
    row.held
        .iter()
        .map(|k| chip(&format!("names a held {}", k.name()), "kind"))
        .collect()
}
