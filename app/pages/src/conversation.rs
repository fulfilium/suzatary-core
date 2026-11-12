//! One conversation, on its page or in the panel beside another page.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use suzatary_api::wire::Frame;
use suzatary_base::reference::Reference;
use suzatary_core::secretary::erasure::Erasure;
use suzatary_core::secretary::store::ConversationState;
use suzatary_core::wire::topics::{
    ConversationPayload, ConversationsPayload, ProposalRow, Receipt, Speaker, TurnGone, TurnRow,
    VerbOutcome,
};
use ts_rs::TS;
use viewtree::Attr::{AriaLabel, Class, Data, Hidden, Href, Maxlength, Title, Type, Value};
use viewtree::kit::{Reading, para};
use viewtree::{Key, Node, Program, view};

use crate::answer;
use crate::card::{self, Cards, Drawn, Placed, Titles};
use crate::chats::{self, Searching};
use crate::chrome::{self, Chips, Chrome, Masthead, Shape, Status, link, mark, prose};
use crate::composer::{self, Composer};
use crate::intents::{Intents, Shown};
use crate::part::Who;
use crate::places::Place;
use crate::runtime::{
    self, Act, Answer, Context, Local, Outcome, Question, Questioning, Reach, Rounding, Said,
    Scroll, Secret, Window, answered, read,
};
use crate::slices::Slices;
use crate::zone;

/// How many turns the core's window holds when asked for none (its `DEFAULT_TURN_WINDOW`).
const WINDOW: u32 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "ConversationAt")]
pub enum At {
    Page {
        id: String,
        refs: Vec<String>,
        focus: bool,
        /// A search hit's turn, where the first window ends.
        at: Option<u32>,
    },
    /// Beside another page: the default conversation's rounds about that page's reference.
    Panel { about: String },
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "ConversationFlags")]
pub struct Flags {
    pub context: Context,
    pub at: At,
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "ConversationHold")]
pub enum Hold {
    Window,
    /// A streaming block's render, by its sha.
    Block(String),
    Card(card::Key),
    Intents,
    Titles,
    Draft,
    Scroll,
    Chats,
    Found,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "ConversationAsk")]
pub enum Ask {
    Said(Box<Spoken>),
    Round,
    /// By the title it sent.
    Rename {
        title: String,
    },
    Forget(u64),
    Erase,
    Card(card::Asking),
    Composer(composer::Asking),
    Slice,
    Search(chats::Asking),
}

/// A question as said, handed back to the composer if the box refuses it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct Spoken {
    pub text: String,
    pub references: Vec<String>,
    pub files: Vec<String>,
    /// Whether it edited the last question.
    pub edit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "ConversationStream")]
pub enum Stream {
    Conversation,
}

pub(crate) type Effect = runtime::Effect<Hold, Ask, Stream>;
pub type Input = runtime::Input<Hold, Ask, Stream>;

/// A round as the worker groups it, by turn and card indices.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Shape_ {
    /// The opening's index, else the first trace turn's.
    pub(crate) key: usize,
    pub(crate) start_index: usize,
    /// The opening turn, `None` for a round the model opened unasked.
    pub(crate) opening: Option<usize>,
    pub(crate) trace: Vec<usize>,
    /// The trailing model turn with words.
    pub(crate) answer: Option<usize>,
    pub(crate) proposals: Vec<usize>,
    /// Turns a rewind set aside.
    pub(crate) earlier: Vec<usize>,
    /// Whether a day's separator (`label`) stands above it.
    pub(crate) marked: bool,
    pub(crate) label: String,
}

/// `grouped.Window`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(rename = "GroupedWindow")]
pub(crate) struct Grouped {
    #[serde(flatten)]
    pub(crate) payload: ConversationPayload,
    pub(crate) rounds: Vec<Shape_>,
}

/// `Local::Window`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Windowed {
    Read {
        payload: Box<Grouped>,
    },
    /// Lines of it are not on this device and could not be fetched.
    NotHere {
        lines: u32,
        why: String,
    },
    /// The device holds no line of it, and the box has not said whether it holds one.
    Unheard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Landing {
    Thinking,
    Working(String, u32),
    Streaming(Vec<(String, String)>, String),
    Done,
}

impl Landing {
    /// A question the box holds thinks from the moment it is seen; a running round keeps its words.
    fn settle(self, s: ConversationState) -> Landing {
        match (s, self) {
            (ConversationState::Idle, _) => Landing::Done,
            (_, Landing::Done) => Landing::Thinking,
            (_, a) => a,
        }
    }

