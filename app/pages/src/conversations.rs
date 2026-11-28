//! `#/conversations`: the secretary's conversations as a list, with the search over what was
//! said. A conversation opened from it stands beside it where the room holds both, and the list
//! marks the conversation the address has open.

use serde::{Deserialize, Serialize};
use suzatary_topics::wire::topics::ConversationsPayload;
use ts_rs::TS;
use viewtree::kit::Reading;
use viewtree::{Node, Program, view};

use crate::chats::{self, Search, Searching};
use crate::chrome::{self, Chrome, Masthead, Shape, Status, link};
use crate::needsyou::Route;
use crate::places::Place;
use crate::runtime::{self, Context, Effect, Listening, Local, Never, Slot, read, watch};
use crate::zone;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ConversationsFlags")]
pub struct Flags {
    pub context: Context,
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub now: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ConversationsHold")]
pub enum Hold {
    Chats,
    Found,
    /// The conversation the address has open, which the list marks.
    Route,
}

pub type Ask = chats::Asking;
pub type Input = runtime::Input<Hold, Ask>;

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    now: i64,
    chats: chats::List,
    search: Searching,
    /// The conversation the address has open.
    current: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Chats(chats::Ev),
    Searched(String),
}

#[derive(Debug)]
pub enum Conversations {}

impl Program for Conversations {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Ask>;
    type Flags = Flags;
    type Slot = Slot<Hold, Never>;
    type Sub = runtime::Sub;

    fn init(f: Flags) -> (Model, Vec<Self::Effect>) {
        let m = Model {
            context: f.context,
            now: f.now,
            chats: chats::List::default(),
            search: Searching::default(),
            current: None,
        };
        (m, Vec::new())
    }

    fn subscriptions(m: &Model) -> Listening<Hold, Never> {
        [
            Some(watch(Hold::Chats, Local::Conversations)),
            Some(watch(Hold::Route, Local::Route)),
            m.search.listening(Hold::Found),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Self::Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.context = c,
                Input::Held {
                    ask: Hold::Chats,
                    reading,
                    ..
                } => {
                    m.chats.rows = read::<ConversationsPayload>(reading).map(|p| p.conversations);
                }
                Input::Held {
                    ask: Hold::Found,
                    reading,
                    ..
                } => m.search.found(reading),
                Input::Held {
                    ask: Hold::Route,
                    reading,
                    ..
                } => {
                    if let Reading::Val { v } = read::<Route>(reading) {
                        m.current = v.conversation;
                    }
                }
                Input::Answered {
                    ask: chats::Asking::Rested(n),
                    ..
                } => m.search.rested(n),
                Input::Heard { ask, .. } | Input::Opened { ask } | Input::Lost { ask } => {
                    match ask {}
                }
            },
            Msg::Chats(ev) => m.chats.update(ev),
            Msg::Searched(words) => return m.search.typed(words, |a| a),
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let today = zone::day_ms(&m.context.offsets, m.now);
        let c = Chrome {
            class: "chats conversations",
            width: chrome::Width::Column,
            title: "Conversations".to_owned(),
            masthead: Masthead::Page,
            back: None,
            legend: None,
            now: vec![
                view! { P "legend chatsmemory" [link("", Place::Memory, "What the secretary remembers")] },
                chats::head(&m.search, &m.context.reach, Msg::Searched),
            ],
            shape: Shape::Rows,
        };
        let status = if m.search.waits() {
            Status::Waiting
        } else {
            Status::Quiet
        };
        let body = match &m.search.search {
            Search::On { q, found, .. } => {
                chats::hits(q, found.reading(), &m.context.offsets, today)
            }
            Search::Off => chats::view(
                &m.chats,
                m.current.as_deref(),
                &m.context.offsets,
                today,
                Msg::Chats,
            ),
        };
        chrome::page(c, status, vec![view! { Div "chatbody" [body] }])
    }
}
