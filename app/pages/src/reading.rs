//! A page that is a view of one held question and has no acts of its own: the calendar, the
//! briefings and a briefing are each a [`View`], and [`One`] is the program of any of them.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::kit::Reading;
use viewtree::{Node, Program};

use crate::chrome::{self, Chrome, Masthead, Shape};
use crate::places::Place;
use crate::runtime::{Answer, Behind, Context, Effect, Input, Never, answered};
use crate::zone::Offset;

/// One page of one reading: what its address names, the path it holds, how the answer reads,
/// and how it looks.
pub trait View {
    /// What the address names.
    type At: std::fmt::Debug + Clone + serde::de::DeserializeOwned;
    /// What the answer reads as.
    type Of: std::fmt::Debug + Clone;
    /// The page's class.
    const CLASS: &'static str;
    /// The placeholder's shape.
    const SHAPE: Shape = Shape::Rows;
    /// How its title stands.
    const MASTHEAD: Masthead = Masthead::Page;
    /// The path it holds.
    fn path(f: &Self::At) -> String;
    /// Back's label and where it goes when the tab has nothing to go back to.
    #[must_use]
    fn back(_: &Self::At) -> Option<(&'static str, Place<'_>)> {
        None
    }
    /// The answer read; a refusal is a failure unless the page says otherwise.
    fn read(a: Answer) -> Result<Self::Of, String>;
    /// Its title, from what is read once there is something.
    fn title(f: &Self::At, v: Option<&Self::Of>) -> String;
    /// The line under the title.
    #[must_use]
    fn legend(_: &Self::At, _: &[Offset], _: Option<&Self::Of>) -> Option<String> {
        None
    }
    /// Its body.
    fn body(f: &Self::At, offsets: &[Offset], v: &Self::Of) -> Vec<Node<Msg>>;
}

/// Such a page asks nothing once.
pub type Ask = Never;

/// The one question such a page holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "OneHold")]
pub enum Hold {
    /// The page's reading.
    Page,
}

/// What the host opens a page with: the tab's context, and what the address names.
#[derive(Debug, Clone, Deserialize)]
pub struct Opened<A> {
    /// The tab's context.
    pub context: Context,
    /// What the address names.
    #[serde(flatten)]
    pub at: A,
}

/// The address of a page that names nothing.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Nothing {}

/// Everything such a page knows.
#[derive(Debug, Clone)]
pub struct Model<V: View> {
    context: Context,
    at: V::At,
    reading: Reading<V::Of>,
    behind: Option<Behind>,
}

/// Everything that can happen to it.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the host.
    Host(Box<Input<Hold, Never>>),
    /// Back pressed.
    Back,
    /// Retry pressed.
    Retry,
}

/// The program of a [`View`].
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
                Input::Answered { ask, .. } => match ask {},
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

/// The answer read as JSON of `T`: the default reading of a [`View`].
///
/// # Errors
/// The box refused, or answered in another shape.
pub fn json<T: serde::de::DeserializeOwned>(a: Answer) -> Result<T, String> {
    answered(a)
}
