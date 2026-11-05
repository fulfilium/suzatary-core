//! What stands around every page (Back, title, legend, status, body), drawn as
//! `web/src/view/frame.ts` draws it for the pages still in TypeScript, so both read the same.

use serde::Serialize;
use viewtree::Attr::{AriaLabel, Class, Href, Role, Type};
use viewtree::kit::Reading;
use viewtree::node::Child;
use viewtree::node::foreign;
use viewtree::{Node, view};

use crate::places::Place;
use crate::runtime::Behind;
use crate::zone::{Offset, when_ms};

/// A mark the host draws in SVG (`web/src/pages/parts.ts`): `rule`, `chevron`, `outline`,
/// `loadmark`, `diamond`, or an icon (`send`, `clip`, `cross`, `plus`, `doc`, `lock`).
#[must_use]
pub(crate) fn mark<M>(name: &'static str) -> Node<M> {
    foreign("mark", name, "")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
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
pub enum Masthead {
    Page,
    /// A destination of the bar's (the briefings), drawn larger.
    Destination,
}

#[derive(Debug)]
pub(crate) struct Chrome<M> {
    /// Beside `column`.
    pub(crate) class: &'static str,
    pub(crate) title: String,
    pub(crate) masthead: Masthead,
    pub(crate) back: Option<(&'static str, M)>,
    /// `""` hides it until there is one.
    pub(crate) legend: Option<String>,
    /// What stands above the status whatever the page read (a search field).
    pub(crate) now: Vec<Node<M>>,
    pub(crate) shape: Shape,
}

fn retry<M>(m: M) -> Node<M> {
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
    view! { Div {Class => class, Role => "status", AriaLabel => "Loading"} [mark("loadmark"), parts] }
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
        "column {}{}",
        c.class,
        if waiting { " pagewaiting" } else { "" }
    );
    let back = c.back.map(
        |(label, m)| view! { Button "back" {Type => "button"} @click(m) [mark("chevron"), label] },
    );
    let legend = c
        .legend
        .map(|l| view! { P "legend" {viewtree::Attr::Hidden => l.is_empty()} [l] });
    // Keyed, so the status line coming and going moves the body and never draws it again.
    let parts = [
        back.map(|n| ("back", n)),
        Some(("head", view! { Div "pagehead" [head, mark("rule")] })),
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

/// A fraction in [0, 1] on the fulfillment scale.
#[must_use]
pub(crate) fn pie<M>(share: f64) -> Node<M> {
    foreign("mark", "pie", share.to_string())
}

#[derive(Serialize)]
struct Rich<'a> {
    html: Option<&'a str>,
    text: &'a str,
    class: &'a str,
    kind: Option<suzatary_core::secretary::answer::Kind>,
}

/// Typst's HTML placed through the tripwire (`web/src/rich.ts`), or its source as text where
/// nothing rendered it.
#[must_use]
pub(crate) fn rich<M>(html: Option<&str>, text: &str, class: &'static str) -> Node<M> {
    let props = serde_json::to_string(&Rich {
        html,
        text,
        class,
        kind: None,
    })
    .expect("strings print as JSON");
    foreign("rich", class, props)
}

/// One block of an answer, as `rich` draws it, marked with its kind.
#[must_use]
pub(crate) fn rich_block<M>(
    html: Option<&str>,
    text: &str,
    kind: suzatary_core::secretary::answer::Kind,
) -> Node<M> {
    let props = serde_json::to_string(&Rich {
        html,
        text,
        class: "answerblock",
        kind: Some(kind),
    })
    .expect("strings print as JSON");
    foreign("rich", "answerblock", props)
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
    view! { Section "callout" [mark("outline"), view! { H2 "callouttitle" [title] }, body] }
}

#[must_use]
pub(crate) fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The mark alone beside a control waiting on the box, shown at once: the person pressed it, so
/// the wait is theirs.
#[must_use]
pub(crate) fn inline_wait<M>() -> Node<M> {
    view! { Span "placeholder phinline" {Role => "status", AriaLabel => "Waiting"} [mark("loadmark")] }
}

/// Whose words a text is decides what is linked: references in anyone's, URLs only in the box's
/// own and an act's (`web/src/chat/refs.ts`'s `textWithReferences`).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Words<'a> {
    Plain,
    /// Said by `who` (a turn's speaker).
    Said(&'a str),
}

/// Whether a held value's placeholder in a text becomes the chip a tap reveals, and where from.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Chips<'a> {
    None,
    Mail,
    Conversation(&'a str),
}

#[derive(Serialize)]
struct Prose<'a> {
    tag: &'a str,
    class: &'a str,
    text: &'a str,
    who: Option<&'a str>,
    held: bool,
    conversation: Option<&'a str>,
}

/// A stranger's or a model's text, drawn by the host as text nodes and built links only, never
/// parsed as markup (`web/src/pages/parts.ts`'s `prose`).
#[must_use]
pub(crate) fn prose<M>(
    tag: &str,
    class: &str,
    text: &str,
    words: Words<'_>,
    chips: Chips<'_>,
) -> Node<M> {
    let (held, conversation) = match chips {
        Chips::None => (false, None),
        Chips::Mail => (true, None),
        Chips::Conversation(c) => (true, Some(c)),
    };
    let who = match words {
        Words::Plain => None,
        Words::Said(who) => Some(who),
    };
    let props = serde_json::to_string(&Prose {
        tag,
        class,
        text,
        who,
        held,
        conversation,
    })
    .expect("strings print as JSON");
    foreign("prose", class.to_owned(), props)
}

/// `lead` and the whole seconds since the clock of `key` first stood, counted by the host
/// (`web/src/pages/parts.ts`'s `elapsed`), so a clock asks no step of the page.
#[must_use]
pub(crate) fn clock<M>(key: &str, lead: &str) -> Node<M> {
    foreign("elapsed", key.to_owned(), lead.to_owned())
}
