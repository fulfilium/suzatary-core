//! Settings: what is connected, by group, and this device, as a model, an update and a view.
//!
//! The host (`web/src/pages/settingsview.ts`) hands over what the page reads (`Input::Shown`)
//! and what each act came to (`Input::Answered`); every tap is a message here, and everything
//! the page asks of the world is an [`Effect`]. A proposal card is still the Inbox's TypeScript,
//! placed as a foreign part by its reference, until the Inbox is converted.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use suzatary_core::connect::{Failure, Group, Standing};
use suzatary_core::wire::{Offer, Overview, OverviewGroup, OverviewLine};
use ts_rs::TS;
use viewtree::Attr::{
    AriaLabel, Autocapitalize, Autocomplete, Class, Data, Href, Placeholder, Spellcheck, Type,
    Value,
};
use viewtree::kit::{Reading, button, disclosure, para};
use viewtree::node::foreign;
use viewtree::{Node, Program, view};

use crate::runtime::Gate;
use crate::zone::{Offset, when};

/// What the page reads, as the host last read it.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Shown {
    /// The box's overview.
    pub overview: Overview,
    /// The Inbox's open cards, on their way while the conversations sync.
    pub rows: Reading<Vec<Card>>,
    /// Whether this login writes: the acts are offered only then.
    pub gate: Gate,
    /// Where Ask goes, printed by the shell's router; `None` for a login with no actor.
    pub ask: Option<String>,
    /// This device, or `None` when the browser answers nothing for itself.
    pub device: Option<Device>,
    /// The owner's zone as offsets from UTC, oldest first, each from the instant it starts.
    pub offsets: Vec<Offset>,
}

/// One proposal card, by its reference, with what this page decides from it.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Card {
    /// `proposal:<conversation>/<n>`, as a line's `cards` names it.
    pub reference: String,
    /// What it does.
    pub act: CardAct,
    /// Whether it still asks: standing, or tapped and failed.
    pub stands: Stands,
    /// Changes whenever the card would draw differently, so its part is told.
    pub version: String,
}

/// The acts of a card this page tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardAct {
    /// Connect a model by signing in, or by a code.
    SignIn,
    /// Store a secret.
    Secret,
    /// Allow an origin.
    Grant,
    /// Anything else.
    Other,
}

/// Whether a card still asks something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Stands {
    /// It asks.
    Open,
    /// It is settled.
    Settled,
}

/// What this browser keeps and who it signs as (`web/src/app/device.ts`'s `Report`).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Device {
    /// Whether the browser may evict what it keeps.
    pub persist: Persist,
    /// How much of the render budget is spent.
    pub usage: Usage,
    /// The budget, in bytes.
    pub budget: f64,
    /// Who it signs as.
    pub signing: Signing,
}

/// Whether storage is persistent.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Persist {
    /// The browser cannot say.
    Unsupported,
    /// Persistent.
    Granted,
    /// Best-effort.
    Denied,
}

/// The rendered text kept.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct Usage {
    /// Bytes.
    pub bytes: f64,
    /// Pieces.
    pub count: u32,
}

/// Who this device writes as, by its key.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Signing {
    /// It has not read the box's root yet.
    Unheard,
    /// This login only reads.
    Reads,
    /// The browser cannot sign.
    Unsupported {
        /// Why.
        e: String,
    },
    /// Its key could not be read.
    Unreadable {
        /// Why.
        e: String,
    },
    /// It signs with this key.
    Keyed {
        /// The key.
        key: String,
        /// Whether the chain trusts it.
        standing: KeyStanding,
    },
}

/// A key's standing on the chain.
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

/// An act the page asks the world for; its button is disabled until it is answered.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Act {
    /// Open a conversation of its own on a template and go to it.
    Begin {
        /// The template.
        template: String,
    },
    /// File this device's card for a trusted device to tap.
    AskTrust {
        /// This device's key.
        key: String,
    },
    /// Make a new key for this device.
    Renew,
    /// Send the pairing code as typed.
    Pair {
        /// The code.
        code: String,
    },
}

/// What an act came to.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Outcome {
    /// It was done.
    Done,
    /// The box refused it, in its words.
    Refused {
        /// The words.
        words: String,
    },
}

/// What the host hands the page.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Input {
    /// A new reading.
    Shown(Box<Shown>),
    /// An act answered.
    Answered {
        /// Which.
        act: Act,
        /// What it came to.
        outcome: Outcome,
    },
}

/// What the page asks of the world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Effect {
    /// Perform an act and answer what it came to.
    Run {
        /// The act.
        act: Act,
    },
    /// Say something briefly.
    Toast {
        /// What.
        words: String,
    },
    /// Read what the page reads again.
    Again,
}

