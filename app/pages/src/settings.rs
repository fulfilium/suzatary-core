//! Settings: what is connected, by group, and this device.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use suzatary_core::connect::{Failure, Group, Standing};
use suzatary_core::wire::{self, Offer, Overview, OverviewGroup, OverviewLine};
use ts_rs::TS;
use viewtree::Attr::{
    AriaLabel, Autocapitalize, Autocomplete, Class, Data, Href, Placeholder, Spellcheck, Type,
    Value,
};
use viewtree::kit::{Reading, button, disclosure, para};
use viewtree::{Node, Program, view};

use crate::card::{self, Cards, Drawn, Placed};
use crate::chrome::{self, Chrome, Masthead, Shape};
use crate::intents::Intents;
use crate::places::{Place, Question};
use crate::runtime::{self, Answer, Body, Context, Gate, Held, Local, read};
use crate::waiting::{Readings, Row, Watched};
use crate::zone::when;

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SettingsFlags")]
pub struct Flags {
    pub context: Context,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "SettingsHold")]
pub enum Hold {
    Overview,
    Device,
    Rows(Watched),
    Card(card::Key),
    Intents,
}

#[derive(Debug, Clone, Deserialize, TS)]
pub struct Device {
    pub(crate) persist: Persist,
    pub(crate) usage: Usage,
    pub(crate) budget: f64,
    pub(crate) signing: Signing,
}

#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(tag = "type")]
pub(crate) enum Persist {
    Unsupported,
    Granted,
    Denied,
}

#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub(crate) struct Usage {
    pub(crate) bytes: f64,
    pub(crate) count: u32,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub(crate) enum Signing {
    Unheard,
    Reads,
    Unsupported { e: String },
    Unreadable { e: String },
    Keyed { key: String, standing: KeyStanding },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub(crate) enum KeyStanding {
    Trusted,
    Untrusted,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "SettingsAct")]
#[serde(tag = "type")]
pub enum Act {
    Begin {
        template: String,
    },
    AskTrust {
        key: String,
    },
    Renew,
    Pair {
        code: String,
    },
    /// A card asking for a new value of the stored secret `handle`.
    Replace {
        handle: String,
    },
}

impl Act {
    fn effect(&self) -> Effect {
        let post = |q: Question, body| Effect::Post {
            ask: Ask::Run(self.clone()),
            path: q.path(),
            body,
        };
        match self {
            Act::Begin { template } => post(Question::Setup(template), Body::Empty {}),
            Act::AskTrust { key } => post(
                Question::Key,
                Body::Key(wire::KeyAsked { key: key.clone() }),
            ),
            Act::Renew => Effect::NewKey {
                ask: Ask::Run(self.clone()),
            },
            Act::Pair { code } => post(
                Question::Pair,
                Body::Pair(wire::Pairing { code: code.clone() }),
            ),
            Act::Replace { handle } => post(Question::Replace(handle), Body::Empty {}),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Opened {
    id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "SettingsAsk")]
#[serde(tag = "type", content = "of")]
pub enum Ask {
    Run(Act),
    Card(card::Asking),
}
pub(crate) type Effect = runtime::Effect<Hold, Ask>;
pub(crate) type Input = runtime::Input<Hold, Ask>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    Closed,
    Open { code: String },
}

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    overview: Held<Overview>,
    device: Reading<Option<Device>>,
    rows: Readings,
    cards: Cards,
    intents: Intents,
    pairing: Pairing,
    running: BTreeSet<Act>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Run(Act),
    Pairing(Pairing),
    Retry,
    Card(card::Key, card::Ev),
}

#[derive(Debug)]
pub enum Settings {}

