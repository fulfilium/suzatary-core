//! A page that is a view of one held question and has no acts of its own; `One` is the program
//! of any `View`.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::kit::Reading;
use viewtree::{Node, Program};

use crate::chrome::{self, Chrome, Masthead, Shape};
use crate::places::Place;
use crate::runtime::{Answer, Behind, Context, Effect, Input, Never, answered};
use crate::zone::Offset;

pub trait View {
    type At: std::fmt::Debug + Clone + serde::de::DeserializeOwned;
    type Of: std::fmt::Debug + Clone;
    const CLASS: &'static str;
    const SHAPE: Shape = Shape::Rows;
    const MASTHEAD: Masthead = Masthead::Page;
    fn path(f: &Self::At) -> String;
    /// Back's label, and where it goes when the tab has nothing to go back to.
    #[must_use]
    fn back(_: &Self::At) -> Option<(&'static str, Place<'_>)> {
        None
    }
    /// A refusal is a failure unless the page says otherwise.
    fn read(a: Answer) -> Result<Self::Of, String>;
    fn title(f: &Self::At, v: Option<&Self::Of>) -> String;
    #[must_use]
    fn legend(_: &Self::At, _: &[Offset], _: Option<&Self::Of>) -> Option<String> {
        None
    }
    fn body(f: &Self::At, offsets: &[Offset], v: &Self::Of) -> Vec<Node<Msg>>;
}

pub type Ask = Never;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "OneHold")]
pub enum Hold {
    Page,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Opened<A> {
    pub context: Context,
    #[serde(flatten)]
    pub at: A,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct Nothing {}

#[derive(Debug, Clone)]
pub struct Model<V: View> {
    context: Context,
    at: V::At,
    reading: Reading<V::Of>,
    behind: Option<Behind>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Never>>),
    Back,
    Retry,
    Copy(String),
}

#[derive(Debug)]
pub struct One<V>(PhantomData<V>);

impl<V: View> Program for One<V> {
    type Model = Model<V>;
    type Msg = Msg;
    type Effect = Effect<Hold, Never>;
    type Flags = Opened<V::At>;

    fn init(Opened { context, at }: Opened<V::At>) -> (Model<V>, Vec<Self::Effect>) {
        let hold = Effect::Hold {
            ask: Hold::Page,
            path: V::path(&at),
        };
        let m = Model {
            context,
            at,
            reading: Reading::Loading,
            behind: None,
        };
        (m, vec![hold])
    }

    fn update(m: &mut Model<V>, msg: Msg) -> Vec<Self::Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.context = c,
                Input::Held {
                    ask: Hold::Page,
                    reading,
                    behind,
                } => {
                    m.reading = match reading {
                        Reading::Loading => Reading::Loading,
                        Reading::Error { e } => Reading::Error { e },
                        Reading::Val { v } => match V::read(v) {
                            Ok(v) => Reading::Val { v },
                            Err(e) => Reading::Error { e },
                        },
                    };
                    m.behind = behind;
                }
                Input::Answered { ask, .. } | Input::Heard { ask, .. } | Input::Opened { ask } => {
                    match ask {}
                }
            },
            Msg::Back => {
                return V::back(&m.at)
                    .map(|(_, to)| Effect::Back {
                        fallback: to.href(),
                    })
                    .into_iter()
                    .collect();
            }
            Msg::Retry => {
                return vec![Effect::Again {
                    path: V::path(&m.at),
                }];
            }
            Msg::Copy(text) => return vec![Effect::Copy { text }],
        }
        Vec::new()
    }

    fn view(m: &Model<V>) -> Node<Msg> {
        let f = &m.at;
        let held = match &m.reading {
            Reading::Val { v } => Some(v),
            Reading::Loading | Reading::Error { .. } => None,
        };
        let offsets = &m.context.offsets;
        let c = Chrome {
            class: V::CLASS,
            title: V::title(f, held),
            masthead: V::MASTHEAD,
            back: V::back(f).map(|(label, _)| (label, Msg::Back)),
            legend: V::legend(f, offsets, held),
            now: Vec::new(),
            shape: V::SHAPE,
        };
        let status = chrome::status(&m.reading, m.behind.as_ref(), offsets, Msg::Retry);
        chrome::page(
            c,
            status,
            held.map(|v| V::body(f, offsets, v)).unwrap_or_default(),
        )
    }
}

/// The default reading of a `View`; fails when the box refused or answered in another shape.
pub(crate) fn json<T: serde::de::DeserializeOwned>(a: Answer) -> Result<T, String> {
    answered(a)
}
