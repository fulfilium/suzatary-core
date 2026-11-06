//! The secretary's conversations as a list, and the search over what was said, which replaces
//! the list (or the stream) while words are typed.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use suzatary_core::secretary::search::{Hit, Side};
use suzatary_core::secretary::store::Kind;
use suzatary_core::wire::{ConversationRow, SearchPayload};
use ts_rs::TS;
use viewtree::Attr::{
    AriaCurrent, AriaLabel, AriaPressed, Class, Href, Placeholder, Role, Title, Type, Value,
};
use viewtree::kit::{Reading, para};
use viewtree::{Node, view};

use crate::chrome::{self, Shape, link};
use crate::places::Place;
use crate::runtime::{Effect, Local, Reach, read};
use crate::zone::{Offset, recent};

/// How long typing rests before the search is asked.
const DEBOUNCE_MS: u32 = 250;

const PAGE: usize = 30;

/// A topic opened with no name is named by the box after its first answer.
#[must_use]
pub(crate) fn named(title: &str) -> &str {
    if title.is_empty() { "New topic" } else { title }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Search {
    Off,
    /// The words trimmed, numbered by the typing that made them.
    On {
        q: String,
        n: u32,
        found: Reading<Vec<Hit>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "SearchAsk")]
pub enum Asking {
    Found,
    /// The rest after the typing numbered here.
    Rested(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Searching {
    typed: String,
    typings: u32,
    pub(crate) search: Search,
}

impl Default for Searching {
    fn default() -> Self {
        Searching {
            typed: String::new(),
            typings: 0,
            search: Search::Off,
        }
    }
}

impl Searching {
    /// The search goes off with no words, and is asked once the typing rests.
    pub(crate) fn typed<H, A, S>(
        &mut self,
        words: String,
        rested: impl Fn(Asking) -> A,
    ) -> Vec<Effect<H, A, S>> {
        let q = words.trim().to_owned();
        self.typed = words;
        self.typings += 1;
        match &self.search {
            _ if q.is_empty() => {
                self.search = Search::Off;
                Vec::new()
            }
            Search::On { q: was, .. } if *was == q => Vec::new(),
            Search::On { .. } | Search::Off => {
                self.search = Search::On {
                    q,
                    n: self.typings,
                    found: Reading::Loading,
                };
                vec![Effect::Timer {
                    ask: rested(Asking::Rested(self.typings)),
                    after: DEBOUNCE_MS,
                }]
            }
        }
    }

    /// Asked unless another typing came since.
    pub(crate) fn rested<H, A, S>(&self, n: u32, found: H) -> Vec<Effect<H, A, S>> {
        match &self.search {
            Search::On { q, n: m, .. } if *m == n => vec![Effect::Watch {
                ask: found,
                local: Local::Search { q: q.clone() },
            }],
            Search::On { .. } | Search::Off => Vec::new(),
        }
    }

    pub(crate) fn found(&mut self, r: Reading<crate::runtime::Answer>) {
        if let Search::On { found, .. } = &mut self.search {
            *found = read::<SearchPayload>(r).map(|p| p.hits);
        }
    }

    /// A page waits on this before it has anything to show.
    #[must_use]
    pub(crate) fn waits(&self) -> bool {
        matches!(
            self.search,
            Search::On {
                found: Reading::Loading,
                ..
            }
        )
    }
}

pub(crate) fn head<M: Clone + 'static>(
    s: &Searching,
    reach: &Reach,
    typed: fn(String) -> M,
) -> Node<M> {
    let writes = match reach {
        Reach::Writes => Some(link("act act-newtopic", Place::NewTopic, "New topic")),
        Reach::Waits => None,
        Reach::Reads { .. } => Some(readonly(reach)),
    };
    view! { Div "chathead" [
        view! { Input "search chatsearch" {Type => "search", Placeholder => "Search", AriaLabel => "Search conversations", Value => &s.typed} @input(typed) },
        view! { Div "chatwrites" [writes] },
    ] }
}

/// Drawn for a login that may not write, where a box to write in would be.
#[must_use]
pub(crate) fn readonly<M>(reach: &Reach) -> Node<M> {
    match reach {
        Reach::Reads { said, settings, .. } => view! { P "readonly" {Role => "status"} [
            said.as_str(),
            settings.then(|| vec![viewtree::node::text(" "), link("", Place::Settings, "Open Settings")]),
        ] },
        Reach::Writes | Reach::Waits => viewtree::node::text(""),
    }
}

fn card<M>(rows: Vec<Node<M>>) -> Node<M> {
    view! { Div "chatcard" [rows] }
}

#[must_use]
pub(crate) fn hits<M>(
    q: &str,
    found: &Reading<Vec<Hit>>,
    offsets: &[Offset],
    today: NaiveDate,
) -> Node<M> {
    match found {
        Reading::Loading => chrome::placeholder(Shape::Rows),
        Reading::Error { e } => para("problem chatsaid", format!("The search did not run: {e}")),
        Reading::Val { v } if v.is_empty() => chrome::empty(&format!("Nothing said matches “{q}”.")),
        Reading::Val { v } => card(
            v.iter()
                .map(|h| {
                    let who = match h.side {
                        Side::Person => "You",
                        Side::Answer => "Secretary",
                    };
                    view! { A "chatrow chathit" {Href => Place::Conversation { id: h.conversation.as_str(), reference: "", focus: false, at: u32::try_from(h.turn).ok() }.href()} [
                        view! { Span "chattitle" [named(&h.title)] },
                        view! { Span "chatwhen" [recent(offsets, &h.at, today)] },
                        view! { Span "chatlast" [format!("{who}: {}", h.excerpt)] },
                    ] }
                })
                .collect(),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filter {
    Topics,
    Reviews,
}

#[derive(Debug, Clone)]
pub(crate) struct List {
    pub(crate) rows: Reading<Vec<ConversationRow>>,
    pub(crate) filter: Filter,
    pub(crate) shown: usize,
}

impl Default for List {
    fn default() -> Self {
        List {
            rows: Reading::Loading,
            filter: Filter::Topics,
            shown: PAGE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ev {
    Filtered(Filter),
    More,
}

impl List {
    pub(crate) fn update(&mut self, ev: Ev) {
        match ev {
            Ev::Filtered(f) if f != self.filter => {
                self.filter = f;
                self.shown = PAGE;
            }
            Ev::Filtered(_) => {}
            Ev::More => self.shown += PAGE,
        }
    }
}

/// Listing opens nothing, so the default is pinned before it exists; following it opens it.
fn unopened() -> ConversationRow {
    ConversationRow {
        id: "default".to_owned(),
        title: "Secretary".to_owned(),
        kind: Kind::Default,
        opened: String::new(),
        turns: 0,
        standing: 0,
        unplaced: Vec::new(),
        last: String::new(),
        last_at: String::new(),
    }
}

fn row<M>(r: &ConversationRow, current: &str, offsets: &[Offset], today: NaiveDate) -> Node<M> {
    let on = r.id == current;
    let badges = (r.standing > 0 || !r.unplaced.is_empty()).then(|| {
        let n = r.unplaced.len();
        view! { Span "chatbadges" [
            (r.standing > 0).then(|| view! { Span "chatbadge" {Title => format!("{} waiting on you", r.standing)} [r.standing.to_string()] }),
            (n > 0).then(|| view! { Span "chatbadge problem" {Title => format!("{n} proposal{} with no card on this device", if n == 1 { "" } else { "s" })} [format!("{n}?")] }),
        ] }
    });
    view! { A {Class => if on { "chatrow current" } else { "chatrow" }, Href => Place::conversation(&r.id).href(), AriaCurrent => on.then_some("page")} [
        view! { Span "chattitle" [named(&r.title)] },
        badges,
        view! { Span "chatwhen" [recent(offsets, &r.last_at, today)] },
        (!r.last.is_empty()).then(|| view! { Span "chatlast" [r.last.as_str()] }),
    ] }
}

pub(crate) fn view<M: Clone>(
    l: &List,
    current: &str,
    offsets: &[Offset],
    today: NaiveDate,
    ev: fn(Ev) -> M,
) -> Node<M> {
    let all = match &l.rows {
        Reading::Loading => return chrome::placeholder(Shape::Rows),
        Reading::Error { e } => {
            return para(
                "problem chatsaid",
                format!("The conversations did not load: {e}"),
            );
        }
        Reading::Val { v } => v,
    };
    let pinned: Vec<ConversationRow> = {
        let d: Vec<ConversationRow> = all
            .iter()
            .filter(|r| r.kind == Kind::Default)
            .cloned()
            .collect();
        if d.is_empty() { vec![unopened()] } else { d }
    };
    let want = match l.filter {
        Filter::Topics => Kind::Topic,
        Filter::Reviews => Kind::Review,
    };
    let mut rows: Vec<&ConversationRow> = all.iter().filter(|r| r.kind == want).collect();
    // Instants and ids are ASCII, so their bytes order them.
    rows.sort_by(|a, b| b.last_at.cmp(&a.last_at).then_with(|| a.id.cmp(&b.id)));
    let reviews = all.iter().filter(|r| r.kind == Kind::Review).count();
    let toggle = |f: Filter, label: String| {
        let on = f == l.filter;
        view! { Button {Type => "button", Class => if on { "segment chatfilter on" } else { "segment chatfilter" }, AriaPressed => if on { "true" } else { "false" }} @click(ev(Ev::Filtered(f))) [label] }
    };
    let older = rows.len().saturating_sub(l.shown);
    view! { Div "chatsections" [
        card(pinned.iter().map(|r| row(r, current, offsets, today)).collect()),
        view! { Div "segmented chatfilters" {Role => "group", AriaLabel => "Which conversations"} [
            toggle(Filter::Topics, "Topics".to_owned()),
            toggle(Filter::Reviews, format!("Reviews · {reviews}")),
        ] },
        if rows.is_empty() {
            chrome::empty(match l.filter {
                Filter::Topics => "No topics yet.",
                Filter::Reviews => "No reviews yet.",
            })
        } else {
            card(rows.iter().take(l.shown).map(|r| row(r, current, offsets, today)).collect())
        },
        (older > 0).then(|| view! { Button "chatmore" {Type => "button"} @click(ev(Ev::More)) [format!("Show older · {older}")] }),
    ] }
}