impl Program for Settings {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let (rows, watched) = Readings::of(&[]);
        let model = Model {
            context: flags.context,
            overview: Held::loading(),
            device: Reading::Loading,
            rows,
            cards: Cards::default(),
            intents: Intents::default(),
            pairing: Pairing::Closed,
            running: BTreeSet::new(),
        };
        let watch = |ask, local| Effect::Watch { ask, local };
        let mut effects = vec![
            Effect::Hold {
                ask: Hold::Overview,
                path: Question::Overview.path(),
            },
            watch(Hold::Device, Local::Device),
            watch(Hold::Intents, Local::Intents),
        ];
        effects.extend(
            watched
                .into_iter()
                .map(|w| watch(Hold::Rows(w.clone()), w.local())),
        );
        (model, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Context(c) => {
                    m.context = c;
                    Vec::new()
                }
                Input::Held {
                    ask,
                    reading,
                    behind,
                } => {
                    match ask {
                        Hold::Overview => {
                            m.overview = Held::of(reading, behind);
                            Vec::new()
                        }
                        Hold::Device => {
                            m.device = read(reading);
                            Vec::new()
                        }
                        // A card the box files changes which line asks for what.
                        Hold::Rows(w) => {
                            let was = shown_cards(m);
                            let new = m.rows.held(w, reading);
                            let mut effects: Vec<Effect> = new
                                .into_iter()
                                .map(|w| Effect::Watch {
                                    local: w.local(),
                                    ask: Hold::Rows(w),
                                })
                                .collect();
                            effects.extend(seen(m));
                            if shown_cards(m) != was {
                                effects.push(again());
                            }
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
                    }
                }
                Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
                Input::Answered {
                    ask: Ask::Run(act),
                    answer,
                } => {
                    m.running.remove(&act);
                    answered(m, &act, answer)
                }
                Input::Answered {
                    ask: Ask::Card(asking),
                    answer,
                } => {
                    let follows = grant_taken(m, &asking, &answer);
                    let readings = &m.rows;
                    let mut outs = m.cards.asked(asking, answer, |k| readings.card(k));
                    outs.extend(follows);
                    // The box checks a stored link inside the tap, so the line's health is
                    // read again once the tap is answered, whatever else moves after it.
                    let mut effects = out(outs);
                    effects.push(again());
                    effects
                }
            },
            Msg::Run(act) => m
                .running
                .insert(act.clone())
                .then(|| act.effect())
                .into_iter()
                .collect(),
            Msg::Pairing(p) => {
                m.pairing = p;
                Vec::new()
            }
            Msg::Retry => vec![again()],
            Msg::Card(key, ev) => {
                let rows = m.rows.rows();
                match row_of(&rows, &key) {
                    Some(row) => {
                        let row = row.clone();
                        out(m.cards.update(&key, &row, ev))
                    }
                    None => Vec::new(),
                }
            }
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let c = Chrome {
            class: "settingspage",
            title: "Settings".to_owned(),
            masthead: Masthead::Page,
            back: None,
            legend: None,
            now: Vec::new(),
            shape: Shape::Lines,
        };
        let status = chrome::status(
            &m.overview.reading,
            m.overview.behind.as_ref(),
            &m.context.offsets,
            Msg::Retry,
        );
        let body = match &m.overview.reading {
            Reading::Val { v } => vec![shown(m, v)],
            Reading::Loading | Reading::Error { .. } => Vec::new(),
        };
        chrome::page(c, status, body)
    }
}

fn again() -> Effect {
    Effect::Again {
        path: Question::Overview.path(),
    }
}

fn focused(id: &str) -> String {
    Place::Conversation {
        id,
        reference: "",
        focus: true,
        at: None,
    }
    .href()
}

fn shown(m: &Model, o: &Overview) -> Node<Msg> {
    let device = match &m.device {
        Reading::Val { v: Some(d) } => Some(d),
        Reading::Val { v: None } | Reading::Loading | Reading::Error { .. } => None,
    };
    let here = view! { Div "settingshere" [device.map(|d| vec![view! { H2 ["This device"] }, self::device(m, d)])] };
    let first = matches!(
        device,
        Some(Device {
            signing: Signing::Keyed {
                standing: KeyStanding::Untrusted,
                ..
            },
            ..
        })
    );
    let (before, after) = if first {
        (Some(here), None)
    } else {
        (None, Some(here))
    };
    let ask = match m.context.gate {
        Gate::Writes | Gate::Reads => Some(
            view! { A "act act-ask" {Href => focused(&o.ask)} ["Ask to add or change anything"] },
        ),
        Gate::Views => None,
    };
    let groups = o.groups.iter().filter(|g| !g.lines.is_empty());
    view! { Div "settingsview" [
        before,
        view! { Div "settingsask" [ask] },
        match &m.rows.proposals {
            Reading::Error { e } => Some(para("settingshealth settingshealth-failing settingscards", format!("The Inbox's cards could not be read: {e}"))),
            Reading::Loading | Reading::Val { .. } if !m.rows.answered() => Some(para("settingsmeta settingscards", "The Inbox's cards are on their way.")),
            Reading::Loading | Reading::Val { .. } => None,
        },
        offered(m, o),
        view! { Div "settingsbody" => groups.map(|g| (key(g.group).to_owned(), group(m, g))) },
        after,
    ] }
}