    /// A block closes only the prefix of the tail it names; any other frame changes nothing.
    fn step(self, f: Frame) -> Landing {
        match (f, self) {
            (Frame::State { state }, a) => a.settle(state),
            (Frame::Working { verb }, Landing::Working(_, n)) => Landing::Working(verb, n + 1),
            (
                Frame::Working { verb },
                Landing::Thinking | Landing::Streaming(..) | Landing::Done,
            ) => Landing::Working(verb, 1),
            (Frame::Delta { text }, Landing::Streaming(blocks, tail)) => {
                Landing::Streaming(blocks, tail + &text)
            }
            (Frame::Delta { text }, Landing::Thinking | Landing::Working(..) | Landing::Done) => {
                Landing::Streaming(Vec::new(), text)
            }
            (Frame::Block { n, sha, source }, Landing::Streaming(mut blocks, tail))
                if n == blocks.len() && tail.starts_with(&source) =>
            {
                let rest = tail[source.len()..].to_owned();
                blocks.push((sha, source));
                Landing::Streaming(blocks, rest)
            }
            (Frame::Block { .. }, a) => a,
            (Frame::Turn, _) => Landing::Done,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Newest,
    /// Questions asked whose turns are not known yet, as many as are on their way.
    Asking(u32),
    Question(usize),
    Drifted,
}

#[derive(Debug, Clone)]
enum Held {
    Loading,
    Failed(String),
    NotHere(u32, String),
    Unheard,
    Loaded {
        grouped: Box<Grouped>,
        problem: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    id: String,
    at: At,
    held: Held,
    /// Monotone: once nothing about the reference is found, the whole conversation is drawn.
    fallback: bool,
    asked: Window,
    keep: bool,
    landing: Landing,
    blocks: BTreeMap<String, Reading<String>>,
    anchor: Anchor,
    held_at: Option<usize>,
    shown: Slices,
    /// Where the box goes once the window is drawn whole.
    placing: Option<Scroll>,
    pill: bool,
    composer: Composer,
    cards: Cards,
    intents: Intents,
    titles: Titles,
    erasing: Option<usize>,
    renaming: Option<String>,
    sent: Option<String>,
    now: i64,
    chats: chats::List,
    search: Searching,
    told: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Composer(composer::Ev),
    Card(card::Key, card::Ev),
    Quote(String),
    Earlier,
    Pill,
    Stop,
    Regenerate,
    Edit,
    /// Erase pressed on a turn, or its confirm closed.
    Erasing(Option<usize>),
    Erase(usize),
    Forget(u64),
    Rename,
    Renamed(String),
    /// The rename asked with the field's words, or let go.
    Named(Option<String>),
    Copy(String),
    Chats(chats::Ev),
    Searched(String),
}

#[derive(Debug)]
pub enum Conversation {}

fn about(at: &At) -> Option<&str> {
    match at {
        At::Panel { about } if !about.is_empty() => Some(about),
        At::Panel { .. } | At::Page { .. } => None,
    }
}

fn window(m: &Model) -> Effect {
    Effect::Watch {
        ask: Hold::Window,
        local: Local::Window {
            id: m.id.clone(),
            window: m.asked.clone(),
        },
    }
}

/// The newest window holding every turn the page holds, or `older` turns further back too.
fn asking(m: &Model, older: u32) -> Window {
    let about = if m.fallback {
        None
    } else {
        about(&m.at).map(str::to_owned)
    };
    let Held::Loaded { grouped, .. } = &m.held else {
        return Window {
            limit: None,
            from: None,
            before: None,
            about,
        };
    };
    let p = &grouped.payload;
    let held = u32::try_from(p.turns.len()).unwrap_or(u32::MAX);
    let limit = Some(WINDOW.max(held + older));
    let first = u32::try_from(p.first).unwrap_or(u32::MAX);
    match about {
        Some(a) => Window {
            limit,
            from: None,
            before: None,
            about: Some(a),
        },
        None => Window {
            limit,
            from: Some(first.saturating_sub(older)),
            before: None,
            about: None,
        },
    }
}

fn writes(m: &Model) -> bool {
    matches!(m.context.reach, Reach::Writes)
}

fn payload(m: &Model) -> Option<&ConversationPayload> {
    match &m.held {
        Held::Loaded { grouped, .. } => Some(&grouped.payload),
        Held::Loading | Held::Failed(_) | Held::NotHere(..) | Held::Unheard => None,
    }
}

fn total(m: &Model) -> usize {
    payload(m).map_or(0, |p| p.total)
}

fn running(m: &Model) -> bool {
    payload(m).is_some_and(|p| p.state != ConversationState::Idle)
}

/// The question the box would ask again: what ↑ and Edit bring back.
fn again(m: &Model) -> Option<&TurnRow> {
    let p = payload(m)?;
    if p.state != ConversationState::Idle {
        return None;
    }
    let n = p.again?;
    p.turns
        .iter()
        .find(|t| t.index == n && matches!(t.who, Speaker::Person { .. }))
}

impl Program for Conversation {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let (id, refs, focus, first) = match &flags.at {
            At::Page {
                id,
                refs,
                focus,
                at,
            } => (id.clone(), refs.clone(), *focus, *at),
            At::Panel { about } => (
                "default".to_owned(),
                if about.is_empty() {
                    Vec::new()
                } else {
                    vec![about.clone()]
                },
                false,
                None,
            ),
        };
        let (composer, draft) = Composer::of(&id, &refs);
        let mut m = Model {
            context: flags.context,
            at: flags.at,
            held: Held::Loading,
            fallback: false,
            asked: Window {
                limit: None,
                from: None,
                before: None,
                about: None,
            },
            keep: false,
            landing: Landing::Done,
            blocks: BTreeMap::new(),
            anchor: Anchor::Newest,
            held_at: None,
            shown: Slices::default(),
            placing: None,
            pill: false,
            composer,
            cards: Cards::default(),
            intents: Intents::default(),
            titles: Titles::new(),
            erasing: None,
            renaming: None,
            sent: None,
            now: flags.now,
            chats: chats::List::default(),
            search: Searching::default(),
            told: None,
            id,
        };
        m.asked = match first {
            Some(turn) => Window {
                limit: None,
                from: None,
                before: Some(turn + 1),
                about: None,
            },
            None => asking(&m, 0),
        };
        let mut effects = vec![
            window(&m),
            Effect::Listen {
                ask: Stream::Conversation,
                path: format!(
                    "/api/secretary/conversations/{}/stream",
                    runtime::component(&m.id)
                ),
            },
            Effect::Watch {
                ask: Hold::Intents,
                local: Local::Intents,
            },
            Effect::Watch {
                ask: Hold::Titles,
                local: Local::Titles,
            },
            Effect::Watch {
                ask: Hold::Scroll,
                local: Local::Scroll,
            },
            Effect::Watch {
                ask: Hold::Draft,
                local: draft,
            },
        ];
        if m.context.wide && matches!(m.at, At::Page { .. }) {
            effects.push(Effect::Watch {
                ask: Hold::Chats,
                local: Local::Conversations,
            });
        }
        if focus && writes(&m) {
            effects.push(Effect::Focus {
                field: "compose".to_owned(),
            });
        }
        (m, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => host(m, *input),
            Msg::Composer(ev) => {
                let outs = m.composer.update(ev);
                composed(m, outs)
            }
            Msg::Card(key, ev) => {
                let Some(row) = row(m, &key).cloned() else {
                    return Vec::new();
                };
                let outs = m.cards.update(&key, &row, ev);
                carded(m, outs)
            }
            Msg::Quote(text) => {
                let outs = m.composer.quote(&text);
                composed(m, outs)
            }
            Msg::Earlier => {
                m.asked = asking(m, WINDOW);
                m.keep = true;
                vec![window(m)]
            }
            Msg::Pill => {
                m.pill = false;
                m.anchor = Anchor::Newest;
                vec![Effect::Scroll { to: Scroll::End }]
            }
            Msg::Stop => stop(m),
            Msg::Regenerate => vec![round(m, Rounding::Regenerate)],
            Msg::Edit => match again(m).cloned() {
                Some(q) => {
                    let outs = m.composer.edit(&q);
                    composed(m, outs)
                }
                None => Vec::new(),
            },
            Msg::Erasing(turn) => {
                m.erasing = turn;
                Vec::new()
            }
            Msg::Erase(turn) => {
                m.erasing = None;
                vec![Effect::Act {
                    ask: Ask::Erase,
                    act: Act::Erase {
                        conversation: m.id.clone(),
                        turns: vec![u32::try_from(turn).unwrap_or(u32::MAX)],
                    },
                    secret: Secret::None,
                    told: runtime::Told::Held,
                }]
            }
            Msg::Forget(fact) => vec![Effect::Act {
                ask: Ask::Forget(fact),
                act: Act::Forget {
                    fact: suzatary_core::secretary::memory::FactId(fact),
                },
                secret: Secret::None,
                told: runtime::Told::Held,
            }],
            Msg::Rename => {
                let stored = m
                    .intents
                    .renamed(&m.id)
                    .map(str::to_owned)
                    .or_else(|| payload(m).map(|p| p.title.clone()))
                    .unwrap_or_default();
                m.renaming = Some(stored);
                vec![Effect::Focus {
                    field: "rename".to_owned(),
                }]
            }
            Msg::Renamed(words) => {
                if m.renaming.is_some() {
                    m.renaming = Some(words);
                }
                Vec::new()
            }
            Msg::Named(None) => {
                m.renaming = None;
                Vec::new()
            }
            Msg::Named(Some(title)) => vec![rename(m, title)],
            Msg::Copy(text) => vec![Effect::Copy { text }],
            Msg::Chats(ev) => {
                m.chats.update(ev);
                Vec::new()
            }
            Msg::Searched(words) => m.search.typed(words, Ask::Search),
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let transcript = transcript(m);
        let compose =
            composer::view(&m.composer, &m.context.reach, m.context.wide).map(Msg::Composer);
        match &m.at {
            At::Panel { about } => view! { Div "panelbody" [
                view! { Div "panelheader" [panel_header(m, about)] },
                view! { Div "panelscroll" {Data("box") => "panel"} [transcript] },
                view! { Div "panelcompose" [compose] },
            ] },
            At::Page { .. } => {
                let chrome = Chrome {
                    class: "conversation",
                    title: title(m),
                    masthead: Masthead::Page,
                    width: crate::chrome::Width::Column,
                    back: None,
                    legend: None,
                    now: vec![
                        view! { P "legend memorylink" [link("", Place::Memory, "What the secretary remembers")] },
                    ],
                    shape: Shape::Turns,
                };
                let page =
                    chrome::page_headed(chrome, head(m), Status::Quiet, vec![transcript, compose]);
                view! { Div "chatswide" [column(m), page] }
            }
        }
    }
}

fn composed(m: &mut Model, outs: Vec<composer::Out>) -> Vec<Effect> {
    let (mut effects, then) = composer::effects(outs, Ask::Composer);
    for t in then {
        match t {
            composer::Then::Stop => effects.extend(stop(m)),
            composer::Then::Up => {
                if let Some(q) = again(m).cloned() {
                    let more = m.composer.edit(&q);
                    effects.extend(composed(m, more));
                }
            }
            composer::Then::Send(sent) => effects.extend(say(m, sent)),
        }
    }
    effects
}

fn rename(m: &mut Model, title: String) -> Effect {
    m.renaming = Some(title.clone());
    Effect::Act {
        ask: Ask::Rename {
            title: title.clone(),
        },
        act: Act::Rename {
            conversation: m.id.clone(),
            title,
        },
        secret: Secret::None,
        told: runtime::Told::Answered,
    }
}

fn say(m: &mut Model, sent: composer::Sent) -> Vec<Effect> {
    m.anchor = match m.anchor {
        Anchor::Asking(n) => Anchor::Asking(n + 1),
        Anchor::Newest | Anchor::Question(_) | Anchor::Drifted => Anchor::Asking(1),
    };
    let edit = sent.mode == composer::Mode::Editing;
    let spoken = Spoken {
        text: sent.text.clone(),
        references: sent.references.clone(),
        files: sent.files.iter().map(|f| f.id.clone()).collect(),
        edit,
    };
    m.sent = Some(sent.text.clone());
    vec![Effect::Act {
        ask: Ask::Said(Box::new(spoken)),
        act: Act::Question {
            kind: if edit {
                Questioning::Edit
            } else {
                Questioning::Say
            },
            question: Question {
                conversation: m.id.clone(),
                after: u32::try_from(total(m)).unwrap_or(u32::MAX),
                text: sent.text,
                references: sent.references,
                files: sent.files,
            },
        },
        secret: Secret::None,
        told: runtime::Told::Held,
    }]
}

fn stop(m: &Model) -> Vec<Effect> {
    if writes(m) && running(m) {
        vec![round(m, Rounding::Stop)]
    } else {
        Vec::new()
    }
}

fn round(m: &Model, kind: Rounding) -> Effect {
    Effect::Act {
        ask: Ask::Round,
        act: Act::Round {
            kind,
            conversation: m.id.clone(),
            after: u32::try_from(total(m)).unwrap_or(u32::MAX),
        },
        secret: Secret::None,
        told: runtime::Told::Held,
    }
}

fn row<'a>(m: &'a Model, key: &card::Key) -> Option<&'a ProposalRow> {
    payload(m)?
        .proposals
        .iter()
        .find(|p| p.index == key.index as usize)
}

fn carded(m: &mut Model, outs: Vec<card::Out>) -> Vec<Effect> {
    let (mut effects, replies) = card::effects(outs, Hold::Card, Ask::Card);
    for k in replies {
        let more = m.composer.attach(k.reference());
        effects.extend(composed(m, more));
    }
    effects
}

fn moved(m: &mut Model, to: Option<Scroll>) -> Vec<Effect> {
    to.map(|to| {
        m.pill = false;
        Effect::Scroll { to }
    })
    .into_iter()
    .collect()
}

fn host(m: &mut Model, input: Input) -> Vec<Effect> {
    match input {
        Input::Context(c) => {
            let wide = c.wide
                && !m.context.wide
                && matches!(m.at, At::Page { .. })
                && matches!(m.chats.rows, Reading::Loading);
            m.context = c;
            if wide {
                vec![Effect::Watch {
                    ask: Hold::Chats,
                    local: Local::Conversations,
                }]
            } else {
                Vec::new()
            }
        }
        Input::Held {
            ask: Hold::Window,
            reading,
            ..
        } => windowed(m, reading),
        Input::Held {
            ask: Hold::Block(sha),
            reading,
            ..
        } => {
            m.blocks
                .insert(sha, read::<Rendered>(reading).map(|r| r.html));
            Vec::new()
        }
        Input::Held {
            ask: Hold::Card(key),
            reading,
            ..
        } => {
            if let Reading::Val { v } = read::<serde_json::Value>(reading) {
                m.cards.held(key, v);
            }
            Vec::new()
        }
        Input::Held {
            ask: Hold::Intents,
            reading,
            ..
        } => {
            if let Reading::Val { v } = read(reading) {
                m.intents = v;
            }
            Vec::new()
        }
        Input::Held {
            ask: Hold::Titles,
            reading,
            ..
        } => {
            if let Reading::Val { v } = read(reading) {
                m.titles = v;
            }
            Vec::new()
        }
        Input::Held {
            ask: Hold::Draft,
            reading,
            ..
        } => {
            m.composer.held(reading);
            Vec::new()
        }
        Input::Held {
            ask: Hold::Scroll,
            reading,
            ..
        } => {
            scrolled(m, reading);
            Vec::new()
        }
        Input::Held {
            ask: Hold::Chats,
            reading,
            ..
        } => {
            m.chats.rows = read::<ConversationsPayload>(reading).map(|p| p.conversations);
            Vec::new()
        }
        Input::Held {
            ask: Hold::Found,
            reading,
            ..
        } => {
            m.search.found(reading);
            Vec::new()
        }
        Input::Heard { frame, .. } => heard(m, &frame),
        // A stream that opens again may mean the box answers again.
        Input::Opened { .. } => match m.held {
            Held::NotHere(..) | Held::Unheard => vec![window(m)],
            Held::Loading | Held::Failed(_) | Held::Loaded { .. } => Vec::new(),
        },
        Input::Answered { ask, answer } => asked(m, ask, answer),
    }
}

fn heard(m: &mut Model, frame: &str) -> Vec<Effect> {
    match serde_json::from_str::<serde_json::Value>(frame).and_then(serde_json::from_value::<Frame>)
    {
        Ok(f) => {
            // A written turn comes with the replicas, pulled now.
            let turned = matches!(f, Frame::Turn);
            let landing = std::mem::replace(&mut m.landing, Landing::Done).step(f);
            let mut effects: Vec<Effect> = match &landing {
                Landing::Streaming(blocks, _) => blocks
                    .iter()
                    .filter(|(sha, _)| !m.blocks.contains_key(sha))
                    .map(|(sha, _)| Effect::Hold {
                        ask: Hold::Block(sha.clone()),
                        path: format!(
                            "/api/secretary/conversations/{}/blocks/{}",
                            runtime::component(&m.id),
                            runtime::component(sha)
                        ),
                    })
                    .collect(),
                Landing::Thinking | Landing::Working(..) | Landing::Done => Vec::new(),
            };
            if let Landing::Done = landing {
                m.blocks.clear();
            }
            m.landing = landing;
            if turned {
                effects.push(Effect::Sync);
            }
            effects
        }
        Err(_) => Vec::new(),
    }
}

fn scrolled(m: &mut Model, reading: Reading<Answer>) {
    if let Reading::Val { v } = read::<runtime::Scrolled>(reading)
        && v.by == runtime::Mover::Reader
    {
        m.anchor = if v.at == runtime::Place::Bottom {
            Anchor::Newest
        } else {
            Anchor::Drifted
        };
        if m.anchor == Anchor::Newest {
            m.pill = false;
        }
    }
}

/// `BlockRender`.
#[derive(Debug, Clone, Deserialize)]
struct Rendered {
    html: String,
}

fn windowed(m: &mut Model, reading: Reading<Answer>) -> Vec<Effect> {
    let w = match read::<Windowed>(reading) {
        Reading::Loading => return Vec::new(),
        Reading::Error { e } => {
            match &mut m.held {
                Held::Loaded { problem, .. } => *problem = Some(e),
                Held::Loading | Held::Failed(_) | Held::NotHere(..) | Held::Unheard => {
                    m.held = Held::Failed(e);
                }
            }
            return tell(m);
        }
        Reading::Val { v } => v,
    };
    let mut effects = Vec::new();
    match w {
        Windowed::NotHere { lines, why } => match &mut m.held {
            Held::Loaded { problem, .. } => *problem = Some(not_here(lines, &why)),
            Held::Loading | Held::Failed(_) | Held::NotHere(..) | Held::Unheard => {
                m.held = Held::NotHere(lines, why);
            }
        },
        Windowed::Unheard => {
            if !matches!(m.held, Held::Loaded { .. }) {
                m.held = Held::Unheard;
            }
        }
        Windowed::Read { payload } => {
            // Nothing about the reference: the whole conversation, from now on.
            if about(&m.at).is_some() && !m.fallback && payload.payload.turns.is_empty() {
                m.fallback = true;
                m.asked = asking(m, 0);
                return vec![window(m)];
            }
            // Turns are only appended, so a window holding fewer was read earlier.
            if total(m) > payload.payload.total {
                return Vec::new();
            }
            let state = payload.payload.state;
            m.landing = std::mem::replace(&mut m.landing, Landing::Done).settle(state);
            if !matches!(m.landing, Landing::Streaming(..)) {
                m.blocks.clear();
            }
            let n = payload.payload.total;
            let (before, fresh) = (rounds_held(m), m.held_at.is_none());
            let now = payload.rounds.len();
            m.held = Held::Loaded {
                grouped: payload,
                problem: None,
            };
            m.shown.read(fresh, before, now);
            // From its first turn on, so a new turn grows the window instead of sliding it.
            let pinned = asking(m, 0);
            if pinned.from != m.asked.from {
                m.asked = pinned;
                effects.push(window(m));
            }
            effects.extend(seen(m));
            // The question just sent is the last person turn the box holds once it holds it.
            if let Some(text) = m.sent.clone()
                && let Some(turn) = payload_turn(m, &text)
            {
                m.sent = None;
                effects.extend(sent(m, turn));
            }
            if m.keep {
                m.keep = false;
                effects.push(Effect::Scroll { to: Scroll::Keep });
            } else if m.held_at != Some(n) {
                let fresh = m.held_at.is_none();
                m.held_at = Some(n);
                let to = if fresh {
                    m.anchor = Anchor::Newest;
                    Some(Scroll::End)
                } else {
                    match m.anchor {
                        Anchor::Newest | Anchor::Asking(_) => Some(Scroll::End),
                        Anchor::Drifted => {
                            m.pill = true;
                            None
                        }
                        Anchor::Question(_) => None,
                    }
                };
                m.placing = to;
            }
        }
    }
    effects.extend(tell(m));
    effects.extend(drawn(m));
    effects
}

fn rounds_held(m: &Model) -> usize {
    match &m.held {
        Held::Loaded { grouped, .. } => grouped.rounds.len(),
        Held::Loading | Held::Failed(_) | Held::NotHere(..) | Held::Unheard => 0,
    }
}

/// Asks the next slice while rounds are left; after the last, moves the box.
fn drawn(m: &mut Model) -> Vec<Effect> {
    if !m.shown.whole(rounds_held(m)) {
        return if m.shown.ask(rounds_held(m)) {
            vec![Effect::Frame { ask: Ask::Slice }]
        } else {
            Vec::new()
        };
    }
    let to = m.placing.take();
    let mut effects = moved(m, to);
    effects.push(Effect::Mark {
        name: "transcript:placed".to_owned(),
    });
    effects
}

fn payload_turn(m: &Model, text: &str) -> Option<usize> {
    payload(m)?
        .turns
        .iter()
        .filter(|t| matches!(t.who, Speaker::Person { .. }) && t.text == text)
        .map(|t| t.index)
        .next_back()
}

fn sent(m: &mut Model, turn: usize) -> Vec<Effect> {
    match m.anchor {
        Anchor::Asking(n) if n > 1 => {
            m.anchor = Anchor::Asking(n - 1);
            Vec::new()
        }
        Anchor::Asking(_) => {
            m.anchor = Anchor::Question(turn);
            let key = round_of(m, turn);
            moved(m, key.map(|field| Scroll::Top { field }))
        }
        Anchor::Newest | Anchor::Question(_) | Anchor::Drifted => Vec::new(),
    }
}

fn round_of(m: &Model, turn: usize) -> Option<String> {
    let Held::Loaded { grouped, .. } = &m.held else {
        return None;
    };
    grouped
        .rounds
        .iter()
        .find(|r| r.opening == Some(turn) || r.trace.contains(&turn))
        .map(|r| format!("round/{}", r.key))
}

fn seen(m: &mut Model) -> Vec<Effect> {
    let Held::Loaded { grouped, .. } = &m.held else {
        return Vec::new();
    };
    let id = grouped.payload.id.clone();
    let rows = grouped.payload.proposals.clone();
    let mut outs = m.cards.seen(rows.iter().map(|r| (id.as_str(), r)));
    outs.extend(
        m.cards
            .settled(|k| rows.iter().find(|r| r.index == k.index as usize)),
    );
    card::effects(outs, Hold::Card, Ask::Card).0
}

/// What the panel's toggle reads: whether the conversation holds talk about the page beside it.
fn tell(m: &mut Model) -> Vec<Effect> {
    let At::Panel { .. } = m.at else {
        return Vec::new();
    };
    let talking = !m.fallback && payload(m).is_some_and(|p| !p.turns.is_empty());
    if m.told == Some(talking) {
        return Vec::new();
    }
    m.told = Some(talking);
    vec![Effect::Tell {
        said: serde_json::json!({ "talk": if talking { "Talking" } else { "Quiet" } }),
    }]
}

fn asked(m: &mut Model, ask: Ask, a: Answer) -> Vec<Effect> {
    let said = answered::<Outcome>(a.clone());
    let refused = match &said {
        Ok(Outcome::Answered {
            answer: Said::Refused { e } | Said::Lost { e },
            ..
        })
        | Err(e) => Some(e.clone()),
        Ok(
            Outcome::Answered {
                answer: Said::Taken,
                ..
            }
            | Outcome::Already,
        ) => None,
    };
    match ask {
        Ask::Said(spoken) => match refused {
            Some(e) => {
                m.sent = None;
                m.anchor = match m.anchor {
                    Anchor::Asking(n) if n > 1 => Anchor::Asking(n - 1),
                    Anchor::Asking(_) => Anchor::Newest,
                    a @ (Anchor::Newest | Anchor::Question(_) | Anchor::Drifted) => a,
                };
                let files = Vec::new();
                let outs = m.composer.failed(composer::Sent {
                    text: spoken.text,
                    references: spoken.references,
                    files,
                    mode: if spoken.edit {
                        composer::Mode::Editing
                    } else {
                        composer::Mode::Asking
                    },
                });
                let mut effects = composed(m, outs);
                effects.push(Effect::Toast { words: e });
                effects
            }
            None => match payload_turn(m, &spoken.text) {
                Some(turn) if m.sent.as_deref() == Some(&spoken.text) => {
                    m.sent = None;
                    sent(m, turn)
                }
                Some(_) | None => Vec::new(),
            },
        },
        Ask::Round | Ask::Erase => refused
            .map(|words| Effect::Toast { words })
            .into_iter()
            .collect(),
        Ask::Rename { title } => {
            if let Some(words) = refused {
                vec![Effect::Toast { words }]
            } else {
                if m.renaming.as_ref() == Some(&title) {
                    m.renaming = None;
                }
                Vec::new()
            }
        }
        Ask::Forget(fact) => refused
            .map(|e| Effect::Toast {
                words: format!("Could not forget fact {fact}: {e}"),
            })
            .into_iter()
            .collect(),
        Ask::Card(asking) => {
            let rows = payload(m).map(|p| p.proposals.clone()).unwrap_or_default();
            let outs = m.cards.asked(asking, a, |k| {
                rows.iter().find(|p| p.index == k.index as usize)
            });
            carded(m, outs)
        }
        Ask::Composer(c) => {
            let outs = m.composer.answered(c, a);
            composed(m, outs)
        }
        Ask::Slice => {
            m.shown.more(rounds_held(m));
            drawn(m)
        }
        Ask::Search(chats::Asking::Rested(n)) => m.search.rested(n, Hold::Found),
        Ask::Search(chats::Asking::Found) => Vec::new(),
    }
}

fn not_here(lines: u32, why: &str) -> String {
    format!(
        "Not on this device yet: {lines} {} of this conversation ({why}).",
        if lines == 1 { "line" } else { "lines" }
    )
}

fn title(m: &Model) -> String {
    match (m.intents.renamed(&m.id), &m.held) {
        (Some(t), _) => t.to_owned(),
        (None, Held::Loaded { grouped, .. }) => chats::named(&grouped.payload.title).to_owned(),
        (None, Held::Failed(_)) => "Conversation".to_owned(),
        (None, Held::Loading | Held::NotHere(..) | Held::Unheard) => String::new(),
    }
}

fn rename_key(Key { name, words, .. }: Key) -> Option<Msg> {
    match name.as_str() {
        "Enter" => Some(Msg::Named(Some(words))),
        "Escape" => Some(Msg::Named(None)),
        _ => None,
    }
}

/// The model's name is only a tooltip.
fn head(m: &Model) -> Node<Msg> {
    let model = payload(m).map_or(String::new(), |p| p.model.clone());
    let waiting = m.intents.renamed(&m.id).is_none() && matches!(m.held, Held::Loading);
    let heading = match &m.renaming {
        Some(typed) => {
            view! { Input "renamefield" {Type => "text", Maxlength => "80", Value => typed, AriaLabel => "Conversation name", Data("field") => "rename"} @input(Msg::Renamed) @keydown(rename_key) }
        }
        None => {
            view! { H1 {Class => if waiting { "pagetitle pagewaiting" } else { "pagetitle" }, Title => model} [title(m)] }
        }
    };
    let rename = (writes(m) && m.renaming.is_none()).then(
        || view! { Button "act act-rename" {Type => "button"} @click(Msg::Rename) ["Rename"] },
    );
    view! { Div "convhead" [heading, view! { Span "convwrites" [rename] }] }
}

fn panel_header(m: &Model, about: &str) -> Node<Msg> {
    let line = match &m.held {
        Held::Loaded { .. } if m.fallback => vec![
            view! { Span [format!("Nothing about {about} yet · ")] },
            view! { A {Href => "#/secretary/default"} ["whole conversation →"] },
        ],
        Held::Loaded { .. } => vec![
            view! { Span [if about.is_empty() { String::new() } else { format!("About {about} · ") }] },
            view! { A {Href => "#/secretary/default"} ["Whole conversation →"] },
        ],
        Held::Loading | Held::Failed(_) | Held::NotHere(..) | Held::Unheard => Vec::new(),
    };
    view! { Div "frameheader" [line] }
}

/// Shown at 62rem and up; a phone reads no list.
fn column(m: &Model) -> Node<Msg> {
    let today = zone::day_ms(&m.context.offsets, m.now);
    let body = match &m.search.search {
        chats::Search::On { q, found, .. } => chats::hits(q, found, &m.context.offsets, today),
        chats::Search::Off => chats::view(&m.chats, &m.id, &m.context.offsets, today, Msg::Chats),
    };
    view! { Aside "chatsaside" {AriaLabel => "Conversations"} [
        m.context.wide.then(|| vec![chats::head(&m.search, &m.context.reach, Msg::Searched), view! { Div "chatbody" [body] }]),
    ] }
}

fn speaker(t: &TurnRow) -> String {
    match &t.who {
        Speaker::Person { .. } => "You".to_owned(),
        Speaker::Model { .. } => "Secretary".to_owned(),
        Speaker::Act { .. } => "You did".to_owned(),
        Speaker::Box { .. } => "The box".to_owned(),
        Speaker::Verb {
            verb,
            outcome: VerbOutcome::Refused,
        } => format!("{verb} — refused"),
        Speaker::Verb { verb, .. } => verb.clone(),
    }
}

fn gone_why(g: &TurnGone) -> &'static str {
    match g {
        TurnGone::Asked { .. } => "asked",
        TurnGone::Forgot { .. } => "forgot",
        TurnGone::Missing => "missing",
        TurnGone::Withheld => "withheld",
    }
}