/// The pairing field: closed (its button) or open with the words typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    /// The button that opens it.
    Closed,
    /// The field, holding the words typed.
    Open {
        /// As typed.
        code: String,
    },
}

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    shown: Shown,
    pairing: Pairing,
    running: BTreeSet<Act>,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    /// From the host.
    Host(Box<Input>),
    /// A tap that asks the world for an act.
    Run(Act),
    /// The pairing field opened, closed or edited.
    Pairing(Pairing),
}

/// The page as a program.
#[derive(Debug)]
pub enum Settings {}

impl Program for Settings {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Shown;

    fn init(shown: Shown) -> (Model, Vec<Effect>) {
        (
            Model {
                shown,
                pairing: Pairing::Closed,
                running: BTreeSet::new(),
            },
            Vec::new(),
        )
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => match *input {
                Input::Shown(shown) => {
                    m.shown = *shown;
                    Vec::new()
                }
                Input::Answered { act, outcome } => {
                    m.running.remove(&act);
                    let paired = matches!((&act, &outcome), (Act::Pair { .. }, Outcome::Done));
                    if paired {
                        m.pairing = Pairing::Closed;
                    }
                    let words = match outcome {
                        Outcome::Done => paired.then(|| "This device is trusted.".to_owned()),
                        Outcome::Refused { words } => Some(words),
                    };
                    let again = paired || matches!(act, Act::AskTrust { .. } | Act::Renew);
                    words
                        .map(|words| Effect::Toast { words })
                        .into_iter()
                        .chain(again.then_some(Effect::Again))
                        .collect()
                }
            },
            Msg::Run(act) => m
                .running
                .insert(act.clone())
                .then_some(Effect::Run { act })
                .into_iter()
                .collect(),
            Msg::Pairing(p) => {
                m.pairing = p;
                Vec::new()
            }
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let here = view! { Div "settingshere" [m.shown.device.as_ref().map(|d| vec![view! { H2 ["This device"] }, device(m, d)])] };
        let first = matches!(
            &m.shown.device,
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
        let ask =
            m.shown.ask.as_ref().map(
                |href| view! { A "act act-ask" {Href => href} ["Ask to add or change anything"] },
            );
        let groups = m
            .shown
            .overview
            .groups
            .iter()
            .filter(|g| !g.lines.is_empty());
        view! { Div "settingsview" [
            before,
            view! { Div "settingsask" [ask] },
            match &m.shown.rows {
                Reading::Loading => Some(para("settingsmeta settingscards", "The Inbox's cards are on their way.")),
                Reading::Error { e } => Some(para("settingshealth settingshealth-failing settingscards", format!("The Inbox's cards could not be read: {e}"))),
                Reading::Val { .. } => None,
            },
            offered(m),
            view! { Div "settingsbody" => groups.map(|g| (key(g.group).to_owned(), group(m, g))) },
            after,
        ] }
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
    match &m.shown.rows {
        Reading::Val { v } => l
            .cards
            .iter()
            .flat_map(|c| v.iter().filter(move |r| &r.reference == c))
            .collect(),
        Reading::Loading | Reading::Error { .. } => Vec::new(),
    }
}

/// A line's cards; a link's card and the grant its origin needs, when both stand, as one part.
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

/// A standing in a sentence, whether it asks something of the owner, and the command a terminal
/// runs for it; `None` for one in place with nothing to say.
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
    let said = health(&l.standing, &|t| when(&m.shown.offsets, t));
    let (asks, words, command) = said.map_or((false, None, None), |(a, w, c)| (a, Some(w), c));
    let (line, health) = if asks {
        (
            "settingsline settingsline-failing",
            "settingshealth settingshealth-failing",
        )
    } else {
        ("settingsline", "settingshealth")
    };
    // Keyed, so a card arriving or leaving moves no other: the line's own parts under fixed keys.
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
    let parts = head
        .into_iter()
        .flatten()
        .map(|(k, n)| (k.to_owned(), n))
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
    match m.shown.gate {
        Gate::Writes => button(
            &format!("act settingsoffer settingsoffer-{}", x.template),
            &capital(&x.title),
            Msg::Run(act),
            busy,
        ),
        Gate::Reads => para("settingsmeta settingsoffer", capital(&x.title)),
    }
}

fn offered(m: &Model) -> Option<Node<Msg>> {
    let mut groups: Vec<(Group, Vec<&Offer>)> = Vec::new();
    for x in &m.shown.overview.offers {
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
