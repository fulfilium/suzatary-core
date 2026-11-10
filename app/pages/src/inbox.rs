//! The Inbox: what waits on the owner, ascending in urgency, then what was settled today. A row
//! this app cannot resolve is plain text with no control.

use serde::{Deserialize, Serialize};
use suzatary_core::wire::UnplacedProposal;
use ts_rs::TS;
use viewtree::Attr::{Class, Data, Open};
use viewtree::kit::{Reading, para};
use viewtree::{Node, Program, view};

use crate::card::{self, Cards, Drawn, Placed, Titles};
use crate::chrome::{self, Chrome, Masthead, Shape, Status, link};
use crate::intents::Intents;
use crate::places::Place;
use crate::runtime::{self, Context, Local, read};
use crate::waiting::{CHAINS, Readings, Row, Waiting, Watched, unheld};
use crate::zone;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "InboxFlags")]
pub struct Flags {
    pub context: Context,
    pub open: String,
    #[ts(type = "number")]
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "InboxHold")]
pub enum Hold {
    Rows(Watched),
    Card(card::Key),
    Intents,
    Titles,
}

pub type Ask = card::Asking;
pub(crate) type Effect = runtime::Effect<Hold, Ask>;
pub(crate) type Input = runtime::Input<Hold, Ask>;

/// Reached unfolds its Setup row and scrolls to it once; it stays marked while the page reads.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Opening {
    None,
    Pending(String),
    Scrolled(String),
}

impl Opening {
    fn key(&self) -> Option<&str> {
        match self {
            Opening::None => None,
            Opening::Pending(k) | Opening::Scrolled(k) => Some(k),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    readings: Readings,
    cards: Cards,
    intents: Intents,
    titles: Titles,
    today: chrono::NaiveDate,
    setup: bool,
    settled: bool,
    opening: Opening,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Card(card::Key, card::Ev),
    Setup(bool),
    Settled(bool),
}

#[derive(Debug)]
pub enum Inbox {}

fn watch(w: Watched) -> Effect {
    Effect::Watch {
        local: w.local(),
        ask: Hold::Rows(w),
    }
}

impl Program for Inbox {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let (readings, watched) = Readings::of(&CHAINS);
        let today = zone::day_ms(&flags.context.offsets, flags.now);
        let model = Model {
            context: flags.context,
            readings,
            cards: Cards::default(),
            intents: Intents::default(),
            titles: Titles::new(),
            today,
            setup: false,
            settled: false,
            opening: if flags.open.is_empty() {
                Opening::None
            } else {
                Opening::Pending(flags.open)
            },
        };
        let mut effects: Vec<Effect> = watched.into_iter().map(watch).collect();
        effects.push(Effect::Watch {
            ask: Hold::Intents,
            local: Local::Intents,
        });
        effects.push(Effect::Watch {
            ask: Hold::Titles,
            local: Local::Titles,
        });
        (model, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => {
                    m.context = c;
                    Vec::new()
                }
                Input::Held { ask, reading, .. } => match ask {
                    Hold::Rows(w) => {
                        let mut effects: Vec<Effect> =
                            m.readings.held(w, reading).into_iter().map(watch).collect();
                        effects.extend(seen(m));
                        effects
                    }
                    Hold::Card(key) => {
                        if let Reading::Val { v } = read::<serde_json::Value>(reading) {
                            m.cards.held(key, v);
                        }
                        Vec::new()
                    }
                    Hold::Intents => {
                        if let Reading::Val { v } = read(reading) {
                            m.intents = v;
                        }
                        Vec::new()
                    }
                    Hold::Titles => {
                        if let Reading::Val { v } = read(reading) {
                            m.titles = v;
                        }
                        Vec::new()
                    }
                },
                Input::Answered { ask, answer } => {
                    let readings = &m.readings;
                    let outs = m.cards.asked(ask, answer, |k| readings.card(k));
                    out(outs)
                }
                Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
            },
            Msg::Card(key, ev) => {
                let rows = m.readings.rows();
                match row_of(&rows, &key).cloned() {
                    Some(row) => {
                        let outs = m.cards.update(&key, &row, ev);
                        out(outs)
                    }
                    None => Vec::new(),
                }
            }
            Msg::Setup(open) => {
                m.setup = open;
                Vec::new()
            }
            Msg::Settled(open) => {
                m.settled = open;
                Vec::new()
            }
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let rows = m.readings.rows();
        let (work, setup): (Vec<Row<'_>>, Vec<Row<'_>>) = rows.iter().partition(|r| !r.setup());
        let unplaced = m.readings.unplaced();
        let problems = m.readings.problems();
        let problem = (!problems.is_empty()).then(|| problems.join(" "));
        let answered = m.readings.answered();
        let owed = rows.len() + unplaced.len();
        let legend = match &problem {
            Some(_) => "Something did not answer, so this may not be everything.".to_owned(),
            None => waiting_sentence(owed - setup.len(), setup.len()),
        };
        let status = if problem.is_none() && !answered && owed == 0 {
            Status::Waiting
        } else {
            Status::Quiet
        };
        let chrome = Chrome {
            class: "inboxpage",
            title: "Inbox".to_owned(),
            masthead: Masthead::Destination,
            width: crate::chrome::Width::Column,
            back: None,
            legend: Some(legend),
            now: Vec::new(),
            shape: Shape::Cards,
        };
        let all = m.readings.all();
        let settled = crate::waiting::settled_on(&all, m.today, &m.context.offsets);
        let unreadable: Vec<Waiting<'_>> = all
            .iter()
            .copied()
            .filter(|w| matches!(w.standing(), crate::waiting::Standing::Unreadable(_)))
            .collect();
        let nothing = (problem.is_none() && answered).then(|| chrome::empty("Nothing is waiting on you. Changes the triage agent and the secretary propose appear here."));
        let listed: Vec<Node<Msg>> = if owed == 0 {
            nothing.into_iter().collect()
        } else {
            let mut v: Vec<Node<Msg>> = work.iter().map(|r| row(m, *r)).collect();
            v.extend(setup_row(m, &setup));
            v
        };
        let body = vec![
            problem.map(|p| para("problem", p)),
            Some(
                view! { Div "proposals" {Data("box") => if answered { "Answered" } else { "Asking" }} [
                    listed,
                    unplaced_section(&unplaced),
                    unreadable_section(&unreadable),
                    settled_section(m, &settled),
                ] },
            ),
        ];
        chrome::page(chrome, status, body.into_iter().flatten().collect())
    }
}

fn row_of<'a>(rows: &[Row<'a>], key: &card::Key) -> Option<&'a suzatary_core::wire::ProposalRow> {
    rows.iter().find_map(|r| match r {
        Row::Card(_, c, row) if card::Key::of(c.id, row) == *key => Some(*row),
        Row::Card(..) | Row::Unresolved(_) => None,
    })
}