fn refused(t: &TurnRow) -> bool {
    matches!(
        t.who,
        Speaker::Verb {
            outcome: VerbOutcome::Refused,
            ..
        }
    )
}

fn queued(t: &TurnRow) -> bool {
    matches!(t.who, Speaker::Person { queued: true, .. })
}

/// A model turn with no words draws nothing; one whose words are gone draws why.
fn silent(t: &TurnRow) -> bool {
    matches!(t.who, Speaker::Model { .. }) && t.text.is_empty() && t.gone.is_none()
}

/// The turn is drawn erased once the replica holds the box's erasure.
fn erase(m: &Model, t: &TurnRow) -> Node<Msg> {
    let turn = u32::try_from(t.index).unwrap_or(u32::MAX);
    let shown = m.intents.about(
        |a| matches!(a, Act::Erase { conversation, turns } if *conversation == m.id && turns == &vec![turn]),
        |_| t.gone.is_some(),
    );
    let idle = || view! { Button "act act-erase" {Type => "button"} @click(Msg::Erasing(Some(t.index))) ["Erase"] };
    let parts = match shown {
        Shown::Pending | Shown::Taken => vec![chrome::inline_wait()],
        Shown::Failed { e, .. } => vec![para("problem", e), idle()],
        Shown::Absent if m.erasing == Some(t.index) => vec![
            view! { Span "proposalconfirm" [format!("Erase these words for good? {}", Erasure::for_good())] },
            view! { Button "act act-erase confirming" {Type => "button"} @click(Msg::Erase(t.index)) ["Erase"] },
            view! { Button "act act-keep" {Type => "button"} @click(Msg::Erasing(None)) ["Keep"] },
        ],
        Shown::Absent => vec![idle()],
    };
    view! { Div "turnerase" [parts] }
}

