//! What stands around every page: Back, title, legend, status and body.

use viewtree::Attr::{AriaLabel, Class, Href, Role, Type};
use viewtree::kit::Reading;
use viewtree::node::Child;
use viewtree::{Node, view};

use crate::part::{Mark, Part, Tag, Who, part};
use crate::places::Place;
use crate::runtime::Behind;
use crate::zone::{Offset, when_ms};

#[must_use]
pub(crate) fn mark<M>(mark: Mark) -> Node<M> {
    part(format!("{mark:?}"), &Part::Mark { mark })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Lines,
    Rows,
    Cards,
    Turns,
}

#[derive(Debug, Clone)]
pub(crate) enum Status<M> {
    Waiting,
    Failed {
        e: String,
        retry: M,
    },
    Behind {
        since: String,
        /// `None` while an ask is on its way.
        failed: Option<(String, M)>,
    },
    Quiet,
}

pub(crate) fn status<T, M>(
    r: &Reading<T>,
    behind: Option<&Behind>,
    offsets: &[Offset],
    retry: M,
) -> Status<M> {
    match (r, behind) {
        (Reading::Loading, _) => Status::Waiting,
        (Reading::Error { e }, _) => Status::Failed {
            e: e.clone(),
            retry,
        },
        (Reading::Val { .. }, None) => Status::Quiet,
        (Reading::Val { .. }, Some(b)) => Status::Behind {
            since: when_ms(offsets, b.since),
            failed: b.why.clone().map(|why| (why, retry)),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Masthead {
    Page,
    /// A destination of the bar's (the briefings), drawn larger.
    Destination,
}

/// How wide a page stands: in the reading column, or across the screen (a list with its pane).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Width {
    Column,
    Wide,
}

#[derive(Debug)]
pub(crate) struct Chrome<M> {
    /// Beside `column`.
    pub(crate) class: &'static str,
    pub(crate) width: Width,
    pub(crate) title: String,
    pub(crate) masthead: Masthead,
    pub(crate) back: Option<(&'static str, M)>,
    /// `""` hides it until there is one.
    pub(crate) legend: Option<String>,
    /// What stands above the status whatever the page read (a search field).
    pub(crate) now: Vec<Node<M>>,
    pub(crate) shape: Shape,
}

pub(crate) fn retry<M>(m: M) -> Node<M> {
    view! { Button "retry" {Type => "button"} @click(m) ["Retry"] }
}

#[must_use]
pub(crate) fn placeholder<M>(shape: Shape) -> Node<M> {
    let bars = |n| (0..n).map(|_| view! { Span "phbar" }).collect::<Vec<_>>();
    let each = |n, class: &'static str, inner| {
        (0..n)
            .map(|_| view! { Span {Class => class} [bars(inner)] })
            .collect::<Vec<_>>()
    };
    let (class, parts) = match shape {
        Shape::Lines => ("placeholder ph-lines", bars(5)),
        Shape::Rows => ("placeholder ph-rows", each(5, "phrow", 2)),
        Shape::Cards => ("placeholder ph-cards", each(3, "phcard", 3)),
        Shape::Turns => ("placeholder ph-turns", each(4, "phturn", 2)),
    };
    view! { Div {Class => class, Role => "status", AriaLabel => "Loading"} [mark(crate::part::Mark::Loadmark), parts] }
}

#[must_use]
pub(crate) fn page<M: Clone>(c: Chrome<M>, status: Status<M>, body: Vec<Node<M>>) -> Node<M> {
    let title = view! { H1 {Class => match c.masthead {
        Masthead::Page => "pagetitle",
        Masthead::Destination => "pagetitle destination",
    }} [c.title.clone()] };
    page_headed(c, title, status, body)
}

/// `page`, its title drawn as `head` (a heading that becomes a field).
#[must_use]
pub(crate) fn page_headed<M: Clone>(
    c: Chrome<M>,
    head: Node<M>,
    status: Status<M>,
    body: Vec<Node<M>>,
) -> Node<M> {
    let waiting = matches!(status, Status::Waiting);
    let said = match status {
        Status::Waiting => Some(view! { Div "pagewait" [placeholder(c.shape)] }),
        Status::Failed { e, retry: m } => {
            Some(view! { Div "problem" [format!("Could not load: {e} "), retry(m)] })
        }
        Status::Behind {
            since,
            failed: None,
        } => Some(view! { Div "legend stale" [format!("As of {since}; asking the box again…")] }),
        Status::Behind {
            since,
            failed: Some((why, m)),
        } => Some(view! { Div "legend stale" [
            format!("Could not refresh: {why}. This is what the tab last read, at {since}. "),
            retry(m),
        ] }),
        Status::Quiet => None,
    };
    let column = format!(
        "{}{}{}",
        match c.width {
            Width::Column => "column ",
            Width::Wide => "",
        },
        c.class,
        if waiting { " pagewaiting" } else { "" }
    );
    let back = c.back.map(
        |(label, m)| view! { Button "back" {Type => "button"} @click(m) [mark(crate::part::Mark::Chevron), label] },
    );
    let legend = c
        .legend
        .map(|l| view! { P "legend" {viewtree::Attr::Hidden => l.is_empty()} [l] });
    // Keyed, so the status line coming and going moves the body and never draws it again.
    let parts = [
        back.map(|n| ("back", n)),
        Some((
            "head",
            view! { Div "pagehead" [head, mark(crate::part::Mark::Rule)] },
        )),
        legend.map(|n| ("legend", n)),
        Some(("now", view! { Div "pagenow" [c.now] })),
        said.map(|n| ("status", n)),
        Some(("body", view! { Div "pagebody" [body] })),
    ];
    view! { Div {Class => column} => parts.into_iter().flatten().map(|(k, n)| (k.to_owned(), n)) }
}

/// `kind` is one of the modifiers `style.css` gives `.chip` (`""`, `ok`, `cat`, `kind`).
#[must_use]
pub(crate) fn chip<M>(label: &str, kind: &str) -> Node<M> {
    let class = if kind.is_empty() {
        "chip".to_owned()
    } else {
        format!("chip {kind}")
    };
    view! { Span {Class => class} [label] }
}

/// A fraction as its whole percent, `0.129` as `13%`.
#[must_use]
pub(crate) fn percent(share: f64) -> String {
    format!("{}%", (share * 100.0).round())
}

/// A fraction to a tenth of a percent, `62.3%` (`site::pct`).
#[must_use]
pub(crate) fn pct(share: f64) -> String {
    format!("{:.1}%", share * 100.0)
}

/// A fraction in [0, 1] on the fulfillment scale.
#[must_use]
pub(crate) fn pie<M>(share: f64) -> Node<M> {
    part("pie", &Part::Pie { share })
}

#[must_use]
pub(crate) fn rich<M>(html: Option<&str>, text: &str, class: &'static str) -> Node<M> {
    let p = Part::Rich {
        html: html.map(str::to_owned),
        text: text.to_owned(),
        class: class.to_owned(),
        kind: None,
    };
    part(class, &p)
}

/// One block of an answer, as `rich` draws it, marked with its kind.
#[must_use]
pub(crate) fn rich_block<M>(
    html: Option<&str>,
    text: &str,
    kind: suzatary_core::answer::Kind,
) -> Node<M> {
    let p = Part::Rich {
        html: html.map(str::to_owned),
        text: text.to_owned(),
        class: "answerblock".to_owned(),
        kind: Some(kind),
    };
    part("answerblock", &p)
}

#[must_use]
pub(crate) fn link<M>(class: &str, to: Place<'_>, words: impl Child<M>) -> Node<M> {
    view! { A {Class => class, Href => to.href()} [words] }
}

/// A link, so it middle-clicks and Back returns here.
#[must_use]
pub(crate) fn ask<M>(reference: &str) -> Node<M> {
    view! { Div "askrow" [link("asksecretary", Place::Ask(reference), "Ask the secretary")] }
}

#[must_use]
pub(crate) fn empty<M>(said: &str) -> Node<M> {
    view! { P "empty" [said] }
}

#[must_use]
pub(crate) fn callout<M>(title: &str, body: impl Child<M>) -> Node<M> {
    view! { Section "callout" [mark(crate::part::Mark::Outline), view! { H2 "callouttitle" [title] }, body] }
}

#[must_use]
pub(crate) fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The mark beside a control waiting on the box, shown at once, since the person pressed it.
#[must_use]
pub(crate) fn inline_wait<M>() -> Node<M> {
    view! { Span "placeholder phinline" {Role => "status", AriaLabel => "Waiting"} [mark(crate::part::Mark::Loadmark)] }
}

/// Whether a held value's placeholder in a text becomes the chip a tap reveals, and where from.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Chips<'a> {
    None,
    Mail,
    Conversation(&'a str),
}

#[must_use]
pub(crate) fn prose<M>(
    tag: Tag,
    class: &str,
    text: &str,
    who: Option<Who>,
    chips: Chips<'_>,
) -> Node<M> {
    let (held, conversation) = match chips {
        Chips::None => (false, None),
        Chips::Mail => (true, None),
        Chips::Conversation(c) => (true, Some(c.to_owned())),
    };
    let p = Part::Prose {
        tag,
        class: class.to_owned(),
        text: text.to_owned(),
        who,
        held,
        conversation,
    };
    part(class, &p)
}

/// Counted by the host, so a clock asks no step of the page.
#[must_use]
pub(crate) fn clock<M>(key: &str, lead: &str) -> Node<M> {
    part(
        key,
        &Part::Elapsed {
            lead: lead.to_owned(),
        },
    )
}
