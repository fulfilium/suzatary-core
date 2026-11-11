//! The popup of what newly waits: anything that starts waiting after the app opened pops up
//! until it stops waiting, its page is on screen, or the owner closes it.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use viewtree::Attr::{Href, Role, Title, Type};
use viewtree::{Node, Program, view};

use crate::places::Place;
use crate::runtime::{self, Context, Local, Never, read};
use crate::waiting::{CHAINS, Readings, Waiting, Watched, Where, open};

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "NeedsYouFlags")]
pub struct Flags {
    pub context: Context,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[ts(rename = "Where")]
pub struct Route {
    pub(crate) page: String,
    pub(crate) conversation: String,
    /// The todo a list's screen shows in its pane, `""` for none.
    pub(crate) selected: String,
    /// What a list's screen puts in its add sheet, shared from another app, `""` for nothing.
    pub(crate) shared: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "NeedsYouHold")]
pub enum Hold {
    Rows(Watched),
    Route,
}

pub(crate) type Ask = Never;
pub(crate) type Effect = runtime::Effect<Hold, Never>;
pub type Input = runtime::Input<Hold, Never>;

#[derive(Debug, Clone)]
pub struct Model {
    readings: Readings,
    route: Route,
    baseline: Option<BTreeSet<String>>,
    shown: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Close(String),
}

#[derive(Debug)]
pub enum NeedsYou {}

/// A popup over its page would cover the button it is about.
fn on_screen(w: Waiting<'_>, route: &Route) -> bool {
    route.page == "inbox"
        || (route.page == "conversation"
            && matches!(w.at(), Where::Proposal(c, _) if c == route.conversation))
}

impl NeedsYou {
    fn draw(m: &mut Model) {
        if !m.readings.read() {
            m.baseline = None;
            return;
        }
        let all = m.readings.all();
        let waiting = open(&all);
        let keys: BTreeSet<String> = waiting.iter().map(|w| w.key().to_owned()).collect();
        let route = &m.route;
        m.shown.retain(|k| {
            waiting
                .iter()
                .any(|w| w.key() == k && !on_screen(*w, route))
        });
        if let Some(base) = &m.baseline {
            for w in &waiting {
                let k = w.key().to_owned();
                if !base.contains(&k) && !m.shown.contains(&k) && !on_screen(*w, route) {
                    m.shown.push(k);
                }
            }
        }
        m.baseline = Some(keys);
    }
}

impl Program for NeedsYou {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(_: Flags) -> (Model, Vec<Effect>) {
        let (readings, watched) = Readings::of(&CHAINS);
        let mut effects: Vec<Effect> = watched
            .into_iter()
            .map(|w| Effect::Watch {
                local: w.local,
                ask: Hold::Rows(w.ask),
            })
            .collect();
        effects.push(Effect::Watch {
            ask: Hold::Route,
            local: Local::Route,
        });
        let m = Model {
            readings,
            route: Route::default(),
            baseline: None,
            shown: Vec::new(),
        };
        (m, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(_) => {}
                Input::Held {
                    ask: Hold::Rows(w),
                    reading,
                    ..
                } => {
                    // The popup reads no card, so the conversations it names are not watched.
                    let _named = m.readings.held(w, reading);
                    NeedsYou::draw(m);
                }
                Input::Held {
                    ask: Hold::Route,
                    reading,
                    ..
                } => {
                    if let viewtree::kit::Reading::Val { v } = read::<Route>(reading) {
                        m.route = v;
                        NeedsYou::draw(m);
                    }
                }
                Input::Answered { ask, .. } | Input::Heard { ask, .. } | Input::Opened { ask } => {
                    match ask {}
                }
            },
            Msg::Close(k) => m.shown.retain(|s| *s != k),
        }
        Vec::new()
    }

    fn view(m: &Model) -> Node<Msg> {
        let all = m.readings.all();
        view! { Div "needsyous" => m.shown.iter().filter_map(|k| all.iter().find(|w| w.key() == k)).map(|w| {
            let k = w.key().to_owned();
            (k.clone(), view! { Div "needsyou" {Role => "alert"} [
                view! { Div "needsyouhead" ["Needs you"] },
                view! { Div "needsyoutext" [w.asked()] },
                view! { Div "needsyourow" [
                    view! { A "needsyouopen" {Href => Place::Inbox(&k).href()} @click(Msg::Close(k.clone())) ["Open"] },
                    view! { Button "needsyouclose" {Type => "button", Title => "Later"} @click(Msg::Close(k.clone())) ["×"] },
                ] },
            ] })
        }) }
    }
}
