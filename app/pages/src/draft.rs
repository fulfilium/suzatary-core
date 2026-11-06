//! A topic not asked anything yet: the first send opens the conversation under the typed name,
//! so the box keeps no conversation nobody spoke in; every send waits on the one open.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::Attr::{AriaLabel, Maxlength, Placeholder, Type, Value};
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, Status};
use crate::composer::{self, Composer};
use crate::places::Place;
use crate::runtime::{
    self, Act, Body, Context, Outcome, Question, Questioning, Reach, Said, Secret, answered,
};

/// The composer's key for the draft, a name no conversation can have.
const DRAFT: &str = "new";

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DraftFlags")]
pub struct Flags {
    pub context: Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "DraftHold")]
pub enum Hold {
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "DraftAsk")]
pub enum Ask {
    Open,
    Said(String),
    Composer(composer::Asking),
}

pub(crate) type Effect = runtime::Effect<Hold, Ask>;
pub type Input = runtime::Input<Hold, Ask>;

/// Not asked, asked with sends waiting on it, or open; a failed open is asked again by the next
/// send.
#[derive(Debug, Clone)]
enum Opened {
    No,
    Opening(Vec<composer::Sent>),
    Yes(String),
}

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    name: String,
    composer: Composer,
    opened: Opened,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Named(String),
    Composer(composer::Ev),
}

#[derive(Debug)]
pub enum Draft {}

#[derive(Debug, Deserialize)]
struct Opening {
    id: String,
}

fn say(id: &str, sent: composer::Sent) -> Effect {
    Effect::Act {
        ask: Ask::Said(id.to_owned()),
        act: Act::Question {
            kind: Questioning::Say,
            question: Question {
                conversation: id.to_owned(),
                after: 0,
                text: sent.text,
                references: sent.references,
                files: sent.files,
            },
        },
        secret: Secret::None,
        told: runtime::Told::Held,
    }
}

fn composed(m: &mut Model, outs: Vec<composer::Out>) -> Vec<Effect> {
    let (mut effects, then) = composer::effects(outs, Ask::Composer);
    for t in then {
        let composer::Then::Send(sent) = t else {
            continue;
        };
        match &mut m.opened {
            Opened::Yes(id) => effects.push(say(id, sent)),
            Opened::Opening(waiting) => waiting.push(sent),
            Opened::No => {
                m.opened = Opened::Opening(vec![sent]);
                effects.push(Effect::Post {
                    ask: Ask::Open,
                    path: "/api/secretary/conversations".to_owned(),
                    body: Body::Titling(suzatary_core::wire::Titling {
                        title: m.name.clone(),
                    }),
                });
            }
        }
    }
    effects
}

impl Program for Draft {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let (composer, draft) = Composer::of(DRAFT, &[]);
        let m = Model {
            context: flags.context,
            name: String::new(),
            composer,
            opened: Opened::No,
        };
        let effects = vec![
            Effect::Watch {
                ask: Hold::Draft,
                local: draft,
            },
            Effect::Focus {
                field: "compose".to_owned(),
            },
        ];
        (m, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Named(name) => {
                m.name = name;
                Vec::new()
            }
            Msg::Composer(ev) => {
                let outs = m.composer.update(ev);
                composed(m, outs)
            }
            Msg::Host(input) => match *input {
                Input::Context(c) => {
                    m.context = c;
                    Vec::new()
                }
                Input::Held { reading, .. } => {
                    m.composer.held(reading);
                    Vec::new()
                }
                Input::Answered {
                    ask: Ask::Open,
                    answer,
                } => {
                    let waiting = match std::mem::replace(&mut m.opened, Opened::No) {
                        Opened::Opening(w) => w,
                        Opened::No | Opened::Yes(_) => Vec::new(),
                    };
                    match answered::<Opening>(answer) {
                        Ok(Opening { id }) => {
                            let effects = waiting.into_iter().map(|s| say(&id, s)).collect();
                            m.opened = Opened::Yes(id);
                            effects
                        }
                        Err(words) => {
                            let outs: Vec<composer::Out> = waiting
                                .into_iter()
                                .flat_map(|s| m.composer.failed(s))
                                .collect();
                            let mut effects = composed(m, outs);
                            effects.push(Effect::Toast { words });
                            effects
                        }
                    }
                }
                Input::Answered {
                    ask: Ask::Said(id),
                    answer,
                } => match answered::<Outcome>(answer) {
                    Ok(
                        Outcome::Answered {
                            answer: Said::Taken,
                            ..
                        }
                        | Outcome::Already,
                    ) => vec![Effect::Replace {
                        href: Place::conversation(&id).href(),
                    }],
                    Ok(Outcome::Answered {
                        answer: Said::Refused { e } | Said::Lost { e },
                        ..
                    })
                    | Err(e) => vec![Effect::Toast { words: e }],
                },
                Input::Answered {
                    ask: Ask::Composer(c),
                    answer,
                } => {
                    let outs = m.composer.answered(c, answer);
                    composed(m, outs)
                }
                Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
            },
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let head = match m.context.reach {
            Reach::Writes => {
                view! { Input "renamefield topicname" {Type => "text", Maxlength => "80", Placeholder => "Name it, or leave it blank", AriaLabel => "Topic name", Value => &m.name} @input(Msg::Named) }
            }
            Reach::Waits | Reach::Reads { .. } => view! { H1 "pagetitle" ["New topic"] },
        };
        let chrome = Chrome {
            class: "conversation",
            title: "New topic".to_owned(),
            masthead: Masthead::Page,
            width: crate::chrome::Width::Column,
            back: None,
            legend: None,
            now: Vec::new(),
            shape: Shape::Turns,
        };
        let head = view! { Div "convhead" [head] };
        let compose =
            composer::view(&m.composer, &m.context.reach, m.context.wide).map(Msg::Composer);
        chrome::page_headed(chrome, head, Status::Quiet, vec![compose])
    }
}