fn turn(m: &Model, t: &TurnRow, reply: bool, tucked: Option<Node<Msg>>) -> Node<Msg> {
    let class = format!(
        "turn turn-{}{}{}",
        Who::of(&t.who).name(),
        if refused(t) { " refused" } else { "" },
        if t.gone.is_some() { " gone" } else { "" }
    );
    let rich = match (&t.who, &t.gone) {
        (Speaker::Model { blocks }, None) if !blocks.is_empty() => Some(blocks),
        (
            Speaker::Model { .. }
            | Speaker::Person { .. }
            | Speaker::Verb { .. }
            | Speaker::Act { .. }
            | Speaker::Box { .. },
            _,
        ) => None,
    };
    let body = match (&t.gone, rich) {
        (Some(g), _) => {
            view! { Div "turntext turngone" {Data("gone") => gone_why(g)} [t.text.as_str()] }
        }
        (None, Some(blocks)) => answer::blocks(&t.text, blocks, Msg::Copy),
        (None, None) => prose(
            crate::part::Tag::Div,
            "turntext",
            &t.text,
            Some(crate::part::Who::of(&t.who)),
            Chips::Conversation(&m.id),
        ),
    };
    let (files, references): (&[suzatary_core::wire::topics::AttachmentRow], &[Reference]) =
        match &t.who {
            Speaker::Person {
                references,
                attachments,
                ..
            } => (attachments, references),
            Speaker::Act { references } | Speaker::Box { references } => (&[], references),
            Speaker::Model { .. } | Speaker::Verb { .. } => (&[], &[]),
        };
    let photos = (!files.is_empty()).then(|| {
        view! { Div "turnphotos" [files.iter().map(|f| {
            let a = composer::attachment(f);
            if f.private.is_empty() && f.media_type != "application/pdf" {
                view! { A {Href => Place::Attachment(&f.id).href()} [a] }
            } else {
                a
            }
        }).collect::<Vec<_>>()] }
    });
    let refs = (!references.is_empty()).then(|| {
        view! { Div "turnrefs" [references.iter().map(|r| view! { A {Class => format!("reflink turnref ref-{}", r.scheme()), Href => Place::Reference(r).href()} [r.to_string()] }).collect::<Vec<_>>()] }
    });
    let replies = reply
        && t.gone.is_none()
        && matches!(t.who, Speaker::Model { .. } | Speaker::Person { .. });
    let reply = replies.then(|| view! { Button "turnreply" {Type => "button"} @click(Msg::Quote(t.text.clone())) ["Reply"] });
    let erasable = writes(m) && t.gone.is_none() && !queued(t);
    let at_rest = !erasable
        || (m.erasing != Some(t.index)
            && {
                let turn = u32::try_from(t.index).unwrap_or(u32::MAX);
                m.intents.about(|a| matches!(a, Act::Erase { conversation, turns } if *conversation == m.id && turns == &vec![turn]), |_| t.gone.is_some()) == Shown::Absent
            });
    let mut items = rich
        .map(|b| answer::copies(&t.text, b, Msg::Copy))
        .unwrap_or_default();
    if erasable {
        items.push(erase(m, t));
    }
    items.extend(tucked);
    let menu = answer::more(items, !at_rest);
    let controls =
        (reply.is_some() || menu.is_some()).then(|| view! { Div "turncontrols" [reply, menu] });
    view! { Div {Class => class, Data("who") => Who::of(&t.who).name()} [
        view! { Div "turnwho" [speaker(t)] },
        body,
        photos,
        refs,
        controls,
    ] }
}

