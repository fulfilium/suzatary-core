//! `#/secretary/memory`: every fact the secretary keeps, by kind, each with Forget, and the
//! summaries of the owner's words a conversation is told, each with Wrong.

use serde::{Deserialize, Serialize};
use suzatary_core::secretary::memory::{FactId, Kind};
use suzatary_core::wire::{FactRow, FactSaid, MemoryPayload, RecordPayload, RecordSummary, SaidOn};
use ts_rs::TS;
use viewtree::Attr::{Class, Data, Disabled, Type};
use viewtree::kit::{Reading, para};
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, callout, counted, empty, link};
use crate::places::{Place, Question};
use crate::reading::{Nothing, Opened};
use crate::runtime::{Answer, Context, Effect, Gate, Held, Input};

/// The page's two questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "MemoryHold")]
pub enum Hold {
    /// The facts.
    Memory,
    /// The summaries.
    Record,
}

/// A tap that asks the box to change the memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "MemoryAct")]
pub enum Act {
    /// Erase a fact.
    Forget(FactId),
    /// Write a summary again from the owner's lines.
    Retract(String),
}

impl Act {
    fn effect(&self) -> Effect<Hold, Act> {
        let path = match self {
            Act::Forget(id) => Question::Forget(id.0).path(),
            Act::Retract(node) => Question::Retract(node).path(),
        };
        Effect::Post {
            ask: self.clone(),
            path,
        }
    }
}

/// Its one-time asks are its acts.
pub type Ask = Act;

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    memory: Held<MemoryPayload>,
    record: Held<RecordPayload>,
    /// The acts sent and not yet answered.
    running: Vec<Act>,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the host.
    Host(Box<Input<Hold, Act>>),
    /// Back pressed.
    Back,
    /// Retry pressed.
    Retry,
    /// Forget or Wrong tapped.
    Run(Act),
}

/// The page as a program.
#[derive(Debug)]
pub enum Memory {}