fn seen(m: &mut Model) -> Vec<Effect> {
    let rows = m.readings.rows();
    let outs = m.cards.seen(rows.iter().filter_map(|r| match r {
        Row::Card(_, c, row) => Some((c.id, *row)),
        Row::Unresolved(_) => None,
    }));
    let readings = &m.readings;
    let outs: Vec<card::Out> = outs
        .into_iter()
        .chain(m.cards.settled(|k| readings.card(k)))
        .collect();
    let opened = m
        .opening
        .key()
        .and_then(|k| rows.iter().find(|r| r.waiting().key() == k))
        .map(|r| (r.setup(), field(*r)));
    let mut effects = card::effects(outs, Hold::Card, |a| a).0;
    match (&m.opening, opened) {
        (Opening::Pending(k), Some((setup, field))) => {
            m.setup |= setup;
            effects.push(Effect::Scroll {
                to: runtime::Scroll::Into { field },
            });
            m.opening = Opening::Scrolled(k.clone());
        }
        (Opening::Pending(_) | Opening::Scrolled(_) | Opening::None, _) => {}
    }
    effects
}

fn field(r: Row<'_>) -> String {
    match r {
        Row::Card(_, c, row) => format!("card/{}", card::Key::of(c.id, row).print()),
        Row::Unresolved(w) => format!("decision/{}", w.key()),
    }
}

fn out(outs: Vec<card::Out>) -> Vec<Effect> {
    let (mut effects, replies) = card::effects(outs, Hold::Card, |a| a);
    // This page has no composer, so Reply opens the conversation with the card attached.
    effects.extend(replies.into_iter().map(|k| {
        Effect::Go {
            href: Place::Conversation {
                id: &k.conversation,
                reference: &k.reference(),
                focus: false,
                at: None,
            }
            .href(),
        }
    }));
    effects
}

fn waiting_sentence(work: usize, steps: usize) -> String {
    let setup = chrome::counted(steps, "setup step", "setup steps");
    match (work, steps) {
        (0, 0) => String::new(),
        (w, 0) => format!("{} waiting on you.", chrome::counted(w, "thing", "things")),
        (0, _) => format!("{setup} waiting on you."),
        (w, _) => format!("{w} waiting on you, and {setup}."),
    }
}

