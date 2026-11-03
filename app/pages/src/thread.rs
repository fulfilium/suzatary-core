//! One mail thread, oldest first, with only the newest open: read the way it happened. Bodies are
//! text.

use serde::{Deserialize, Serialize};
use suzatary_core::wire::{MailPage, ThreadPayload};
use ts_rs::TS;
use viewtree::Attr::{Class, Href, Open};
use viewtree::kit::{Reading, para};
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Shape, mark};
use crate::mailrow::{account, arrived, ask, body, decided, sender, subject, todo};
use crate::places::{Place, Question};
use crate::runtime::{Context, Effect, Held, Input, Never};
use crate::zone::Offset;

/// Which thread, as the address names it.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ThreadFlags")]
pub struct Flags {
    /// The tab's context.
    pub context: Context,
    /// The thread.
    pub id: String,
}

/// The page's one question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ThreadHold")]
pub enum Hold {
    /// The thread, held.
    Thread,
}

/// It asks nothing once.
pub type Ask = Never;

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    flags: Flags,
    read: Held<ThreadPayload>,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the host.
    Host(Box<Input<Hold, Ask>>),
    /// Back pressed.
    Back,
    /// Retry pressed.
    Retry,
}

/// The page as a program.
#[derive(Debug)]
pub enum Thread {}

fn path(f: &Flags) -> String {
    Question::Thread(&f.id).path()
}

impl Program for Thread {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Hold, Ask>;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect<Hold, Ask>>) {
        let hold = Effect::Hold {
            ask: Hold::Thread,
            path: path(&flags),
        };
        (
            Model {
                flags,
                read: Held::loading(),
            },
            vec![hold],
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect<Hold, Ask>> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.flags.context = c,
                Input::Held {
                    ask: Hold::Thread,
                    reading,
                    behind,
                } => m.read = Held::of(reading, behind),
                Input::Answered { ask, .. } => match ask {},
            },
            Msg::Back => {
                return vec![Effect::Back {
                    fallback: Place::Mail {
                        query: "",
                        account: "",
                    }
                    .href(),
                }];
            }
            Msg::Retry => {
                return vec![Effect::Again {
                    path: path(&m.flags),
                }];
            }
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let offsets = &m.flags.context.offsets;
        let (title, legend, body) = match &m.read.reading {
            Reading::Val { v } => {
                let n = v.messages.len();
                let title = v
                    .messages
                    .first()
                    .map_or("Conversation", |p| subject(&p.message.row))
                    .to_owned();
                let legend = format!(
                    "{n} message{}, oldest first · notmuch thread {}",
                    if n == 1 { "" } else { "s" },
                    v.thread
                );
                // Ask about the whole conversation, the reference only this page can offer.
                let body = std::iter::once(ask(&format!("thread:{}", v.thread)))
                    .chain(
                        v.messages
                            .iter()
                            .enumerate()
                            .map(|(i, p)| block(offsets, p, i + 1 == n)),
                    )
                    .collect();
                (title, legend, body)
            }
            Reading::Loading | Reading::Error { .. } => {
                ("Conversation".to_owned(), String::new(), Vec::new())
            }
        };
        let chrome = Chrome {
            class: "mailthread",
            title,
            back: Some(("Mail", Msg::Back)),
            legend: Some(legend),
            now: Vec::new(),
            shape: Shape::Rows,
        };
        let status = chrome::status(&m.read.reading, m.read.behind.as_ref(), offsets, Msg::Retry);
        chrome::page(chrome, status, body)
    }
}

fn block(offsets: &[Offset], p: &MailPage, open: bool) -> Node<Msg> {
    let row = &p.message.row;
    let cut = p
        .message
        .truncated
        .then(|| para("legend", "Cut at 200 KB."));
    view! { Details {Class => "why threadmsg", Open => open} [
        view! { Summary [
            view! { Span "threadwho" [sender(row)] },
            view! { Span "meta" [arrived(offsets, row)] },
            mark("chevron"),
        ] },
        view! { Div "mailchips" [
            account(row),
            decided(row),
            todo(row),
            view! { A "hashlink" {Href => Place::Message(&row.id).href()} ["open"] },
        ] },
        body(row, &p.message.body),
        cut,
    ] }
}