/// One outside the families prints as itself.
fn family(verb: &str) -> &str {
    match verb {
        "search" | "show" | "thread" => "mail",
        "todo_show" | "list_open_todos" | "todo_add" | "todo_update" => "todos",
        "context" | "briefing" | "briefing_delta" => "notes",
        v if v.starts_with("propose_") => "proposals",
        v => v,
    }
}

/// `41 s`, `2 min`, `2 min 5 s`.
fn elapsed(ms: i64) -> String {
    let seconds = ms / 1000;
    match (seconds / 60, seconds % 60) {
        (0, s) => format!("{s} s"),
        (min, 0) => format!("{min} min"),
        (min, s) => format!("{min} min {s} s"),
    }
}

/// `None` when nothing was checked.
fn fold_line(
    trace: &[&TurnRow],
    opening: Option<&TurnRow>,
    answer: Option<&TurnRow>,
) -> Option<(String, bool)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    let mut refused_any = false;
    for t in trace {
        if let Speaker::Verb { verb, outcome } = &t.who {
            refused_any |= *outcome == VerbOutcome::Refused;
            let f = family(verb).to_owned();
            match counts.iter_mut().find(|(g, _)| *g == f) {
                Some((_, n)) => *n += 1,
                None => counts.push((f, 1)),
            }
        }
    }
    if counts.is_empty() {
        return None;
    }
    let parts: Vec<String> = counts.iter().map(|(f, n)| format!("{f} ({n})")).collect();
    let mut label = format!("Checked {}", parts.join(", "));
    let opened = opening
        .or(trace.first().copied())
        .map_or("", |t| t.at.as_str());
    let closed = answer.map_or("", |t| t.at.as_str());
    if let (Some(a), Some(b)) = (zone::ms(opened), zone::ms(closed))
        && b >= a
    {
        label = format!("{label} · {}", elapsed(b - a));
    }
    Some((label, refused_any))
}

