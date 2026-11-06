//! Mail by date or any notmuch query, and the owner's watches on mail. Triage's reading is a
//! badge, never a filter, so what it dismissed stays visible. Every string from mail is text.

use serde::{Deserialize, Serialize};
use suzatary_core::mail::MailRow;
use suzatary_core::wire::{InboxPayload, MailWatch, WatchOn, WatchesPayload};
use ts_rs::TS;
use viewtree::Attr::{
    AriaCurrent, AriaLabel, Class, Data, Disabled, EnterKeyHint, Hidden, Href, Placeholder, Title,
    Type, Value,
};
use viewtree::kit::{Reading, para};
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, mark};
use crate::mailrow::{account, arrived, decided, folder, held, sender, subject, todo};
use crate::places::{PAGE, Place, Question};
use crate::runtime::{Body, Context, Effect, Gate, Held, Input, answered, read};
use crate::zone::when;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "MailFlags")]
pub struct Flags {
    pub context: Context,
    /// `""` for the inbox.
    pub query: String,
    /// `""` for every account.
    pub account: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "MailHold")]
pub enum Hold {
    First,
    Watches,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "MailAsk")]
pub enum Ask {
    More,
    Cancel { id: String },
}

/// `Ended`: the last page came back short.
#[derive(Debug, Clone, PartialEq, Eq)]
enum More {
    Open,
    Asking,
    Failed(String),
    Ended,
}

impl More {
    fn after(n: usize) -> More {
        if n < PAGE { More::Ended } else { More::Open }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    flags: Flags,
    typed: String,
    /// A search answers the inbox's fields, so both read as the inbox's.
    first: Held<InboxPayload>,
    watches: Held<Vec<MailWatch>>,
    after: Vec<MailRow>,
    more: More,
    cancelling: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Ask>>),
    Typed(String),
    Search,
    More,
    Cancel(String),
    Retry,
}

#[derive(Debug)]
pub enum Mail {}

fn first(f: &Flags) -> String {
    question(f, "").path()
}

fn question<'a>(f: &'a Flags, before: &'a str) -> Question<'a> {
    if f.query.is_empty() {
        Question::Inbox {
            account: &f.account,
            before,
        }
    } else {
        Question::Search {
            q: &f.query,
            account: &f.account,
            before,
        }
    }
}

impl Model {
    fn shown(&self) -> impl Iterator<Item = &MailRow> {
        let first = match &self.first.reading {
            Reading::Val { v } => v.messages.as_slice(),
            Reading::Loading | Reading::Error { .. } => &[],
        };
        first.iter().chain(&self.after)
    }
}

