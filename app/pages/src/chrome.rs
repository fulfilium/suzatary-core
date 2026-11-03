//! What stands around every page, drawn the way `web/src/view/frame.ts` draws it for the pages still
//! in TypeScript, so a page reads the same in either: a Back button, the title on its sheet, the
//! legend, the line of what the page holds now, its status (waiting, failed, maybe behind), and
//! its body. The marks drawn in SVG (the rule, the chevron, the loading mark) are the host's parts.
//! When the shell is a program (plan §7 stage 9), `frame.ts` goes and this is the only one.

use serde::Serialize;
use viewtree::Attr::{AriaLabel, Class, Href, Role, Type};
use viewtree::kit::Reading;
use viewtree::node::Child;
use viewtree::node::foreign;
use viewtree::{Node, view};

use crate::places::Place;
use crate::runtime::Behind;
use crate::zone::{Offset, when_ms};

/// A mark the host draws in SVG: `rule`, `chevron`, `outline` or `loadmark`
/// (`web/src/pages/parts.ts`).
#[must_use]
pub fn mark<M>(name: &'static str) -> Node<M> {
    foreign("mark", name, "")
}

/// The shape of what a page draws, for the placeholder shown while it waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Lines of text.
    Lines,
    /// Rows of a list.
    Rows,
}

/// What a page says above its body about what it read.
#[derive(Debug, Clone)]
pub enum Status<M> {
    /// It waits on its first answer: the placeholder.
    Waiting,
    /// Its first answer failed: why, and a Retry.
    Failed {
        /// Why.
        e: String,
        /// What Retry sends.
        retry: M,
    },
    /// What it shows may be behind the box.
    Behind {
        /// Since when, on the owner's clock.
        since: String,
        /// Why the last ask failed and what Retry sends, or `None` while one is on its way.
        failed: Option<(String, M)>,
    },
    /// Nothing to say.
    Quiet,
}

/// What a held reading says, frame's way: the first answer awaited or failed, or one shown and
/// maybe behind.
pub fn status<T, M>(
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

/// How a page's title stands: a page's own, or a destination's, drawn larger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Masthead {
    /// A page's title.
    Page,
    /// A destination of the bar's (the briefings).
    Destination,
}

/// A page's parts other than its body.
#[derive(Debug)]
pub struct Chrome<M> {
    /// The page's class, beside `column`.
    pub class: &'static str,
    /// Its title.
    pub title: String,
    /// How the title stands.
    pub masthead: Masthead,
    /// The Back button's label and what it sends, for a page under another.
    pub back: Option<(&'static str, M)>,
    /// The line under the title, for a page that has one (`""` hides it until there is one).
    pub legend: Option<String>,
    /// What stands above the status whatever the page read (a search field).
    pub now: Vec<Node<M>>,
    /// The placeholder's shape.
    pub shape: Shape,
}

fn retry<M>(m: M) -> Node<M> {
    view! { Button "retry" {Type => "button"} @click(m) ["Retry"] }
}

fn placeholder<M>(shape: Shape) -> Node<M> {
    let bars = |n| (0..n).map(|_| view! { Span "phbar" }).collect::<Vec<_>>();
    let (class, parts) = match shape {
        Shape::Lines => ("placeholder ph-lines", bars(5)),
        Shape::Rows => (
            "placeholder ph-rows",
            (0..5).map(|_| view! { Span "phrow" [bars(2)] }).collect(),
        ),
    };
    view! { Div {Class => class, Role => "status", AriaLabel => "Loading"} [mark("loadmark"), parts] }
}

/// The page: its chrome, its status, and its body.
#[must_use]
pub fn page<M: Clone>(c: Chrome<M>, status: Status<M>, body: Vec<Node<M>>) -> Node<M> {
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
        Some((
            "head",
            view! { Div "pagehead" [view! { H1 {Class => match c.masthead {
                Masthead::Page => "pagetitle",
                Masthead::Destination => "pagetitle destination",
            }} [c.title] }, mark("rule")] },
        )),
        legend.map(|n| ("legend", n)),
        Some(("now", view! { Div "pagenow" [c.now] })),
        said.map(|n| ("status", n)),
        Some(("body", view! { Div "pagebody" [body] })),
    ];
    view! { Div {Class => column} => parts.into_iter().flatten().map(|(k, n)| (k.to_owned(), n)) }
}

/// A chip: `.chip` and one of the modifiers `style.css` gives it (`""`, `ok`, `cat`, `kind`).
#[must_use]
pub fn chip<M>(label: &str, kind: &str) -> Node<M> {
    let class = if kind.is_empty() {
        "chip".to_owned()
    } else {
        format!("chip {kind}")
    };
    view! { Span {Class => class} [label] }
}

/// A fraction in [0, 1] as the app's pie, on the fulfillment scale.
#[must_use]
pub fn pie<M>(share: f64) -> Node<M> {
    foreign("mark", "pie", share.to_string())
}

/// What a `rich` part is drawn from.
#[derive(Serialize)]
struct Rich<'a> {
    html: Option<&'a str>,
    text: &'a str,
    class: &'a str,
}

/// Typst's HTML placed through the tripwire (`web/src/rich.ts`), or its source as text where
/// nothing rendered it.
#[must_use]
pub fn rich<M>(html: Option<&str>, text: &str, class: &'static str) -> Node<M> {
    let props = serde_json::to_string(&Rich { html, text, class }).expect("strings print as JSON");
    foreign("rich", class, props)
}

/// A link of a class to a place in the app.
#[must_use]
pub fn link<M>(class: &str, to: Place<'_>, words: impl Child<M>) -> Node<M> {
    view! { A {Class => class, Href => to.href()} [words] }
}

/// Ask the secretary about a reference: a link, so it middle-clicks and Back returns here.
#[must_use]
pub fn ask<M>(reference: &str) -> Node<M> {
    view! { Div "askrow" [link("asksecretary", Place::Ask(reference), "Ask the secretary")] }
}

/// What a list says when it holds nothing.
#[must_use]
pub fn empty<M>(said: &str) -> Node<M> {
    view! { P "empty" [said] }
}

/// A callout: a title and its body inside the outline.
#[must_use]
pub fn callout<M>(title: &str, body: impl Child<M>) -> Node<M> {
    view! { Section "callout" [mark("outline"), view! { H2 "callouttitle" [title] }, body] }
}

/// `1 fact` or `3 facts`.
#[must_use]
pub fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
