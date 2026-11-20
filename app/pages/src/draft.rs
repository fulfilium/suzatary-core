//! A topic not asked anything yet: the first send seals the conversation's opening under the
//! typed name and the question resting on it, handed to the box in one post, so the box keeps no
//! conversation nobody spoke in; a send made meanwhile waits for the opening's seal.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::Attr::{AriaLabel, Maxlength, Placeholder, Type, Value};
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, Status};
use crate::composer::{self, Composer};
use crate::places::Place;
use crate::runtime::{self, Context, Reach, answered};

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

fn say(id: &str, sent: &composer::Sent) -> Effect {
    Effect::Core {
        ask: Ask::Said(id.to_owned()),
        question: runtime::Core::Line {
            line: runtime::Sealing::Say {
                conversation: id.to_owned(),
                question: runtime::Asking::of(sent),
            },
        },
    }
}

fn composed(m: &mut Model, outs: Vec<composer::Out>) -> Vec<Effect> {
    let (mut effects, then) = composer::effects(outs, Ask::Composer);
    for t in then {
        let composer::Then::Send(sent) = t else {
            continue;
        };
        match &mut m.opened {
            Opened::Yes(id) => effects.push(say(id, &sent)),
            Opened::Opening(waiting) => waiting.push(sent),
            Opened::No => {
                effects.push(Effect::Core {
                    ask: Ask::Open,
                    question: runtime::Core::Line {
                        line: runtime::Sealing::Begin {
                            title: m.name.clone(),
                            say: runtime::Asking::of(&sent),
                        },
                    },
                });
                m.opened = Opened::Opening(vec![sent]);
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
                    match answered::<runtime::Sealed>(answer) {
                        // The first send went with the opening; the rest rest on it.
                        Ok(runtime::Sealed { conversation: id }) => {
                            let outs = m.composer.sealed();
                            let mut effects = composed(m, outs);
                            effects.extend(waiting.iter().skip(1).map(|s| say(&id, s)));
                            effects.push(Effect::Replace {
                                href: Place::conversation(&id).href(),
                            });
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
                    ask: Ask::Said(_),
                    answer,
                } => match answered::<runtime::Sealed>(answer) {
                    Ok(_) => {
                        let outs = m.composer.sealed();
                        composed(m, outs)
                    }
                    Err(words) => {
                        m.composer.released();
                        vec![Effect::Toast { words }]
                    }
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
