//! About: what the app claims about itself, the archive, the version the box runs, and the
//! releases the project chain reads.

use serde::{Deserialize, Serialize};
use suzatary_core::wire::{AboutPayload, ReleasesPayload};
use ts_rs::TS;
use viewtree::kit::{Reading, para};
use viewtree::node::text;
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape, link};
use crate::places::{Place, Question};
use crate::reading::{Nothing, Opened};
use crate::releases;
use crate::runtime::{Context, Effect, Held, Input, Never};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "AboutHold")]
pub enum Hold {
    About,
    Releases,
}

pub type Ask = Never;

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    running: Held<AboutPayload>,
    releases: Held<ReleasesPayload>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Never>>),
    Retry,
}

#[derive(Debug)]
pub enum About {}

/// The claim has a page, not a footer.
fn claims() -> Vec<Node<Msg>> {
    let prose = |words: Vec<Node<Msg>>| para("aboutprose", words);
    vec![
        view! { H2 ["One evaluator, on two machines"] },
        prose(vec![text(
            "Everything on these pages is folded from the event chain by the server — and this browser runs the same core (prodrome, compiled to WebAssembly), so it can check that for itself: it samples each todo's thirty days itself, and when the network is gone it folds the objects it is holding rather than showing you yesterday's numbers. One evaluator, on two machines.",
        )]),
        prose(vec![text(
            "The browser computes no fulfillment of its own: every number on screen is one the core produced, on the box or in this tab. Low means urgent, everywhere a number appears.",
        )]),
        prose(vec![text(
            "Every number can be explained: a todo's page has \"Why this number\".",
        )]),
        view! { H2 ["Archive"] },
        prose(vec![
            text(
                "The morning brief is a secretary conversation since 11 September 2026. The briefings saved before that are still here: ",
            ),
            link("", Place::Briefings, "saved briefings"),
            text("."),
        ]),
        view! { H2 ["Version"] },
    ]
}

/// Only the box can say what it runs: never this module's own idea.
fn version(p: &AboutPayload) -> Node<Msg> {
    let built = if p.build == "dev" {
        ", a development build outside the flake.".to_owned()
    } else {
        format!(", built from revision {}.", p.build)
    };
    view! { P "aboutprose aboutversion" [view! { Strong [format!("Suzatary {}", p.version)] }, built] }
}

impl Program for About {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect<Hold, Never>;
    type Flags = Opened<Nothing>;

    fn init(o: Opened<Nothing>) -> (Model, Vec<Self::Effect>) {
        let hold = |ask, q: Question<'_>| Effect::Hold {
            ask,
            path: q.path(),
        };
        let m = Model {
            context: o.context,
            running: Held::loading(),
            releases: Held::loading(),
        };
        (
            m,
            vec![
                hold(Hold::About, Question::About),
                hold(Hold::Releases, Question::Releases),
            ],
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Self::Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => m.context = c,
                Input::Held {
                    ask: Hold::About,
                    reading,
                    behind,
                } => m.running = Held::of(reading, behind),
                Input::Held {
                    ask: Hold::Releases,
                    reading,
                    behind,
                } => m.releases = Held::of(reading, behind),
                Input::Answered { ask, .. } | Input::Heard { ask, .. } | Input::Opened { ask } => {
                    match ask {}
                }
            },
            Msg::Retry => {
                return [Question::About, Question::Releases]
                    .map(|q| Effect::Again { path: q.path() })
                    .into();
            }
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        // Releases that cannot be read are said in the page and lose nothing else.
        let settled = m.releases.reading.settled();
        let reading = m.running.reading.both(&settled);
        let body = match &reading {
            Reading::Val { v: (v, ()) } => [Some(version(v)), releases::view(&m.releases.reading)]
                .into_iter()
                .flatten()
                .collect(),
            Reading::Loading | Reading::Error { .. } => Vec::new(),
        };
        let c = Chrome {
            class: "aboutpage",
            title: "About".to_owned(),
            masthead: Masthead::Page,
            width: crate::chrome::Width::Column,
            back: None,
            legend: None,
            now: claims(),
            shape: Shape::Lines,
        };
        let status = chrome::status(
            &reading,
            m.running.behind.as_ref(),
            &m.context.offsets,
            Msg::Retry,
        );
        chrome::page(c, status, body)
    }
}