fn row(m: &Model, shown: Row<'_>) -> Node<Msg> {
    let waits = shown.waiting();
    let opened = m.opening.key() == Some(waits.key());
    let unpriced = waits.unpriced().map(|chain| {
        para(
            "legend notyetpriced",
            format!(
                "Not yet priced: this device holds nothing of {chain} yet, so it is listed last."
            ),
        )
    });
    match shown {
        Row::Card(_, conversation, proposal) => {
            let placed = if card::is_setup(&proposal.card.by) {
                Placed::Setup
            } else {
                Placed::Inbox(conversation.title)
            };
            let drawn = Drawn {
                conversation: conversation.id,
                row: proposal,
                placed,
                reach: &m.context.reach,
                intents: &m.intents,
                titles: &m.titles,
                opened,
            };
            m.cards
                .view(drawn, unpriced.into_iter().collect(), Vec::new(), Msg::Card)
        }
        Row::Unresolved(w) => {
            let said: Vec<Node<Msg>> = match w {
                Waiting::Proposal(p, _) => vec![
                    para("proposalbody", p.headline.as_str()),
                    para("proposalwhy", p.why.as_str()),
                ],
                Waiting::Decision(_, e) => match &e.authored {
                    None => vec![para("proposalbody", e.todo.as_str())],
                    Some(a) => vec![
                        Some(chrome::rich(
                            a.rich.body.as_deref(),
                            &a.body,
                            "proposalbody",
                        )),
                        (!a.rationale.is_empty()).then(|| {
                            chrome::rich(
                                a.rich.rationale.as_deref(),
                                &a.rationale.join(" "),
                                "proposalwhy",
                            )
                        }),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                },
            };
            let class = if opened {
                "batch decision-unresolved opened"
            } else {
                "batch decision-unresolved"
            };
            view! { Section {Class => class, Data("decision") => w.key(), Data("field") => format!("decision/{}", w.key())} [
                unpriced,
                said,
                para("problem", m.readings.problem(w)),
            ] }
        }
    }
}

fn setup_row(m: &Model, rows: &[Row<'_>]) -> Option<Node<Msg>> {
    (!rows.is_empty()).then(|| {
        let n = rows.len();
        view! { Details "setupgroup" {Open => m.setup, Data("setup") => n.to_string()} @toggle(Msg::Setup) [
            view! { Summary [view! { Span [view! { Span "setuplabel" ["Setup"] }, format!(" · {}", chrome::counted(n, "thing to set up", "things to set up"))] }] },
            rows.iter().map(|r| row(m, *r)).collect::<Vec<_>>(),
            link("setupsettings", Place::Settings, "Open Settings"),
        ] }
    })
}

fn unplaced_section(lines: &[(&str, &UnplacedProposal)]) -> Option<Node<Msg>> {
    (!lines.is_empty()).then(|| {
        view! { Div "unplaced" [lines.iter().map(|(conversation, u)| {
            let line = format!("proposal:{conversation}/{}", u.index);
            view! { Div "problem" {Data("unplaced") => &line} [
                para("", format!("No card for {line}: {}.", unheld(u.why))),
                link("", Place::Conversation { id: conversation, reference: &line, focus: false, at: None }, "Open its conversation"),
            ] }
        }).collect::<Vec<_>>()] }
    })
}

fn unreadable_section(ws: &[Waiting<'_>]) -> Option<Node<Msg>> {
    (!ws.is_empty()).then(|| {
        view! { Div "unreadable" [ws.iter().map(|w| {
            let why = match w.standing() {
                crate::waiting::Standing::Unreadable(why) => why,
                crate::waiting::Standing::Open | crate::waiting::Standing::Settled(..) => "",
            };
            view! { Div "problem" {Data("unreadable") => w.key()} [
                para("", format!("Cannot be read, so nothing acts on it: {why}.")),
                para("settledwhy", w.asked()),
            ] }
        }).collect::<Vec<_>>()] }
    })
}

fn settled_section(m: &Model, settled: &[(Waiting<'_>, &'static str, &str)]) -> Option<Node<Msg>> {
    (!settled.is_empty()).then(|| {
        let n = settled.len();
        view! { Details "settledtoday" {Open => m.settled} @toggle(Msg::Settled) [
            view! { Summary [format!("Settled today: {}", chrome::counted(n, "decision", "decisions"))] },
            settled.iter().map(|(w, how, at)| {
                let why = match w {
                    Waiting::Proposal(p, _) => para("settledwhy", p.headline.as_str()),
                    Waiting::Decision(_, e) => match &e.authored {
                        None => para("settledwhy", e.todo.as_str()),
                        Some(a) => chrome::rich(a.rich.body.as_deref(), &a.body, "settledwhy"),
                    },
                };
                view! { Div {Class => format!("settledbatch settled-{how}"), Data("settled") => w.key()} [
                    view! { P "settledhead" [
                        view! { Span "settledwhat" [*how] },
                        view! { Span "settledchain" [w.source()] },
                        view! { Span "settledwhen" [at.replace('T', " ")] },
                    ] },
                    why,
                ] }
            }).collect::<Vec<_>>(),
        ] }
    })
}
