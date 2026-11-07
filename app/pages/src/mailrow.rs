//! What every mail page says of a message. Every string here came out of mail and is set as text;
//! a body is the host's part (`mailbody`), which draws a held value's chip in it.

use suzatary_core::mail::{Disposed, MailRow};
use viewtree::Attr::{Class, Href};
use viewtree::{Node, view};

use crate::chrome::{chip, mark};
use crate::places::Place;
use crate::zone::{Offset, minute};

/// `None` when triage made nothing of it yet; `skip` prints as the agent moving on.
#[must_use]
pub(crate) fn badge(d: &Disposed) -> Option<(&'static str, &'static str)> {
    match d {
        Disposed::Undecided => None,
        Disposed::Todo { .. } => Some(("todo", "ok")),
        Disposed::Noise => Some(("noise", "")),
        Disposed::Skip => Some(("seen", "")),
        Disposed::Unkeyed => Some(("no dedup key", "")),
    }
}

#[must_use]
pub(crate) fn subject(row: &MailRow) -> &str {
    if row.subject.trim().is_empty() {
        "(no subject)"
    } else {
        &row.subject
    }
}

/// The whole address stays on the row's `title`.
#[must_use]
pub(crate) fn sender(row: &MailRow) -> &str {
    match row.sender.find('@') {
        Some(at) if at > 0 => &row.sender[..at],
        Some(_) | None => &row.sender,
    }
}

/// On the owner's clock; `""` when its Date would not parse.
#[must_use]
pub(crate) fn arrived(offsets: &[Offset], row: &MailRow) -> String {
    row.date
        .map_or_else(String::new, |d| minute(offsets, &d.isoformat()))
}

#[must_use]
pub(crate) fn account<M>(row: &MailRow) -> Option<Node<M>> {
    (!row.account.is_empty()).then(|| chip(&row.account, "cat"))
}

#[must_use]
pub(crate) fn folder<M>(row: &MailRow) -> Node<M> {
    chip(&row.container, "kind")
}

#[must_use]
pub(crate) fn decided<M>(row: &MailRow) -> Option<Node<M>> {
    badge(&row.disposition).map(|(label, kind)| chip(label, kind))
}

#[must_use]
pub(crate) fn todo<M>(row: &MailRow) -> Option<Node<M>> {
    match &row.disposition {
        Disposed::Todo { todo } => Some(view! { Code "cardid" [todo.as_str()] }),
        Disposed::Undecided | Disposed::Noise | Disposed::Skip | Disposed::Unkeyed => None,
    }
}

/// Drawn by the host's part so a held value's placeholder is its chip.
#[must_use]
pub(crate) fn body<M>(body: &str) -> Node<M> {
    let text = if body.is_empty() {
        "(this message has no readable body)"
    } else {
        body
    };
    crate::chrome::prose(
        crate::part::Tag::Div,
        "mailbody",
        text,
        None,
        crate::chrome::Chips::Mail,
    )
}

#[must_use]
pub(crate) fn onward<M>(class: &str, place: Place<'_>, words: &str) -> Node<M> {
    view! { A {Class => class, Href => place.href()} [words, mark(crate::part::Mark::Chevron)] }
}

/// One phrase per kind of held value a message names, never a value.
#[must_use]
pub(crate) fn held<M>(row: &MailRow) -> Vec<Node<M>> {
    row.held
        .iter()
        .map(|k| chip(&format!("names a held {}", k.name()), "kind"))
        .collect()
}
