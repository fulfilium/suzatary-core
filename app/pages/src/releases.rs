//! The releases the project chain reads, as a part of About.

use suzatary_core::release::{Lot, Named, Place};
use suzatary_core::wire::cards::HeldTodo;
use suzatary_core::wire::todo::Fulfillment;
use suzatary_core::wire::{ReleaseItem, ReleaseRow, ReleasesPayload};
use viewtree::Attr::{Class, Data};
use viewtree::kit::Reading;
use viewtree::{Node, view};

use crate::chrome::{chip, pie};

/// An open item a revision dropped is kept apart from what is owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Owed,
    Done,
    Cancelled,
    Dropped,
}

impl Mark {
    /// An item completed once is done, whatever its state reads today.
    fn of(i: &ReleaseItem) -> Self {
        match (i.lot, i.named) {
            (Lot::Completed, Named::Current | Named::Dropped) => Mark::Done,
            (Lot::Cancelled, Named::Current | Named::Dropped) => Mark::Cancelled,
            (Lot::Open, Named::Dropped) => Mark::Dropped,
            (Lot::Open, Named::Current) => Mark::Owed,
        }
    }

    fn rank(self) -> usize {
        match self {
            Mark::Owed => 0,
            Mark::Done => 1,
            Mark::Cancelled => 2,
            Mark::Dropped => 3,
        }
    }

    fn class(self) -> &'static str {
        match self {
            Mark::Owed => "releaseitem releaseitem-owed",
            Mark::Done => "releaseitem releaseitem-done",
            Mark::Cancelled => "releaseitem releaseitem-cancelled",
            Mark::Dropped => "releaseitem releaseitem-dropped",
        }
    }
}

/// A cancelled or dropped item leaves the count of what is owed.
fn sentence(r: &ReleaseRow) -> String {
    let mut n = [0usize; 4];
    for i in &r.items {
        n[Mark::of(i).rank()] += 1;
    }
    let [owed, done, cancelled, dropped] = n;
    let asides: Vec<String> = [(cancelled, "cancelled"), (dropped, "moved out")]
        .into_iter()
        .filter(|(k, _)| *k > 0)
        .map(|(k, w)| format!("{k} {w}"))
        .collect();
    if owed + done == 0 {
        "No items yet.".to_owned()
    } else if asides.is_empty() {
        format!("{done} of {} done.", owed + done)
    } else {
        format!("{done} of {} done, {}.", owed + done, asides.join(", "))
    }
}

fn standing(r: &ReleaseRow) -> &'static str {
    match (r.lot, r.place) {
        (Lot::Completed, Place::Earlier | Place::Next | Place::Later) => "Shipped.",
        (Lot::Cancelled, Place::Earlier | Place::Next | Place::Later) => "Cancelled.",
        (Lot::Open, Place::Next) => "In progress.",
        (Lot::Open, Place::Later) => "Later.",
        (Lot::Open, Place::Earlier) => "Passed over.",
    }
}

/// `unpriced` where a held todo has no fulfillment; nothing where it does not link.
fn value<M>(held: Option<&HeldTodo>) -> Option<Node<M>> {
    match held.and_then(|h| h.row.value) {
        Some(Fulfillment::Number(v)) => Some(pie(v)),
        Some(Fulfillment::Absent(_)) => Some(chip("unpriced", "")),
        None => None,
    }
}

fn title(held: Option<&HeldTodo>, todo: &str) -> String {
    held.map_or_else(|| todo.to_owned(), |h| h.title.clone())
}

fn item<M>(i: &ReleaseItem) -> Node<M> {
    let mark = Mark::of(i);
    view! { Li {Class => mark.class(), Data("todo") => &i.todo} [
        value(i.held.as_ref()),
        view! { Span "releaseitemtitle" [title(i.held.as_ref(), &i.todo)] },
        (i.named == Named::Dropped).then(|| chip("moved out", "")),
    ] }
}

fn release<M>(r: &ReleaseRow) -> Node<M> {
    let class = match r.place {
        Place::Next => "release release-next",
        Place::Earlier | Place::Later => "release",
    };
    let mut items: Vec<&ReleaseItem> = r.items.iter().collect();
    items.sort_by_key(|i| Mark::of(i).rank());
    let items: Vec<Node<M>> = items.into_iter().map(item).collect();
    view! { Section {Class => class, Data("release") => &r.todo} [
        view! { H3 "releasetitle" [value(r.held.as_ref()), view! { Span [title(r.held.as_ref(), &r.todo)] }] },
        view! { P "settingsmeta" [format!("{} {}", standing(r), sentence(r))] },
        (!items.is_empty()).then(|| view! { Ul "releaseitems" [items] }),
    ] }
}

/// Why the box could not read them is shown, never hidden; nothing while they are on their way
/// or where the box serves no project chain.
pub(crate) fn view<M>(r: &Reading<ReleasesPayload>) -> Option<Node<M>> {
    match r {
        Reading::Loading => None,
        Reading::Error { e } => Some(
            view! { P "settingshealth settingshealth-failing" [format!("The releases could not be read: {e}")] },
        ),
        Reading::Val { v } if v.releases.is_empty() => None,
        Reading::Val { v } => {
            let rows: Vec<Node<M>> = v.releases.iter().map(release).collect();
            Some(view! { Div "releases" [
                view! { P "settingsmeta" [format!("The project chain reads {}, the last completed release.", v.reads)] },
                rows,
            ] })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The box's release sample (`app/core/tests/web/answers.json`): an item completed once stays
    /// done, a dropped one moves out, and what is owed comes first.
    #[test]
    fn the_box_sample_reads_as_the_box_reads_it() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../core/tests/web/answers.json"
        ))
        .expect("the core's answer samples");
        let samples: serde_json::Value = serde_json::from_str(&text).expect("the samples are JSON");
        let read: ReleasesPayload =
            serde_json::from_value(samples["ReleasesPayload"][0]["value"].clone())
                .expect("the sample reads");
        let [two, twoone] = &read.releases[..] else {
            panic!("two releases in the sample")
        };
        assert_eq!(sentence(two), "2 of 3 done, 1 cancelled, 1 moved out.");
        let mut items: Vec<&ReleaseItem> = two.items.iter().collect();
        items.sort_by_key(|i| Mark::of(i).rank());
        let order: Vec<&str> = items.iter().map(|i| i.todo.as_str()).collect();
        assert_eq!(
            order,
            [
                "portsuites",
                "calendarlinks",
                "wireonce",
                "instancesplit",
                "onboarding"
            ]
        );
        assert_eq!(standing(twoone), "Later.");
        assert!(twoone.held.is_none());
    }
}