/// A refusal is said in its words, a conversation begun is gone to, a trust is said, and the
/// overview is asked again after the acts that move it.
fn answered(m: &mut Model, act: &Act, answer: Answer) -> Vec<Effect> {
    match answer {
        Answer::Refused { words, .. } => {
            let said = Effect::Toast { words };
            match act {
                Act::AskTrust { .. } | Act::Renew | Act::Replace { .. } => vec![said, again()],
                Act::Begin { .. } | Act::Pair { .. } => vec![said],
            }
        }
        Answer::Done { body } => match act {
            Act::Begin { .. } => vec![match runtime::answered::<Opened>(Answer::Done { body }) {
                Ok(Opened { id }) => Effect::Go { href: focused(&id) },
                Err(words) => Effect::Toast { words },
            }],
            Act::Pair { .. } => {
                m.pairing = Pairing::Closed;
                vec![
                    Effect::Toast {
                        words: "This device is trusted.".to_owned(),
                    },
                    again(),
                ]
            }
            Act::AskTrust { .. } | Act::Renew => vec![again()],
            Act::Replace { .. } => vec![
                Effect::Toast {
                    words: "Type the new value in the card on this line; the old one stays until you store it.".to_owned(),
                },
                again(),
            ],
        },
    }
}

fn key(g: Group) -> &'static str {
    match g {
        Group::Storage => "storage",
        Group::Mail => "mail",
        Group::Calendar => "calendar",
        Group::Drive => "drive",
        Group::Models => "models",
        Group::Notifications => "notifications",
        Group::Coding => "coding",
        Group::Plugins => "plugins",
        Group::Box => "box",
    }
}

fn title(g: Group) -> &'static str {
    match g {
        Group::Storage => "Storage",
        Group::Mail => "Mail",
        Group::Calendar => "Calendar",
        Group::Drive => "Drive",
        Group::Models => "Models",
        Group::Notifications => "Notifications",
        Group::Coding => "Coding sessions",
        Group::Plugins => "Other connections",
        Group::Box => "This box",
    }
}

fn open_cards(rows: &[Row<'_>]) -> Vec<(String, card::Key)> {
    rows.iter()
        .filter_map(|r| match r {
            Row::Card(_, c, row) => Some((c.id.clone(), card::Key::of(&c.id, row))),
            Row::Unresolved(_) => None,
        })
        .collect()
}

fn row_of<'a>(rows: &[Row<'a>], key: &card::Key) -> Option<&'a suzatary_core::wire::ProposalRow> {
    rows.iter().find_map(|r| match r {
        Row::Card(_, c, row) if card::Key::of(&c.id, row) == *key => Some(*row),
        Row::Card(..) | Row::Unresolved(_) => None,
    })
}

/// Every card the page reads, waiting or settled, each whole: any change to one (the outcome a
/// check writes after the tap, say) can change what its line says, so the overview is asked
/// again.
fn shown_cards(m: &Model) -> Vec<(String, suzatary_core::wire::ProposalRow)> {
    m.rows
        .cards
        .iter()
        .flat_map(|(id, read)| match read {
            Reading::Val { v } => v
                .proposals
                .iter()
                .map(|p| (id.clone(), p.clone()))
                .collect(),
            Reading::Loading | Reading::Error { .. } => Vec::new(),
        })
        .collect()
}

fn seen(m: &mut Model) -> Vec<Effect> {
    let rows = m.rows.rows();
    let outs = m.cards.seen(rows.iter().filter_map(|r| match r {
        Row::Card(_, c, row) => Some((c.id.as_str(), *row)),
        Row::Unresolved(_) => None,
    }));
    let readings = &m.rows;
    let settled = m.cards.settled(|k| readings.card(k));
    out(outs.into_iter().chain(settled).collect())
}

