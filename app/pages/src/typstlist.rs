//! The open list drawn by a Typst view compiled in the tab, beside the list: what the tab's fold
//! holds is handed to the host's view (`typstview`), and its controls answer as intents from a
//! closed vocabulary, parsed here.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::json;
use suzatary_core::reference::Reference;
use ts_rs::TS;
use viewtree::node::foreign;
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, Width};
use crate::held::Folded;
use crate::needsyou::Route;
use crate::places::Place;
use crate::runtime::{Answer, Context, Effect, Gate, Input, Local, Never, read};
use crate::todoparts::{Acting, Came, Phase, begin, came, finish, said};
use viewtree::kit::Reading;

/// Which chain's list.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "TypstFlags")]
pub struct Flags {
    pub context: Context,
    pub chain: String,
}

/// The page's held readings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "TypstHold")]
pub enum Hold {
    Fold,
    /// Where the app is: the todo the view has open.
    Route,
}

/// What the page asks once: an act, at its phase, and the Undo beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "TypstAsk")]
pub enum Ask {
    Act {
        todo: String,
        acting: Acting,
        phase: Phase,
    },
    Undo {
        todo: String,
    },
}

/// What a control in the view may ask: a verb, one space, a todo's reference.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Intent {
    Open(String),
    Complete(String),
}

impl Intent {
    fn of(words: &str, chain: &str) -> Option<Intent> {
        let (verb, reference) = words.split_once(' ')?;
        let Ok(Reference::Todo(t)) = Reference::from_str(reference) else {
            return None;
        };
        if t.chain.to_string() != chain {
            return None;
        }
        let id = t.id.as_str().to_owned();
        match verb {
            "open" => Some(Intent::Open(id)),
            "complete" => Some(Intent::Complete(id)),
            _ => None,
        }
    }
}

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    chain: String,
    fold: Option<Folded>,
    open: String,
}

/// Everything that can happen to it.
#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Ask>>),
    /// A control of the view, by its intent's words.
    Intent(String),
}

/// The page as a program.
#[derive(Debug)]
pub enum Typst {}

type Fx = Effect<Hold, Ask>;

impl Model {
    fn act(&self, todo: &str, acting: Acting) -> Fx {
        begin(&self.chain, todo, acting, String::new(), |phase| Ask::Act {
            todo: todo.to_owned(),
            acting,
            phase,
        })
    }
}

impl Program for Typst {
    type Model = Model;
    type Msg = Msg;
    type Effect = Fx;
    type Flags = Flags;

    fn init(f: Flags) -> (Model, Vec<Fx>) {
        let m = Model {
            context: f.context,
            chain: f.chain,
            fold: None,
            open: String::new(),
        };
        (
            m,
            vec![
                Effect::Watch {
                    ask: Hold::Fold,
                    local: Local::Fold,
                },
                Effect::Watch {
                    ask: Hold::Route,
                    local: Local::Route,
                },
            ],
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Fx> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.context = c,
                Input::Held {
                    ask: Hold::Fold,
                    reading,
                    ..
                } => {
                    if let Reading::Val { v } = read::<Option<Folded>>(reading) {
                        m.fold = v;
                    }
                }
                Input::Held {
                    ask: Hold::Route,
                    reading,
                    ..
                } => {
                    if let Reading::Val { v } = read::<Route>(reading) {
                        m.open = v.selected;
                    }
                }
                Input::Answered { ask, answer } => return answered(m, ask, answer),
                Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
            },
            Msg::Intent(words) => match Intent::of(&words, &m.chain) {
                Some(Intent::Open(id)) => {
                    return vec![Effect::Go {
                        href: Place::Typst(&id).href(),
                    }];
                }
                Some(Intent::Complete(id)) => {
                    let entry = m.fold.as_ref().and_then(|f| f.entry(&id));
                    return match entry {
                        Some(e) if m.context.gate == Gate::Writes => {
                            vec![m.act(&id, Acting::Verb(finish(e)))]
                        }
                        Some(_) => Vec::new(),
                        None => vec![Effect::Toast {
                            words: format!("{id} is not on this list"),
                        }],
                    };
                }
                None => {}
            },
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let c = Chrome {
            class: "typstpane",
            width: Width::Wide,
            title: "Todos, drawn by Typst".to_owned(),
            masthead: Masthead::Page,
            back: None,
            legend: None,
            now: Vec::new(),
            shape: Shape::Rows,
        };
        let status = match &m.fold {
            Some(_) => chrome::Status::Quiet,
            None => chrome::Status::Waiting,
        };
        let state = m.fold.as_ref().map(|f| {
            json!({"entries": f.entries, "at": f.at, "chain": m.chain, "open": m.open,
                "writes": m.context.gate == Gate::Writes})
        });
        let body = state
            .map(|s| view! { Div "typstview" @intent(Msg::Intent) [foreign("typstview", &m.chain, s.to_string())] })
            .into_iter()
            .collect();
        chrome::page(c, status, body)
    }
}

fn answered(m: &mut Model, ask: Ask, answer: Answer) -> Vec<Fx> {
    match ask {
        Ask::Undo { todo } => vec![m.act(&todo, Acting::Undo)],
        Ask::Act {
            todo,
            acting,
            phase,
        } => {
            let next = |phase| Ask::Act {
                todo: todo.clone(),
                acting,
                phase,
            };
            match came::<Hold, Ask, Never>(&m.chain, phase, answer, next) {
                Came::Seals(seals) => seals,
                Came::Taken => said(acting, Ask::Undo { todo }),
                Came::Already => Vec::new(),
                Came::Refused(words) => vec![Effect::Toast { words }],
            }
        }
    }
}