fn receipt(m: &Model, r: &Receipt) -> Node<Msg> {
    let (fact, state) = match r {
        Receipt::Standing { fact, .. } => (fact.0, "standing"),
        Receipt::Forgotten { fact } => (fact.0, "forgotten"),
        Receipt::Merged { fact, .. } => (fact.0, "merged"),
        Receipt::Unread { fact } => (fact.0, "unread"),
    };
    let words = match r {
        Receipt::Standing { text, .. } => view! { Span "receipttext" [text.as_str()] },
        Receipt::Forgotten { .. } | Receipt::Merged { .. } | Receipt::Unread { .. } => {
            view! { Span "receiptfact" [format!("fact {fact}")] }
        }
    };
    let tail = match r {
        Receipt::Forgotten { .. } => Some(vec![
            mark(crate::part::Mark::Diamond),
            view! { Span "receiptstate" ["forgotten"] },
        ]),
        Receipt::Merged { into, .. } => Some(vec![
            mark(crate::part::Mark::Diamond),
            view! { Span "receiptstate" [format!("merged into fact {}", into.0)] },
        ]),
        Receipt::Standing { .. } | Receipt::Unread { .. } if writes(m) => Some(vec![
            mark(crate::part::Mark::Diamond),
            view! { Button "act forget" {Type => "button"} @click(Msg::Forget(fact)) ["Forget"] },
        ]),
        Receipt::Standing { .. } | Receipt::Unread { .. } => None,
    };
    view! { P "receipt" {Data("fact") => fact.to_string(), Data("state") => state} [link("", Place::Memory, "Remembered"), ": ", words, tail] }
}

