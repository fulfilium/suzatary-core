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
use crate::refused;
use crate::runtime::{self, Context, Listening, Never, Reach, Slot, answered, read, watch};
use viewtree::kit::Reading;

/// The composer's key for the draft, a name no conversation can have.
const DRAFT: &str = "new";

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "DraftFlags")]
pub struct Flags {
    pub context: Context,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "DraftHold")]
pub enum Hold {
    Draft,
    /// The refusals of topics this device began that the box refused.
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "DraftAsk")]
pub enum Ask {
    Open,
    Said(String),
    Composer(composer::Asking),
    /// The person acted on a refused topic's words.
    Acted,
}

pub(crate) type Effect = runtime::Effect<Ask>;
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
    /// What of the topics this device began the box refused, with their words.
    refused: Vec<suzatary_topics::wire::topics::Waiting>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Named(String),
    Composer(composer::Ev),
    Copy(String),
    Dismissed(String),
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

/// The first refused topic's question placed into the composer where it takes words back, and
/// acted on: the draft is the device's, so every tab reads the same words. A title stays on the
/// refusal.
fn taken_back(m: &mut Model) -> Vec<Effect> {
    let Some((line, sent)) = refused::question(&m.refused) else {
        return Vec::new();
    };
    let Some(outs) = m.composer.place(&sent) else {
        return Vec::new();
    };
    let mut effects = composed(m, outs);
    effects.push(dismissed(line));
    effects
}

/// The refusals of the topics this device began, as read; a reading not made yet keeps the last.
fn held(m: &mut Model, reading: Reading<runtime::Answer>) {
    if let Reading::Val { v } = read(reading) {
        m.refused = v;
    }
}

/// The person acted on a refused topic's words: dismissed them, or took them back.
fn dismissed(line: String) -> Effect {
    Effect::Core {
        ask: Ask::Acted,
        question: runtime::Core::Acted { lines: vec![line] },
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
    type Slot = Slot<Hold, Never>;
    type Sub = runtime::Sub;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let composer = Composer::of(DRAFT, &[]);
        let m = Model {
            context: flags.context,
            name: String::new(),
            composer,
            opened: Opened::No,
            refused: Vec::new(),
        };
        let focus = Effect::Focus {
            field: "compose".to_owned(),
        };
        (m, vec![focus])
    }

    fn subscriptions(m: &Model) -> Listening<Hold, Never> {
        [
            watch(Hold::Draft, m.composer.watching()),
            watch(Hold::Refused, runtime::Local::Refused),
        ]
        .into_iter()
        .collect()
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
            Msg::Copy(text) => vec![Effect::Copy { text }],
            Msg::Dismissed(line) => vec![dismissed(line)],
            Msg::Host(input) => match *input {
                Input::Context(c) => {
                    m.context = c;
                    Vec::new()
                }
                Input::Held {
                    ask: Hold::Draft,
                    reading,
                    ..
                } => {
                    m.composer.held(reading);
                    taken_back(m)
                }
                Input::Held {
                    ask: Hold::Refused,
                    reading,
                    ..
                } => {
                    held(m, reading);
                    taken_back(m)
                }
                Input::Answered {
                    ask: Ask::Acted,
                    answer,
                } => answered::<serde_json::Value>(answer)
                    .err()
                    .map(|words| Effect::Toast { words })
                    .into_iter()
                    .collect(),
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
                Input::Heard { ask, .. } | Input::Opened { ask } | Input::Lost { ask } => {
                    match ask {}
                }
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
        let head =
            view! { Div "convhead" [head, refused::view(&m.refused, Msg::Copy, Msg::Dismissed)] };
        let compose =
            composer::view(&m.composer, &m.context.reach, m.context.keys).map(Msg::Composer);
        chrome::page_headed(chrome, head, Status::Quiet, vec![compose])
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::{Answer, Gate, Text};
    use serde_json::json;
    use viewtree::Runtime;

    use super::*;

    fn page() -> Runtime<Draft> {
        let context = Context {
            gate: Gate::Writes,
            reach: Reach::Writes,
            offsets: Vec::new(),
            keys: crate::runtime::Keys::Touch,
            opens: crate::runtime::Opens::Over,
            categories: Vec::new(),
            clock: None,
        };
        Runtime::<Draft>::start(Flags { context }).0
    }

    fn held(ask: Hold, body: &serde_json::Value) -> Msg {
        Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val {
                v: Answer::Done {
                    body: Text::printed(body),
                },
            },
            behind: None,
        }))
    }

    fn acts(effects: &[Effect]) -> Vec<Vec<String>> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Core {
                    question: runtime::Core::Acted { lines },
                    ..
                } => Some(lines.clone()),
                _ => None,
            })
            .collect()
    }

    /// A refused topic's words stand on the page that begins topics, as on a conversation's page: a
    /// question placed into the composer once its draft is read, and acted on, by any number of
    /// pages alike; a title never placed nor acted on without the person; where the person typed
    /// since, the words stay on the refusal until dismissed.
    #[test]
    fn a_refused_topic_s_words_are_taken_back_and_never_lost() {
        let line = format!("{:064x}", 7);
        let refused = |words: serde_json::Value| json!([{"line": line, "ruling": {"type": "Refused", "why": "say something", "words": words}}]);
        let question = refused(
            json!({"type": "Question", "text": "Is the permit due?", "references": [], "files": [], "attachments": []}),
        );
        let draft = json!({"text": "", "others": []});

        for _ in 0..2 {
            let mut rt = page();
            let (_, effects) = rt.step(held(Hold::Refused, &question));
            assert!(acts(&effects).is_empty(), "not before the draft is read");
            let (_, effects) = rt.step(held(Hold::Draft, &draft));
            assert_eq!(
                acts(&effects),
                [vec![line.clone()]],
                "placed, and acted on once"
            );
            assert!(
                !rt.model().composer.takes_back(),
                "the words stand in the composer"
            );
        }

        let mut rt = page();
        rt.step(held(Hold::Draft, &draft));
        let (_, effects) = rt.step(held(
            Hold::Refused,
            &refused(json!({"type": "Title", "title": "Permits"})),
        ));
        assert!(
            acts(&effects).is_empty(),
            "a title is never acted on without the person"
        );
        assert!(
            rt.model().name.is_empty() && rt.model().composer.takes_back(),
            "and placed nowhere"
        );

        let mut rt = page();
        rt.step(held(Hold::Draft, &draft));
        rt.step(Msg::Composer(composer::Ev::Typed(
            "Something else".to_owned(),
        )));
        let (_, effects) = rt.step(held(Hold::Refused, &question));
        assert!(
            acts(&effects).is_empty(),
            "the composer holds the person's words"
        );
        let (_, effects) = rt.step(Msg::Dismissed(line.clone()));
        assert_eq!(acts(&effects), [vec![line]], "dismissed is acted on");
    }
}
