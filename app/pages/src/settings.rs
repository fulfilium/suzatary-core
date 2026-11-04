//! Settings: what is connected, by group, and this device.
//!
//! A proposal card is the Inbox's TypeScript, placed as a foreign part by its reference.

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
use viewtree::node::foreign;
use viewtree::{Node, Program, view};

use crate::chrome::{self, Chrome, Masthead, Shape};
use crate::places::{Place, Question};
use crate::runtime::{self, Answer, Body, Context, Gate, Held, Local, read};
use crate::zone::when;

/// Start flags.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SettingsFlags")]
pub struct Flags {
    /// The tab's context.
    pub context: Context,
}

/// What the page watches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "SettingsHold")]
pub enum Hold {
    /// The box's overview.
    Overview,
    /// This device.
    Device,
    /// Open cards.
    Cards,
}

/// A proposal card as this page reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub struct Card {
    /// `proposal:<conversation>/<n>`.
    pub reference: String,
    /// What it does.
    pub act: CardAct,
    /// Open while standing or tapped and failed.
    pub stands: Stands,
    /// Changes when its drawing would.
    pub version: String,
}

/// A card's act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardAct {
    /// Connect a model.
    SignIn,
    /// Store a secret.
    Secret,
    /// Allow an origin.
    Grant,
    /// Anything else.
    Other,
}

/// Whether a card asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Stands {
    /// It asks.
    Open,
    /// It is settled.
    Settled,
}

/// `web/src/app/device.ts`'s `Report`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Device {
    /// Eviction policy.
    pub persist: Persist,
    /// Budget spent.
    pub usage: Usage,
    /// Bytes.
    pub budget: f64,
    /// Who it signs as.
    pub signing: Signing,
}

/// Storage persistence.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Persist {
    /// Unknown.
    Unsupported,
    /// Persistent.
    Granted,
    /// Best-effort.
    Denied,
}

/// Rendered text kept.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct Usage {
    /// Bytes.
    pub bytes: f64,
    /// Pieces.
    pub count: u32,
}

/// This device's signing key.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Signing {
    /// Root not read yet.
    Unheard,
    /// Read-only login.
    Reads,
    /// The browser cannot sign.
    Unsupported {
        /// Why.
        e: String,
    },
    /// Key unreadable.
    Unreadable {
        /// Why.
        e: String,
    },
    /// Signs with a key.
    Keyed {
        /// The key.
        key: String,
        /// Its trust.
        standing: KeyStanding,
    },
}

/// A key's trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum KeyStanding {
    /// Trusted.
    Trusted,
    /// Not yet.
    Untrusted,
    /// Revoked.
    Revoked,
}

/// An act; its button is disabled until answered.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "SettingsAct")]
#[serde(tag = "type")]
pub enum Act {
    /// Open a conversation on a template.
    Begin {
        /// The template.
        template: String,
    },
    /// File a trust card for this device.
    AskTrust {
        /// The key.
        key: String,
    },
    /// New key.
    Renew,
    /// Send the pairing code.
    Pair {
        /// The code.
        code: String,
    },
    /// Ask for a card replacing a secret.
    Replace {
        /// Its handle.
        handle: String,
    },
}

impl Act {
    fn effect(&self) -> Effect {
        let post = |q: Question, body| Effect::Post {
            ask: self.clone(),
            path: q.path(),
            body,
        };
        match self {
            Act::Begin { template } => post(Question::Setup(template), Body::Empty {}),
            Act::AskTrust { key } => post(
                Question::Key,
                Body::Key(wire::KeyAsked { key: key.clone() }),
            ),
            Act::Renew => Effect::NewKey { ask: self.clone() },
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

/// One-time asks.
pub type Ask = Act;
/// Effects.
pub type Effect = runtime::Effect<Hold, Act>;
/// Inputs.
pub type Input = runtime::Input<Hold, Act>;

/// The pairing field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    /// Its button.
    Closed,
    /// Open.
    Open {
        /// As typed.
        code: String,
    },
}

/// The model.
#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    overview: Held<Overview>,
    device: Reading<Option<Device>>,
    cards: Reading<Vec<Card>>,
    pairing: Pairing,
    running: BTreeSet<Act>,
}

/// Messages.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the runtime.
    Host(Box<Input>),
    /// Run an act.
    Run(Act),
    /// Pairing field edited.
    Pairing(Pairing),
    /// Refetch the overview.
    Retry,
}

/// The page.
#[derive(Debug)]
pub enum Settings {}

impl Program for Settings {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let model = Model {
            context: flags.context,
            overview: Held::loading(),
            device: Reading::Loading,
            cards: Reading::Loading,
            pairing: Pairing::Closed,
            running: BTreeSet::new(),
        };
        let watch = |ask, local| Effect::Watch { ask, local };
        let effects = vec![
            Effect::Hold {
                ask: Hold::Overview,
                path: Question::Overview.path(),
            },
            watch(Hold::Device, Local::Device),
            watch(Hold::Cards, Local::Cards),
        ];
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
                        // A new card changes the overview.
                        Hold::Cards => {
                            let cards = read(reading);
                            let moved = matches!(&cards, Reading::Val { v } if !matches!(&m.cards, Reading::Val { v: was } if was == v));
                            m.cards = cards;
                            moved.then(again).into_iter().collect()
                        }
                    }
                }
                Input::Answered { ask, answer } => {
                    m.running.remove(&ask);
                    answered(m, &ask, answer)
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
        match &m.cards {
            Reading::Loading => Some(para("settingsmeta settingscards", "The Inbox's cards are on their way.")),
            Reading::Error { e } => Some(para("settingshealth settingshealth-failing settingscards", format!("The Inbox's cards could not be read: {e}"))),
            Reading::Val { .. } => None,
        },
        offered(m, o),
        view! { Div "settingsbody" => groups.map(|g| (key(g.group).to_owned(), group(m, g))) },
        after,
    ] }
}

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

fn cards<'a>(m: &'a Model, l: &OverviewLine) -> Vec<&'a Card> {
    match &m.cards {
        Reading::Val { v } => l
            .cards
            .iter()
            .flat_map(|c| v.iter().filter(move |r| &r.reference == c))
            .collect(),
        Reading::Loading | Reading::Error { .. } => Vec::new(),
    }
}

/// A link card and its origin grant, both open, draw as one part.
fn line_cards(cs: &[&Card]) -> Vec<(String, Node<Msg>)> {
    let open = |act| cs.iter().find(|c| c.act == act && c.stands == Stands::Open);
    let allowing = open(CardAct::Secret).zip(open(CardAct::Grant));
    let part = |c: &Card| match allowing {
        Some((link, grant)) if c.reference == link.reference => foreign(
            "allowing",
            &c.reference,
            format!("{}\n{}\n{}", grant.reference, c.version, grant.version),
        ),
        Some(_) | None => foreign("proposal", &c.reference, &c.version),
    };
    let shown = |c: &&&Card| allowing.is_none_or(|(_, grant)| c.reference != grant.reference);
    cs.iter()
        .filter(shown)
        .map(|c| (format!("card:{}", c.reference), part(c)))
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
        .chain(line_cards(&cards(m, l)));
    view! { Section {Class => line, Data("group") => key(g), Data("line") => &l.key, Data("key") => &l.key} => parts }
}

fn group(m: &Model, g: &OverviewGroup) -> Node<Msg> {
    let signs_in = g.group == Group::Models
        && g.lines
            .iter()
            .flat_map(|l| cards(m, l))
            .any(|c| c.act == CardAct::SignIn);
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