fn out(outs: Vec<card::Out>) -> Vec<Effect> {
    card::effects(outs, Hold::Card, Ask::Card).0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    SignIn,
    Link,
    Grant,
    Other,
}

fn kind(row: &suzatary_core::wire::ProposalRow) -> Kind {
    use suzatary_core::wire::ProposedAct;
    match &row.act {
        ProposedAct::SignIn { .. } | ProposedAct::SignInCode { .. } => Kind::SignIn,
        ProposedAct::Secret { .. } => Kind::Link,
        ProposedAct::Grant { .. } => Kind::Grant,
        ProposedAct::Email { .. }
        | ProposedAct::Forward { .. }
        | ProposedAct::Changes { .. }
        | ProposedAct::Auth { .. }
        | ProposedAct::AuthSend { .. }
        | ProposedAct::Web { .. }
        | ProposedAct::Patch { .. }
        | ProposedAct::Drive { .. }
        | ProposedAct::CodingSession { .. }
        | ProposedAct::Afresh { .. }
        | ProposedAct::Device { .. }
        | ProposedAct::UnlockStore { .. }
        | ProposedAct::Erase { .. }
        | ProposedAct::Retract { .. }
        | ProposedAct::GoogleClient { .. }
        | ProposedAct::GoogleSignIn { .. }
        | ProposedAct::CalendarEvent { .. }
        | ProposedAct::Connect { .. }
        | ProposedAct::Disconnect { .. }
        | ProposedAct::Remember { .. }
        | ProposedAct::Consolidate { .. }
        | ProposedAct::Decision { .. } => Kind::Other,
    }
}

/// The grant is drawn inside the link's card it was asked for, and its yes sends the link again.
fn allowing<'a>(
    rows: &[&'a suzatary_core::wire::ProposalRow],
) -> Option<(
    &'a suzatary_core::wire::ProposalRow,
    &'a suzatary_core::wire::ProposalRow,
)> {
    let open = |k| {
        rows.iter()
            .copied()
            .find(|r| kind(r) == k && card::answerable(r.state))
    };
    open(Kind::Link).zip(open(Kind::Grant))
}

/// Sends the link again, once, with the link the tab kept, whatever the grant's card reads since.
fn grant_taken(m: &Model, asking: &card::Asking, a: &Answer) -> Vec<card::Out> {
    let card::Asking::Act { key, then, .. } = asking else {
        return Vec::new();
    };
    if !then.yes || !card::taken(a) {
        return Vec::new();
    }
    let Some(named) = m.overview_line(&key.reference()) else {
        return Vec::new();
    };
    // The grant may be settled by the time its answer is read; its card is read whatever its state.
    let granted = m.rows.card(key).is_some_and(|r| kind(r) == Kind::Grant);
    let link = named
        .iter()
        .filter_map(|k| m.rows.card(k))
        .find(|r| kind(r) == Kind::Link && card::answerable(r.state));
    match link {
        Some(asked) if granted => vec![Cards::resend(
            &card::Key::of(&key.conversation, asked),
            asked,
        )],
        Some(_) | None => Vec::new(),
    }
}

impl Model {
    fn overview_line(&self, reference: &str) -> Option<Vec<card::Key>> {
        let Reading::Val { v } = &self.overview.reading else {
            return None;
        };
        let rows = self.rows.rows();
        let open = open_cards(&rows);
        v.groups
            .iter()
            .flat_map(|g| &g.lines)
            .find(|l| l.cards.iter().any(|c| c == reference))
            .map(|l| {
                l.cards
                    .iter()
                    .filter_map(|c| {
                        open.iter()
                            .find(|(_, k)| k.reference() == *c)
                            .map(|(_, k)| k.clone())
                    })
                    .collect()
            })
    }
}

static NO_TITLES: card::Titles = card::Titles::new();