/// Each part by its own key, so an answer's HTML arriving keeps the cards beneath it.
fn round_parts(m: &Model, p: &ConversationPayload, s: &Shape_) -> Vec<(String, Node<Msg>)> {
    let at = |i: usize| p.turns.iter().find(|t| t.index == i);
    let reply = writes(m);
    let opening = s.opening.and_then(at);
    let trace: Vec<&TurnRow> = s.trace.iter().filter_map(|i| at(*i)).collect();
    let answer = s.answer.and_then(at);
    let earlier: Vec<&TurnRow> = s.earlier.iter().filter_map(|i| at(*i)).collect();
    let mut parts: Vec<(String, Node<Msg>)> = Vec::new();
    if let Some(o) = opening.filter(|t| !silent(t)) {
        parts.push(("opening".to_owned(), turn(m, o, reply, None)));
    }
    if !earlier.is_empty() {
        parts.push(("earlier".to_owned(), view! { Details "roundfold earlierattempt" [
            view! { Summary "roundfoldsummary" ["Earlier attempt"] },
            view! { Div "roundfoldbody" [earlier.iter().filter(|t| !silent(t)).map(|t| turn(m, t, false, None)).collect::<Vec<_>>()] },
        ] }));
    }
    let calls = fold_line(&trace, opening, answer).map(|(label, refused_any)| {
        view! { Details "roundfold" [
            view! { Summary "roundfoldsummary" [label, refused_any.then(|| view! { Span "roundfoldrefused" ["refused"] })] },
            view! { Div "roundfoldbody" [trace.iter().filter(|t| !silent(t)).map(|t| turn(m, t, reply, None)).collect::<Vec<_>>()] },
        ] }
    });
    let refused_any = fold_line(&trace, opening, answer).is_some_and(|(_, r)| r);
    let tuck = calls.is_some() && !refused_any && answer.is_some();
    let (trace_part, tucked) = if tuck { (None, calls) } else { (calls, None) };
    if let Some(c) = trace_part {
        parts.push(("trace".to_owned(), c));
    }
    for t in &trace {
        if let Speaker::Verb {
            outcome: VerbOutcome::Remembered { receipt: r },
            ..
        } = &t.who
        {
            parts.push((format!("m{}", t.index), receipt(m, r)));
        }
    }
    if let Some(a) = answer.filter(|t| !silent(t)) {
        parts.push(("answer".to_owned(), turn(m, a, reply, tucked)));
    }
    for index in &s.proposals {
        if let Some(row) = p.proposals.iter().find(|c| c.index == *index) {
            let d = Drawn {
                conversation: &p.id,
                row,
                placed: Placed::Conversation,
                reach: &m.context.reach,
                intents: &m.intents,
                titles: &m.titles,
                opened: false,
            };
            parts.push((
                format!("p{index}"),
                m.cards.view(d, Vec::new(), Vec::new(), Msg::Card),
            ));
        }
    }
    parts
}

fn rounds(m: &Model, p: &ConversationPayload, shapes: &[Shape_]) -> Node<Msg> {
    view! { Div "rounds" => shapes.iter().map(|s| {
        let day = s.marked.then(|| ("day".to_owned(), view! { Div "dayseparator" [s.label.as_str()] }));
        let round = view! { Div "round" {Data("field") => format!("round/{}", s.key)} => round_parts(m, p, s) };
        (s.key.to_string(), view! { Div "roundwrap" => day.into_iter().chain(std::iter::once(("round".to_owned(), round))) })
    }) }
}

fn label(l: &Landing) -> String {
    match l {
        Landing::Thinking => "Thinking · ".to_owned(),
        Landing::Working(verb, n) => {
            let p = phrase(verb);
            let mut cs = p.chars();
            let cap: String = cs
                .next()
                .map(|c| c.to_uppercase().chain(cs).collect())
                .unwrap_or_default();
            format!("{cap} · {n} checked · ")
        }
        Landing::Streaming(..) | Landing::Done => String::new(),
    }
}

/// An unlisted verb reads as its own name.
fn phrase(verb: &str) -> &str {
    match verb {
        "search" => "searching mail",
        "show" => "reading a message",
        "todo_show" | "list_open_todos" => "reading todos",
        "briefing_delta" => "reading what changed",
        "context" => "reading notes",
        "propose_email" => "drafting an email",
        "repos_list" => "reading repositories",
        "repo_tree" | "repo_read" => "reading a repository",
        "repo_grep" => "searching a repository",
        "propose_patch" => "drafting a change",
        "propose_coding_session" => "drafting a coding session",
        "drive_list" => "reading Drive",
        "drive_read" => "reading a document",
        "junk_digest" => "reading Junk",
        "junk_show" => "reading a Junk message",
        v => v,
    }
}

fn place(m: &Model) -> Node<Msg> {
    let writes = writes(m);
    let stop = writes && running(m);
    let offered = writes && again(m).is_some();
    let landing = match &m.landing {
        Landing::Thinking | Landing::Working(..) => {
            vec![view! { P "answerstate" [chrome::clock(&m.id, &label(&m.landing))] }]
        }
        Landing::Streaming(blocks, tail) => vec![
            view! { Div "turnwho" ["Secretary"] },
            view! { Div "turnanswer streaming" [
                blocks.iter().map(|(sha, source)| {
                    let html = match m.blocks.get(sha) {
                        Some(Reading::Val { v }) => Some(v.as_str()),
                        Some(Reading::Loading | Reading::Error { .. }) | None => None,
                    };
                    chrome::rich(html, source, "answerblock")
                }).collect::<Vec<_>>(),
                (!tail.is_empty()).then(|| view! { Div "answertail" [tail.as_str()] }),
            ] },
        ],
        Landing::Done => vec![view! { Div "answerdone" }],
    };
    let hidden = m.landing == Landing::Done && !stop && !offered;
    view! { Div "answerplace" {Hidden => hidden} [
        view! { Div "answerlanding" [landing] },
        view! { Div "answercontrols" [
            view! { Button "stopanswer" {Type => "button", Hidden => !stop} @click(Msg::Stop) ["Stop"] },
            view! { Button "againbutton" {Type => "button", Hidden => !offered} @click(Msg::Regenerate) ["Regenerate"] },
            view! { Button "againbutton" {Type => "button", Hidden => !offered} @click(Msg::Edit) ["Edit question"] },
        ] },
    ] }
}