const READS: [Question<'static>; 2] = [Question::Memory, Question::Record];

impl Program for Memory {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Hold, Act>;
    type Flags = Opened<Nothing>;

    fn init(o: Opened<Nothing>) -> (Model, Vec<Self::Effect>) {
        let m = Model {
            context: o.context,
            memory: Held::loading(),
            record: Held::loading(),
            running: Vec::new(),
        };
        let holds = [Hold::Memory, Hold::Record].into_iter().zip(READS);
        (
            m,
            holds
                .map(|(ask, q)| Effect::Hold {
                    ask,
                    path: q.path(),
                })
                .collect(),
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Self::Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.context = c,
                Input::Held {
                    ask: Hold::Memory,
                    reading,
                    behind,
                } => m.memory = Held::of(reading, behind),
                Input::Held {
                    ask: Hold::Record,
                    reading,
                    behind,
                } => m.record = Held::of(reading, behind),
                Input::Answered { ask, answer } => {
                    m.running.retain(|a| *a != ask);
                    // What was forgotten or written again is read again; a refusal names the act.
                    return vec![match (ask, answer) {
                        (Act::Forget(_), Answer::Done { .. }) => Effect::Again {
                            path: Question::Memory.path(),
                        },
                        (Act::Retract(_), Answer::Done { .. }) => Effect::Again {
                            path: Question::Record.path(),
                        },
                        (Act::Forget(id), Answer::Refused { words, .. }) => Effect::Toast {
                            words: format!("Could not forget fact {id}: {words}"),
                        },
                        (Act::Retract(_), Answer::Refused { words, .. }) => Effect::Toast {
                            words: format!("Could not retract the summary: {words}"),
                        },
                    }];
                }
            },
            Msg::Back => {
                return vec![Effect::Back {
                    fallback: Place::Secretary.href(),
                }];
            }
            Msg::Retry => return READS.map(|q| Effect::Again { path: q.path() }).into(),
            // A double tap would be two writes: an act runs once until it is answered.
            Msg::Run(act) if !m.running.contains(&act) => {
                m.running.push(act.clone());
                return vec![act.effect()];
            }
            Msg::Run(_) => {}
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let reading = m.memory.reading.both(&m.record.reading);
        let (legend, body) = match &reading {
            Reading::Val { v: (p, r) } => (
                Some(format!(
                    "{} of {} facts, background for every conversation.",
                    p.facts.len(),
                    p.cap
                )),
                body(m, p, r),
            ),
            Reading::Loading | Reading::Error { .. } => (None, Vec::new()),
        };
        let c = Chrome {
            class: "memorypage",
            title: "Memory".to_owned(),
            masthead: Masthead::Page,
            back: Some(("Secretary", Msg::Back)),
            legend,
            now: Vec::new(),
            shape: Shape::Rows,
        };
        let status = chrome::status(
            &reading,
            m.memory.behind.as_ref(),
            &m.context.offsets,
            Msg::Retry,
        );
        chrome::page(c, status, body)
    }
}

const KINDS: [(Kind, &str, &str); 4] = [
    (Kind::Owner, "owner", "About you"),
    (Kind::Person, "person", "People"),
    (Kind::Preference, "preference", "Preferences"),
    (Kind::Standing, "standing", "Standing instructions"),
];

fn body(m: &Model, p: &MemoryPayload, r: &RecordPayload) -> Vec<Node<Msg>> {
    let mut out: Vec<Node<Msg>> = KINDS
        .iter()
        .filter_map(|&(k, name, label)| {
            let facts: Vec<Node<Msg>> = p
                .facts
                .iter()
                .filter(|f| f.kind == k)
                .map(|f| fact(m, f))
                .collect();
            (!facts.is_empty()).then(|| {
                group(
                    Some(name),
                    "memorygroup",
                    label,
                    counted(facts.len(), "fact", "facts"),
                    facts,
                )
            })
        })
        .collect();
    if out.is_empty() {
        out.push(empty("Nothing remembered yet."));
    }
    let summaries: Vec<Node<Msg>> = r.summaries.iter().map(|s| summary(m, s)).collect();
    if !summaries.is_empty() {
        let count = counted(summaries.len(), "summary", "summaries");
        out.push(group(
            None,
            "memorygroup recordgroup",
            "Summaries of what you said",
            count,
            summaries,
        ));
    }
    out.push(callout("Forgetting erases", view! { P ["A forgotten fact leaves the memory's file, not only this list. A conversation that quoted it keeps its words. A summary marked Wrong is deleted and written again from your lines."] }));
    out
}

fn group(
    kind: Option<&str>,
    class: &str,
    label: &str,
    count: String,
    rows: Vec<Node<Msg>>,
) -> Node<Msg> {
    view! { Section {Class => class, Data("kind") => kind} [
        view! { Div "memoryhead" [view! { Span "grouplabel" [label] }, view! { Span "groupcount" [count] }] },
        view! { Ul "memoryfacts" [rows] },
    ] }
}

/// The round that said it: the conversation windowed at the turn, or the card a tap answered;
/// in its own words unless `words` says otherwise.
fn said(class: &str, s: &SaidOn, words: Option<String>) -> Node<Msg> {
    let (id, card, at, own) = match s {
        SaidOn::Turn {
            conversation: c,
            turn,
        } => (
            c,
            String::new(),
            Some(*turn),
            format!("said in {c}, turn {turn}"),
        ),
        SaidOn::Tapped {
            conversation: c,
            proposal: p,
        } => (
            c,
            format!("proposal:{c}/{p}"),
            None,
            format!("tapped in {c}, card {p}"),
        ),
    };
    link(
        class,
        Place::Conversation {
            id,
            reference: &card,
            at,
        },
        words.unwrap_or(own),
    )
}

/// Where a fact comes from: the round that said it, or the facts it replaced and where each was said.
fn provenance(s: &FactSaid) -> Node<Msg> {
    match s {
        FactSaid::Said { on } => said("memorysaid", on, None),
        FactSaid::Merged {
            from,
            origins,
            round,
        } => {
            let from: Vec<String> = from.iter().map(|id| format!("fact {id}")).collect();
            let merged = said(
                "memorymerged",
                round,
                Some(format!("merged from {}", from.join(", "))),
            );
            let origins: Vec<Node<Msg>> = origins
                .iter()
                .map(|o| said("memorysaid", o, None))
                .collect();
            view! { Span "memoryprovenance" [merged, origins] }
        }
    }
}

/// An act's button, offered only to a login that writes and disabled while it runs.
fn act(m: &Model, class: &str, label: &str, a: Act) -> Option<Node<Msg>> {
    let busy = m.running.contains(&a);
    (m.context.gate == Gate::Writes)
        .then(|| view! { Button {Type => "button", Class => class, Disabled => busy} @click(Msg::Run(a)) [label] })
}

fn row(
    class: &str,
    data: (&'static str, String),
    text: &str,
    foot: Vec<Option<Node<Msg>>>,
) -> Node<Msg> {
    view! { Li {Class => class, Data(data.0) => data.1} [para("memoryfacttext", text), view! { Div "memoryfactfoot" [foot] }] }
}

fn fact(m: &Model, f: &FactRow) -> Node<Msg> {
    let forget = act(m, "act forget", "Forget", Act::Forget(f.id));
    row(
        "memoryfact",
        ("fact", f.id.to_string()),
        &f.fact,
        vec![Some(provenance(&f.said)), forget],
    )
}

fn summary(m: &Model, s: &RecordSummary) -> Node<Msg> {
    let wrong = act(m, "act retract", "Wrong", Act::Retract(s.node.clone()));
    let lines = if s.lines == 1 {
        "of 1 line".to_owned()
    } else {
        format!("of {} lines", s.lines)
    };
    row(
        "memoryfact recordsummary",
        ("node", s.node.clone()),
        &s.text,
        vec![Some(view! { Span "memorysaid" [lines] }), wrong],
    )
}