/// A link's card and the grant its origin needs, when both stand, as one card holding the other.
fn line_cards(m: &Model, l: &OverviewLine) -> Vec<(String, Node<Msg>)> {
    let rows = m.rows.rows();
    let cards: Vec<(&str, &suzatary_core::wire::ProposalRow)> = l
        .cards
        .iter()
        .filter_map(|c| {
            rows.iter().find_map(|r| match r {
                Row::Card(_, conv, row) if card::Key::of(&conv.id, row).reference() == *c => {
                    Some((conv.id.as_str(), *row))
                }
                Row::Card(..) | Row::Unresolved(_) => None,
            })
        })
        .collect();
    let bare: Vec<_> = cards.iter().map(|(_, r)| *r).collect();
    let pair = allowing(&bare);
    let drawn = |conversation, row| Drawn {
        conversation,
        row,
        placed: Placed::Settings,
        reach: &m.context.reach,
        intents: &m.intents,
        titles: &NO_TITLES,
        opened: false,
    };
    cards
        .iter()
        .filter(|(_, r)| pair.is_none_or(|(_, grant)| r.index != grant.index))
        .map(|&(conversation, row)| {
            let within = match pair {
                Some((link, grant)) if link.index == row.index => {
                    vec![view! { Div "proposalallow" [m.cards.view(drawn(conversation, grant), Vec::new(), Vec::new(), Msg::Card)] }]
                }
                Some(_) | None => Vec::new(),
            };
            (
                format!("card:{}", card::Key::of(conversation, row).reference()),
                m.cards.view(drawn(conversation, row), Vec::new(), within, Msg::Card),
            )
        })
        .collect()
}

fn cause(c: Failure) -> &'static str {
    match c {
        Failure::Revoked { .. } => " Its link was changed or revoked, so store the new one.",
        Failure::NeedsLogin => " Its sign-in lapsed: sign in again.",
        Failure::Unresolved => " A secret it reads is stored nowhere: set it.",
        Failure::KeyRefused => " Its key was refused: the card below connects it again.",
        Failure::Limited(_) => {
            " The limit ends by itself, and the box asks the model nothing until then."
        }
        Failure::Unusable => " The link stored for it is not one the box sends: set it again.",
        Failure::Unreachable => " Check the link and set it again.",
        Failure::Unclassified => "",
    }
}

fn and(xs: &[String]) -> String {
    match xs {
        [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => xs.concat(),
    }
}

fn capital(s: &str) -> String {
    let mut cs = s.chars();
    cs.next()
        .map(|c| c.to_uppercase().chain(cs).collect())
        .unwrap_or_default()
}

/// `None` for a standing in place with nothing to say.
fn health<'a>(
    s: &'a Standing,
    at: &dyn Fn(&str) -> String,
) -> Option<(bool, String, Option<&'a str>)> {
    let (asks, words) = match s {
        Standing::Kept => return None,
        Standing::Synced { at: t } => (false, format!("Synced {}.", at(t))),
        Standing::Answered { at: t } => (false, format!("Answered {}.", at(t))),
        Standing::Unobserved => (false, "Nothing has checked it yet.".to_owned()),
        Standing::Locked => (
            true,
            "The secret store is locked, so whether it is stored is not known.".to_owned(),
        ),
        Standing::StoreFailed => (true, "The secret store could not answer.".to_owned()),
        Standing::Unreadable { words } => (true, format!("{}.", capital(words))),
        Standing::ByHand { why, command, .. } => {
            return Some((
                true,
                format!("Set up at a terminal on the box: {why}."),
                Some(command),
            ));
        }
        Standing::Failing {
            since,
            class,
            words,
            at: tried,
        } => {
            let (since, last) = (at(since), at(tried));
            let tried = if last == since {
                String::new()
            } else {
                format!(" (last tried {last})")
            };
            (
                true,
                format!("Failing since {since}: {words}{tried}.{}", cause(*class)),
            )
        }
        Standing::Missing { what } => (true, format!("Needs {}.", and(what))),
    };
    Some((asks, words, None))
}