fn transcript(m: &Model) -> Node<Msg> {
    type Parts = (
        Option<String>,
        Option<String>,
        Vec<Node<Msg>>,
        bool,
        Option<Node<Msg>>,
    );
    let (problem, nothere, unplaced, earlier, rounds_node): Parts = match &m.held {
        Held::Loading | Held::Unheard => (None, None, Vec::new(), false, None),
        Held::Failed(e) => (Some(e.clone()), None, Vec::new(), false, None),
        Held::NotHere(lines, why) => (None, Some(not_here(*lines, why)), Vec::new(), false, None),
        Held::Loaded { grouped, problem } => {
            let p = &grouped.payload;
            let unplaced = p
                .unplaced
                .iter()
                .map(|u| {
                    para(
                        "problem",
                        format!(
                            "proposal:{}/{} has no card: {}.",
                            p.id,
                            u.index,
                            crate::waiting::unheld(u.why)
                        ),
                    )
                })
                .collect();
            (
                problem.clone(),
                None,
                unplaced,
                p.first > 0,
                Some(rounds(
                    m,
                    p,
                    &grouped.rounds[grouped.rounds.len().saturating_sub(m.shown.shown())..],
                )),
            )
        }
    };
    let coming =
        matches!(m.held, Held::Loading | Held::Unheard).then(|| chrome::placeholder(Shape::Turns));
    let held = matches!(m.anchor, Anchor::Question(_));
    view! { Div "transcriptcontent" [
        coming,
        view! { P "problem" {Hidden => problem.is_none()} [problem] },
        view! { P "nothere" {Hidden => nothere.is_none()} [nothere] },
        view! { Div "unplaced" [unplaced] },
        view! { Button "earlierbtn" {Type => "button", Hidden => !earlier} @click(Msg::Earlier) ["Earlier…"] },
        view! { Div "turns" [
            rounds_node.unwrap_or_else(|| view! { Div "rounds" }),
            place(m),
            view! { Div "answerroom" {Data("room") => if held { "held" } else { "free" }} },
        ] },
        view! { Button "newanswerpill" {Type => "button", Hidden => !m.pill} @click(Msg::Pill) ["↓ new answer"] },
    ] }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;
    use viewtree::Runtime;

    use super::*;

    fn frame() -> impl Strategy<Value = Frame> {
        prop_oneof![
            prop::sample::select(vec!["a", "b\n\n", "c"])
                .prop_map(|t| Frame::Delta { text: t.to_owned() }),
            (
                0usize..3,
                prop::sample::select(vec!["a", "ab\n\n", "b\n\n"])
            )
                .prop_map(|(n, s)| Frame::Block {
                    n,
                    sha: s.to_owned(),
                    source: s.to_owned()
                }),
            Just(Frame::Working {
                verb: "search".to_owned()
            }),
            Just(Frame::State {
                state: ConversationState::Thinking
            }),
            Just(Frame::State {
                state: ConversationState::Idle
            }),
            Just(Frame::Turn),
        ]
    }

    proptest! {
        #[test]
        fn the_streamed_blocks_and_tail_are_the_words_said(frames in proptest::collection::vec(frame(), 0..24)) {
            let mut landing = Landing::Done;
            let mut said = String::new();
            for f in frames {
                if let Frame::Delta { text } = &f {
                    if !matches!(landing, Landing::Streaming(..)) {
                        said.clear();
                    }
                    said.push_str(text);
                }
                landing = landing.step(f);
                if let Landing::Streaming(blocks, tail) = &landing {
                    let closed: String = blocks.iter().map(|(_, s)| s.as_str()).collect();
                    prop_assert_eq!(format!("{closed}{tail}"), said.clone());
                }
            }
        }
    }

    fn window(turns: usize) -> Msg {
        let rows: Vec<_> = (0..turns)
            .map(|i| json!({"index": i, "text": format!("t{i}"), "at": "", "attempt": "current", "gone": null,
                "who": "person", "references": [], "attachments": [], "queued": false}))
            .collect();
        let rounds: Vec<_> = (0..turns)
            .map(|i| json!({"key": i, "startIndex": i, "opening": i, "trace": [], "answer": null, "proposals": [], "earlier": [], "marked": false, "label": ""}))
            .collect();
        let body = json!({"type": "Read", "payload": {"id": "c", "title": "", "model": "", "opened": "", "turns": rows,
            "proposals": [], "unplaced": [], "state": "idle", "first": 0, "total": turns, "again": null, "rounds": rounds}});
        Msg::Host(Box::new(Input::Held {
            ask: Hold::Window,
            reading: Reading::Val {
                v: Answer::Done { body },
            },
            behind: None,
        }))
    }

    fn scrolled(at: &str) -> Msg {
        let body = json!({"by": "reader", "at": at});
        Msg::Host(Box::new(Input::Held {
            ask: Hold::Scroll,
            reading: Reading::Val {
                v: Answer::Done { body },
            },
            behind: None,
        }))
    }

    fn scrolls(effects: &[Effect]) -> bool {
        effects.iter().any(|e| matches!(e, Effect::Scroll { .. }))
    }

    /// `msg` stepped, and every slice it asks for answered: what the page asks once it is drawn.
    fn stepped(rt: &mut Runtime<Conversation>, msg: Msg) -> Vec<Effect> {
        let slicing = |e: &Effect| matches!(e, Effect::Frame { ask: Ask::Slice });
        let mut out = Vec::new();
        let mut next = rt.step(msg).1;
        loop {
            let more = next.iter().any(slicing);
            out.extend(next.into_iter().filter(|e| !slicing(e)));
            if !more {
                return out;
            }
            next = rt
                .step(Msg::Host(Box::new(Input::Answered {
                    ask: Ask::Slice,
                    answer: Answer::Done {
                        body: serde_json::Value::Null,
                    },
                })))
                .1;
        }
    }

    fn page() -> Runtime<Conversation> {
        let context = Context {
            gate: runtime::Gate::Writes,
            reach: Reach::Writes,
            offsets: Vec::new(),
            wide: false,
            categories: vec!["home".to_owned(), "work".to_owned()],
        };
        let at = At::Page {
            id: "c".to_owned(),
            refs: Vec::new(),
            focus: false,
            at: None,
        };
        Runtime::<Conversation>::start(Flags {
            context,
            at,
            now: 0,
        })
        .0
    }

    /// A turn the box adds grows the held window instead of sliding the oldest out.
    #[test]
    fn the_window_is_held_from_its_first_turn() {
        let mut rt = page();
        let from = |effects: &[Effect]| {
            effects.iter().find_map(|e| {
                if let Effect::Watch {
                    local: Local::Window { window, .. },
                    ..
                } = e
                {
                    Some(window.from)
                } else {
                    None
                }
            })
        };
        assert_eq!(from(&rt.step(window(3)).1), Some(Some(0)));
        assert_eq!(from(&rt.step(window(4)).1), None, "asked once");
    }

    #[test]
    fn a_new_answer_never_moves_a_reader_who_scrolled_up() {
        let mut rt = page();
        assert!(
            scrolls(&stepped(&mut rt, window(1))),
            "the first window opens at the newest turn"
        );
        assert!(
            scrolls(&stepped(&mut rt, window(2))),
            "a reader at the end stays there"
        );
        rt.step(scrolled("above"));
        assert!(
            !scrolls(&stepped(&mut rt, window(3))),
            "a reader above is not moved"
        );
        assert!(rt.model().pill, "and is offered the way down");
        rt.step(scrolled("bottom"));
        assert!(!rt.model().pill);
        assert!(
            scrolls(&stepped(&mut rt, window(4))),
            "back at the end, the end is followed again"
        );
    }
}