impl Program for Mail {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Hold, Ask>;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect<Hold, Ask>>) {
        let effects = vec![
            Effect::Hold {
                ask: Hold::First,
                path: first(&flags),
            },
            Effect::Hold {
                ask: Hold::Watches,
                path: Question::Watches.path(),
            },
        ];
        let model = Model {
            typed: flags.query.clone(),
            flags,
            first: Held::loading(),
            watches: Held::loading(),
            after: Vec::new(),
            more: More::Open,
            cancelling: Vec::new(),
        };
        (model, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect<Hold, Ask>> {
        match msg {
            Msg::Host(input) => answer(m, *input),
            Msg::Typed(words) => {
                m.typed = words;
                Vec::new()
            }
            Msg::Search => vec![Effect::Go {
                href: Place::Mail {
                    query: m.typed.trim(),
                    account: &m.flags.account,
                }
                .href(),
            }],
            Msg::More if matches!(m.more, More::Asking | More::Ended) => Vec::new(),
            Msg::More => {
                m.more = More::Asking;
                let before = m
                    .shown()
                    .last()
                    .map(|r| r.date)
                    .map_or_else(String::new, |d| {
                        d.map_or_else(String::new, suzatary_core::capability::Datetime::isoformat)
                    });
                vec![Effect::Get {
                    ask: Ask::More,
                    path: question(&m.flags, &before).path(),
                }]
            }
            Msg::Cancel(id) if m.cancelling.contains(&id) => Vec::new(),
            Msg::Cancel(id) => {
                m.cancelling.push(id.clone());
                vec![Effect::Post {
                    path: Question::Cancel(&id).path(),
                    ask: Ask::Cancel { id },
                    body: Body::Empty {},
                }]
            }
            Msg::Retry => vec![
                Effect::Again {
                    path: first(&m.flags),
                },
                Effect::Again {
                    path: Question::Watches.path(),
                },
            ],
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let searching = !m.flags.query.is_empty();
        let legend = if searching {
            format!("notmuch query: {}", m.flags.query)
        } else {
            "The inbox by date, across every account — whatever triage made of it.".to_owned()
        };
        let search = view! { Div "searchbox" [view! { Form @submit(Msg::Search) [
            view! { Input "search" {Type => "search", Value => &m.typed, Placeholder => "notmuch query — from:, subject:, date:", AriaLabel => "search the mail with a notmuch query", EnterKeyHint => "search"} @input(Msg::Typed) },
        ] }] };
        let chrome = Chrome {
            class: "mailpage",
            title: if searching { "Mail search" } else { "Mail" }.to_owned(),
            masthead: Masthead::Page,
            width: crate::chrome::Width::Column,
            back: None,
            legend: None,
            now: vec![para("legend", legend), search],
            shape: Shape::Rows,
        };
        let both = match (&m.first.reading, &m.watches.reading) {
            (Reading::Error { e }, _) | (_, Reading::Error { e }) => {
                Reading::Error { e: e.clone() }
            }
            (Reading::Loading, _) | (_, Reading::Loading) => Reading::Loading,
            (Reading::Val { v }, Reading::Val { v: ws }) => Reading::Val {
                v: (v, ws.as_slice()),
            },
        };
        let behind = m.first.behind.as_ref().or(m.watches.behind.as_ref());
        let status = chrome::status(&both, behind, &m.flags.context.offsets, Msg::Retry);
        let body = match &both {
            Reading::Val { v: (first, ws) } => listing(m, first, ws),
            Reading::Loading | Reading::Error { .. } => Vec::new(),
        };
        chrome::page(chrome, status, body)
    }
}

fn answer(m: &mut Model, input: Input<Hold, Ask>) -> Vec<Effect<Hold, Ask>> {
    match input {
        Input::Context(c) => m.flags.context = c,
        Input::Held {
            ask: Hold::First,
            reading,
            behind,
        } => {
            let first = Held::<InboxPayload>::of(reading, behind);
            if let Reading::Val { v } = &first.reading {
                // The pages after were read after another first page, so they go.
                m.after.clear();
                m.more = More::after(v.messages.len());
            }
            m.first = first;
        }
        Input::Held {
            ask: Hold::Watches,
            reading,
            behind,
        } => {
            m.watches = Held {
                reading: read::<WatchesPayload>(reading).map(|p| p.watches),
                behind,
            };
        }
        Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
        Input::Answered {
            ask: Ask::More,
            answer,
        } => match answered::<InboxPayload>(answer) {
            Ok(page) => {
                m.more = More::after(page.messages.len());
                m.after.extend(page.messages);
                if let Reading::Val { v } = &mut m.first.reading {
                    v.accounts = page.accounts;
                }
            }
            Err(e) => m.more = More::Failed(e),
        },
        Input::Answered {
            ask: Ask::Cancel { id },
            answer,
        } => {
            m.cancelling.retain(|c| *c != id);
            match answered::<WatchesPayload>(answer) {
                Ok(p) => m.watches.reading = Reading::Val { v: p.watches },
                Err(words) => return vec![Effect::Toast { words }],
            }
        }
    }
    Vec::new()
}

fn what(w: &MailWatch) -> String {
    match w.on {
        WatchOn::Sender => format!("mail from “{}”", w.text),
        WatchOn::Subject => format!("a subject with “{}”", w.text),
    }
}

fn watch(m: &Model, w: &MailWatch) -> Node<Msg> {
    let cancel = match m.flags.context.gate {
        Gate::Writes => Some(
            view! { Button "act act-cancel" {Type => "button", AriaLabel => format!("Stop watching for {}", what(w)), Disabled => m.cancelling.contains(&w.id)} @click(Msg::Cancel(w.id.clone())) ["Cancel"] },
        ),
        Gate::Reads | Gate::Views => None,
    };
    let until = when(&m.flags.context.offsets, &w.until);
    view! { Li "watchrow" {Data("watch") => &w.id} [
        view! { Span "watchwhat" [format!("Watching for {}, until {until}", what(w))] },
        cancel,
    ] }
}

// An honest `<a>`: unlike a card, a row holds no buttons.
fn row(m: &Model, r: &MailRow) -> Node<Msg> {
    view! { A "mailrow" {Href => Place::Message(&r.id).href()} [
        view! { Div "mailmeta" [
            view! { Span "mailsender" {Title => &r.sender} [sender(r)] },
            view! { Span "mailwhen" [arrived(&m.flags.context.offsets, r)] },
        ] },
        view! { Div "mailsubject" [subject(r)] },
        view! { Div "mailchips" [account(r), (r.container != "inbox").then(|| folder(r)), decided(r), todo(r), held(r)] },
    ] }
}

// From the box's roster, not the rows received: a filter offering only what is shown cannot find
// what is not.
fn accounts(m: &Model, accounts: &[String]) -> Node<Msg> {
    let link = |value: &str, label: &str| {
        let on = value == m.flags.account;
        let href = Place::Mail {
            query: &m.flags.query,
            account: value,
        }
        .href();
        let class = if on { "chipbtn on" } else { "chipbtn" };
        view! { A {Class => class, Href => href, AriaCurrent => on.then_some("true")} [label] }
    };
    let links: Vec<_> = accounts.iter().map(|a| link(a, a)).collect();
    view! { Div "facets" [view! { Div "chiprow" [link("", "All"), links] }] }
}

fn listing(m: &Model, first: &InboxPayload, watches: &[MailWatch]) -> Vec<Node<Msg>> {
    let shown: Vec<&MailRow> = m.shown().collect();
    let what = if m.flags.query.is_empty() {
        "in the inbox"
    } else {
        "matching"
    };
    let noun = if shown.len() == 1 {
        "message"
    } else {
        "messages"
    };
    let count = view! { P "count" [
        view! { Strong [shown.len().to_string()] },
        format!(" {noun} {what}"),
        view! { Span "countnote" ["newest first; the badge is what triage made of it"] },
    ] };
    let list = if shown.is_empty() {
        view! { Div "maillist" [para("empty", if m.flags.query.is_empty() {
            "Nothing in the inbox for this account."
        } else {
            "No message in the local corpus matches that query."
        })] }
    } else {
        view! { Div "maillist" => shown.iter().map(|r| (format!("{}/{}", r.id, r.container), row(m, r))) }
    };
    let failed = match &m.more {
        More::Failed(e) => Some(para("problem", e.as_str())),
        More::Open | More::Asking | More::Ended => None,
    };
    let foot = view! { Div "mailfoot" {Hidden => shown.is_empty() || m.more == More::Ended} [
        view! { Button "morebtn" {Type => "button", Disabled => m.more == More::Asking} @click(Msg::More) ["Load more", mark("chevron")] },
        failed,
    ] };
    let watches = (!watches.is_empty()).then(|| {
        view! { Ul "watchlist" {AriaLabel => "Watches on expected mail"} => watches.iter().map(|w| (w.id.clone(), watch(m, w))) }
    });
    vec![
        watches,
        Some(accounts(m, &first.accounts)),
        Some(count),
        Some(list),
        Some(foot),
    ]
    .into_iter()
    .flatten()
    .collect()
}