/// `11:32 on 2026-09-04` on the owner's clock; one that does not parse is said as written, as
/// `web/src/view/format.ts`'s `when` does.
fn line(m: &Model, g: Group, l: &OverviewLine) -> Node<Msg> {
    let said = health(&l.standing, &|t| when(&m.context.offsets, t));
    let (asks, words, command) = said.map_or((false, None, None), |(a, w, c)| (a, Some(w), c));
    let (line, health) = if asks {
        (
            "settingsline settingsline-failing",
            "settingshealth settingshealth-failing",
        )
    } else {
        ("settingsline", "settingshealth")
    };
    // Fixed keys, so a card arriving or leaving moves no other part.
    let head = [
        Some(("label", view! { H3 [l.label.as_str()] })),
        l.detail
            .as_ref()
            .map(|d| ("detail", view! { P "settingsmeta" [d.as_str()] })),
        words.map(|w| ("health", view! { P {Class => health} [w] })),
        command.map(|c| {
            (
                "atbox",
                disclosure(
                    "settingsatbox",
                    "How to set this up at the box",
                    view! { Code [c] },
                ),
            )
        }),
    ];
    let replaces = match m.context.gate {
        Gate::Writes => l
            .replaces
            .iter()
            .map(|r| {
                let act = Act::Replace {
                    handle: r.handle.clone(),
                };
                let busy = m.running.contains(&act);
                (
                    format!("replace:{}", r.handle),
                    button(
                        "act act-replace",
                        &format!("Replace {}", r.label),
                        Msg::Run(act),
                        busy,
                    ),
                )
            })
            .collect(),
        Gate::Reads | Gate::Views => Vec::new(),
    };
    let parts = head
        .into_iter()
        .flatten()
        .map(|(k, n)| (k.to_owned(), n))
        .chain(replaces)
        .chain(line_cards(m, l));
    view! { Section {Class => line, Data("group") => key(g), Data("line") => &l.key, Data("key") => &l.key} => parts }
}

fn group(m: &Model, g: &OverviewGroup) -> Node<Msg> {
    let signs_in = g.group == Group::Models
        && g.lines
            .iter()
            .any(|l| {
                let rows = m.rows.rows();
                l.cards.iter().any(|c| {
                    rows.iter().any(|r| matches!(r, Row::Card(_, conv, row) if card::Key::of(&conv.id, row).reference() == *c && kind(row) == Kind::SignIn))
                })
            });
    let unanswered = view! { P "settingshealth settingshealth-failing settingsunanswered" ["No model answers on this box yet. Connect one:"] };
    let head = [
        Some(("title", view! { H2 [title(g.group)] })),
        signs_in.then_some(("unanswered", unanswered)),
    ];
    let lines = g
        .lines
        .iter()
        .map(|l| (format!("line:{}", l.key), line(m, g.group, l)));
    view! { Section "settingsgroup" {Data("group") => key(g.group)} => head.into_iter().flatten().map(|(k, n)| (k.to_owned(), n)).chain(lines) }
}

fn offer(m: &Model, x: &Offer) -> Node<Msg> {
    let act = Act::Begin {
        template: x.template.clone(),
    };
    let busy = m.running.contains(&act);
    match m.context.gate {
        Gate::Writes => button(
            &format!("act settingsoffer settingsoffer-{}", x.template),
            &capital(&x.title),
            Msg::Run(act),
            busy,
        ),
        Gate::Reads | Gate::Views => para("settingsmeta settingsoffer", capital(&x.title)),
    }
}

fn offered(m: &Model, o: &Overview) -> Option<Node<Msg>> {
    let mut groups: Vec<(Group, Vec<&Offer>)> = Vec::new();
    for x in &o.offers {
        match groups.last_mut() {
            Some((g, xs)) if *g == x.group => xs.push(x),
            Some(_) | None => groups.push((x.group, vec![x])),
        }
    }
    let each = |(g, xs): (Group, Vec<&Offer>)| view! { Div "settingsoffers" {Data("group") => key(g)} [view! { H3 [title(g)] }, xs.into_iter().map(|x| offer(m, x)).collect::<Vec<_>>()] };
    (!groups.is_empty()).then(|| {
        view! { Section "settingsoffered" [
            view! { H2 ["Connect an account"] },
            para("settingsmeta", "Tap one: a conversation opens and says each step."),
            groups.into_iter().map(each).collect::<Vec<_>>(),
        ] }
    })
}

fn said(s: &Signing) -> String {
    let short = |k: &str| k.chars().take(12).collect::<String>();
    match s {
        Signing::Unheard => {
            "This device has not read the box's root yet, so it signs nothing until it has."
                .to_owned()
        }
        Signing::Reads => "This login only reads, so this device signs nothing.".to_owned(),
        Signing::Unsupported { e } => {
            format!("This browser cannot sign ({e}), so the box refuses what it writes.")
        }
        Signing::Unreadable { e } => format!(
            "This device cannot sign: {e}, so no other is made over it. A new key, once trusted, writes again."
        ),
        Signing::Keyed { key, standing } => {
            let key = short(key);
            match standing {
                KeyStanding::Trusted => {
                    format!("This device signs with the key {key}…, which the chain trusts.")
                }
                KeyStanding::Revoked => format!(
                    "This device's key {key}… was revoked: nothing it writes is taken. A new key, once trusted, writes again; what the box refused of the old one's stays refused."
                ),
                KeyStanding::Untrusted => format!(
                    "This device's key {key}… is not trusted yet. Trust it with the code that suzatary device pair prints at the box, or ask, then tap the card in the Inbox on a trusted device."
                ),
            }
        }
    }
}

fn code(code: String) -> Msg {
    Msg::Pairing(Pairing::Open { code })
}

fn pairing(m: &Model) -> Node<Msg> {
    let sending = m.running.iter().any(|a| matches!(a, Act::Pair { .. }));
    match &m.pairing {
        Pairing::Closed => {
            view! { Div "pairing" [button("act act-pair", "Enter the code from the box", code(String::new()), false)] }
        }
        Pairing::Open { code: typed } => view! { Div "pairing" [
            para("settingsmeta", "At the box, run suzatary device pair, then type the code it prints. It works once, for 10 minutes."),
            view! { Input "paircode" {Type => "text", Value => typed, Autocomplete => "off", Autocapitalize => "characters", Spellcheck => "false", Placeholder => "BCDF-GHJK", AriaLabel => "The code from the box"} @input(code) },
            button("act act-paircode", "Trust this device", Msg::Run(Act::Pair { code: typed.clone() }), typed.trim().is_empty() || sending),
            button("act act-cancelpair", "Cancel", Msg::Pairing(Pairing::Closed), false),
        ] },
    }
}

fn device(m: &Model, d: &Device) -> Node<Msg> {
    let mb = |bytes: f64| format!("{:.1} MB", bytes / (1024.0 * 1024.0));
    let pieces = if d.usage.count == 1 {
        "piece"
    } else {
        "pieces"
    };
    let persist = match d.persist {
        Persist::Granted => "Storage is persistent: the browser will not evict it under pressure.",
        Persist::Denied => "Storage is best-effort: the browser may evict it under pressure.",
        Persist::Unsupported => "This browser cannot make storage persistent.",
    };
    let run = |class, label, act: Act| {
        button(
            class,
            label,
            Msg::Run(act.clone()),
            m.running.contains(&act),
        )
    };
    let renew = |label| run("act act-renewkey", label, Act::Renew);
    let (key, acts) = match &d.signing {
        Signing::Keyed {
            key,
            standing: KeyStanding::Untrusted,
        } => {
            let ask = run(
                "act act-asktrust",
                "Ask to trust this device",
                Act::AskTrust { key: key.clone() },
            );
            (
                key.as_str(),
                vec![pairing(m), ask, renew("Use a new key on this device")],
            )
        }
        Signing::Keyed {
            key,
            standing: KeyStanding::Trusted,
        } => (
            key.as_str(),
            vec![renew(
                "Use a new key on this device (it must be trusted again)",
            )],
        ),
        Signing::Keyed {
            key,
            standing: KeyStanding::Revoked,
        } => (key.as_str(), vec![renew("Use a new key on this device")]),
        Signing::Unreadable { .. } => ("", vec![renew("Use a new key on this device")]),
        Signing::Unheard | Signing::Reads | Signing::Unsupported { .. } => ("", Vec::new()),
    };
    let kept = format!(
        "Rendered text kept here: {} of {} ({} {pieces}).",
        mb(d.usage.bytes),
        mb(d.budget),
        d.usage.count
    );
    view! { Section "settingsdevice" [para("settingsmeta", kept), para("settingsmeta", persist), view! { P "settingsmeta settingskey" {Data("key") => key} [said(&d.signing)] }, acts] }
}
