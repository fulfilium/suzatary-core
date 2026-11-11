//! The secretary: every conversation's rounds as one stream, filtered by the address's topics.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use suzatary_core::secretary::topics::provenance::Provenance;
use suzatary_core::secretary::topics::topic::Topic;
use suzatary_core::wire::topics::{
    RoundAsked, RoundTopic, StreamPayload, StreamRound, TopicInUse, TopicsPayload,
};
use ts_rs::TS;
use viewtree::Attr::{
    AriaLabel, AriaPressed, Autocapitalize, Autocomplete, Class, Data, Disabled, Href, Role,
    Spellcheck, Title, Type, Value,
};
use viewtree::kit::{Reading, para};
use viewtree::{Key, Node, Program, view};

use crate::answer;
use crate::chats::{self, Search, Searching};
use crate::chrome::{self, Chips, Chrome, Masthead, Shape, Status, link, mark, pie, prose};
use crate::part::Who;
use crate::places::Place;
use crate::runtime::{
    self, Act, Answer, Context, Local, Outcome, Reach, Said, Scroll, Secret, TopicWrite, answered,
    read,
};
use crate::slices::Slices;
use crate::zone::{self, recent};

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "SecretaryFlags")]
pub struct Flags {
    pub context: Context,
    pub topics: Vec<String>,
    /// Epoch milliseconds.
    #[ts(type = "number")]
    pub now: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "SecretaryHold")]
pub enum Hold {
    Stream,
    Found,
    Scroll,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "SecretaryAsk")]
pub enum Ask {
    /// Renamed from a name the filter holds, or another write.
    Topic(Wrote),
    Search(chats::Asking),
    /// The owner's next day.
    Day,
    Slice,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct Wrote {
    pub renamed: Option<(String, String)>,
    pub closes: Closes,
}

/// The sheet a write came from, closed when it is taken only if it still stands.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Closes {
    Nothing,
    Renaming { topic: String },
    Adding { target: String },
}

impl Closes {
    fn stands(&self, sheet: &Sheet) -> bool {
        match (self, sheet) {
            (Closes::Renaming { topic: a }, Sheet::Renaming { topic: b, .. })
            | (Closes::Adding { target: a }, Sheet::Adding { target: b, .. }) => a == b,
            (Closes::Nothing, Sheet::Closed | Sheet::Renaming { .. } | Sheet::Adding { .. })
            | (Closes::Renaming { .. }, Sheet::Closed | Sheet::Adding { .. })
            | (Closes::Adding { .. }, Sheet::Closed | Sheet::Renaming { .. }) => false,
        }
    }
}

pub(crate) type Effect = runtime::Effect<Hold, Ask>;
pub(crate) type Input = runtime::Input<Hold, Ask>;

/// `Local::Stream`.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct StreamRead {
    pub(crate) stream: StreamPayload,
    pub(crate) bar: TopicsPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Check {
    Blank,
    Fits(String),
    /// The rule it breaks, in the core's words.
    Breaks(String),
}

impl Check {
    fn of(text: &str) -> Check {
        if text.is_empty() {
            return Check::Blank;
        }
        match Topic::new(text) {
            Ok(_) => Check::Fits(text.to_owned()),
            Err(e) => Check::Breaks(e.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Sheet {
    Closed,
    Renaming { topic: String, typed: String },
    Adding { target: String, typed: String },
}

#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    chosen: Vec<String>,
    pages: u32,
    stream: Reading<StreamRead>,
    /// The filter and the pages drawn last, which decide where a new reading leaves the box.
    drawn: Option<(Vec<String>, u32)>,
    /// How many of the rounds read are drawn, and where the box goes once they all are.
    shown: Slices,
    placing: Option<Scroll>,
    at_end: bool,
    sheet: Sheet,
    search: Searching,
    now: i64,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input>),
    Copy(String),
    Earlier,
    Rename(String),
    Add(String),
    /// A chip's tap: a topic taken off, or a suggestion kept, which makes it the owner's.
    Chip(TopicWrite),
    Typed(String),
    /// A topic the sheet offers, or the one typed.
    Write(String),
    Submit,
    Close,
    Searched(String),
}

#[derive(Debug)]
pub enum Secretary {}

/// Each topic once, sorted, so one filter has one print.
fn filter(topics: Vec<String>) -> Vec<String> {
    let mut t = topics;
    t.sort();
    t.dedup();
    t
}

fn watch(m: &Model) -> Effect {
    Effect::Watch {
        ask: Hold::Stream,
        local: Local::Stream {
            chosen: m.chosen.clone(),
            pages: m.pages,
        },
    }
}

fn day(m: &Model) -> Effect {
    let next = zone::next_day_ms(&m.context.offsets, m.now);
    Effect::Timer {
        ask: Ask::Day,
        after: u32::try_from((next - m.now).clamp(0, i64::from(u32::MAX))).unwrap_or(u32::MAX),
    }
}

fn today(m: &Model) -> NaiveDate {
    zone::day_ms(&m.context.offsets, m.now)
}

/// The names in words: "a", "a and b", "a, b and c".
fn and(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn toggled(chosen: &[String], topic: &str) -> Vec<String> {
    if chosen.iter().any(|t| t == topic) {
        chosen.iter().filter(|t| *t != topic).cloned().collect()
    } else {
        filter(chosen.iter().cloned().chain([topic.to_owned()]).collect())
    }
}

fn wrote(write: TopicWrite, follows: Wrote) -> Vec<Effect> {
    vec![Effect::Act {
        ask: Ask::Topic(follows),
        act: Act::Topic { write },
        secret: Secret::None,
        told: runtime::Told::Held,
    }]
}

impl Program for Secretary {
    type Model = Model;
    type Msg = Msg;
    type Effect = Effect;
    type Flags = Flags;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let m = Model {
            context: flags.context,
            chosen: filter(flags.topics),
            pages: 1,
            stream: Reading::Loading,
            drawn: None,
            shown: Slices::default(),
            placing: None,
            at_end: true,
            sheet: Sheet::Closed,
            search: Searching::default(),
            now: flags.now,
        };
        let effects = vec![
            watch(&m),
            Effect::Watch {
                ask: Hold::Scroll,
                local: Local::Scroll,
            },
            day(&m),
        ];
        (m, effects)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Effect> {
        match msg {
            Msg::Host(input) => host(m, *input),
            Msg::Copy(text) => vec![Effect::Copy { text }],
            Msg::Earlier => {
                m.pages += 1;
                vec![watch(m)]
            }
            Msg::Rename(topic) => {
                m.sheet = Sheet::Renaming {
                    typed: topic.clone(),
                    topic,
                };
                vec![Effect::Focus {
                    field: "topic".to_owned(),
                }]
            }
            Msg::Add(target) => {
                m.sheet = Sheet::Adding {
                    target,
                    typed: String::new(),
                };
                vec![Effect::Focus {
                    field: "topic".to_owned(),
                }]
            }
            Msg::Chip(write) => wrote(
                write,
                Wrote {
                    renamed: None,
                    closes: Closes::Nothing,
                },
            ),
            Msg::Typed(words) => {
                match &mut m.sheet {
                    Sheet::Renaming { typed, .. } | Sheet::Adding { typed, .. } => *typed = words,
                    Sheet::Closed => {}
                }
                Vec::new()
            }
            Msg::Submit => match ready(m) {
                Some(topic) => update_write(m, topic),
                None => Vec::new(),
            },
            Msg::Write(topic) => update_write(m, topic),
            Msg::Close => {
                m.sheet = Sheet::Closed;
                Vec::new()
            }
            Msg::Searched(words) => m.search.typed(words, Ask::Search),
        }
    }

    fn view(m: &Model) -> Node<Msg> {
        let head = chats::head(&m.search, &m.context.reach, Msg::Searched);
        let chrome = Chrome {
            class: "chats secretarystream",
            title: "Secretary".to_owned(),
            masthead: Masthead::Destination,
            width: crate::chrome::Width::Column,
            back: None,
            legend: None,
            now: vec![
                view! { P "legend chatsmemory" [link("", Place::Memory, "What the secretary remembers")] },
                head,
            ],
            shape: Shape::Turns,
        };
        // A search waits at least its rest, which the column says as the page's first reading.
        let waiting = match &m.search.search {
            Search::Off => matches!(m.stream, Reading::Loading),
            Search::On { .. } => m.search.waits(),
        };
        let status = if waiting {
            Status::Waiting
        } else {
            Status::Quiet
        };
        let body = match &m.search.search {
            Search::On { q, found, .. } => {
                vec![view! { Div "chatbody" [chats::hits(q, found, &m.context.offsets, today(m))] }]
            }
            Search::Off => stream(m),
        };
        chrome::page(chrome, status, body)
    }
}

fn update_write(m: &mut Model, topic: String) -> Vec<Effect> {
    match m.sheet.clone() {
        Sheet::Closed => Vec::new(),
        Sheet::Renaming { topic: from, .. } => {
            let renamed = m
                .chosen
                .contains(&from)
                .then(|| (from.clone(), topic.clone()));
            wrote(
                TopicWrite::Rename {
                    from: from.clone(),
                    to: topic,
                },
                Wrote {
                    renamed,
                    closes: Closes::Renaming { topic: from },
                },
            )
        }
        Sheet::Adding { target, .. } => wrote(
            TopicWrite::Add {
                target: target.clone(),
                topic,
            },
            Wrote {
                renamed: None,
                closes: Closes::Adding { target },
            },
        ),
    }
}

fn host(m: &mut Model, input: Input) -> Vec<Effect> {
    match input {
        Input::Context(c) => {
            m.context = c;
            Vec::new()
        }
        Input::Held {
            ask: Hold::Stream,
            reading,
            ..
        } => {
            let before = rounds(m);
            m.stream = read(reading);
            let Reading::Val { .. } = m.stream else {
                return Vec::new();
            };
            let now = (m.chosen.clone(), m.pages);
            let to = match &m.drawn {
                None => Some(Scroll::End),
                Some((chosen, _)) if *chosen != m.chosen => Some(Scroll::End),
                Some(_) if m.at_end => Some(Scroll::End),
                Some((_, pages)) if *pages < m.pages => Some(Scroll::Keep),
                Some(_) => None,
            };
            let first = m
                .drawn
                .as_ref()
                .is_none_or(|(chosen, _)| *chosen != m.chosen);
            m.shown.read(first, before, rounds(m));
            m.drawn = Some(now);
            m.placing = to;
            drawn(m)
        }
        Input::Held {
            ask: Hold::Found,
            reading,
            ..
        } => {
            m.search.found(reading);
            Vec::new()
        }
        Input::Held {
            ask: Hold::Scroll,
            reading,
            ..
        } => {
            if let Reading::Val { v } = read::<runtime::Scrolled>(reading) {
                m.at_end = v.at == runtime::Place::Bottom;
            }
            Vec::new()
        }
        Input::Answered {
            ask: Ask::Search(chats::Asking::Rested(n)),
            ..
        } => m.search.rested(n, Hold::Found),
        Input::Answered {
            ask: Ask::Search(chats::Asking::Found),
            ..
        } => Vec::new(),
        Input::Answered {
            ask: Ask::Slice, ..
        } => {
            m.shown.more(rounds(m));
            drawn(m)
        }
        Input::Answered {
            ask: Ask::Day,
            answer,
        } => {
            if let Ok(now) = answered::<i64>(answer) {
                m.now = now;
            }
            vec![day(m)]
        }
        Input::Answered {
            ask: Ask::Topic(w),
            answer,
        } => topic_answered(m, &w, answer),
        Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
    }
}

fn rounds(m: &Model) -> usize {
    match &m.stream {
        Reading::Val { v } => v.stream.rounds.len(),
        Reading::Loading | Reading::Error { .. } => 0,
    }
}

/// Asks the next slice while rounds are left; after the last, moves the box.
fn drawn(m: &mut Model) -> Vec<Effect> {
    if !m.shown.whole(rounds(m)) {
        return if m.shown.ask(rounds(m)) {
            vec![Effect::Frame { ask: Ask::Slice }]
        } else {
            Vec::new()
        };
    }
    m.placing
        .take()
        .map(|to| Effect::Scroll { to })
        .into_iter()
        .collect()
}

fn topic_answered(m: &mut Model, w: &Wrote, a: Answer) -> Vec<Effect> {
    match answered::<Outcome>(a) {
        Ok(Outcome::Answered {
            answer: Said::Taken,
            ..
        }) => {
            if w.closes.stands(&m.sheet) {
                m.sheet = Sheet::Closed;
            }
            match &w.renamed {
                Some((from, to)) => {
                    let chosen = filter(
                        m.chosen
                            .iter()
                            .map(|t| if t == from { to.clone() } else { t.clone() })
                            .collect(),
                    );
                    vec![Effect::Replace {
                        href: Place::Secretary(&chosen).href(),
                    }]
                }
                None => Vec::new(),
            }
        }
        Ok(Outcome::Answered {
            answer: Said::Refused { e } | Said::Lost { e },
            ..
        }) => vec![Effect::Toast { words: e }],
        Ok(Outcome::Already) => Vec::new(),
        Err(words) => vec![Effect::Toast { words }],
    }
}

fn ready(m: &Model) -> Option<String> {
    let (typed, renaming) = match &m.sheet {
        Sheet::Closed => return None,
        Sheet::Renaming { topic, typed } => (typed, Some(topic)),
        Sheet::Adding { typed, .. } => (typed, None),
    };
    let Check::Fits(t) = Check::of(typed) else {
        return None;
    };
    match renaming {
        Some(from) => (t != *from).then_some(t),
        None => (!on(m).contains(&t)).then_some(t),
    }
}

fn on(m: &Model) -> Vec<String> {
    match (&m.sheet, &m.stream) {
        (Sheet::Adding { target, .. }, Reading::Val { v }) => v
            .stream
            .rounds
            .iter()
            .find(|r| r.target == *target)
            .map(|r| r.topics.iter().map(|t| t.topic.clone()).collect())
            .unwrap_or_default(),
        (Sheet::Adding { .. } | Sheet::Renaming { .. } | Sheet::Closed, _) => Vec::new(),
    }
}

fn stream(m: &Model) -> Vec<Node<Msg>> {
    let (s, bar) = match &m.stream {
        Reading::Loading => return vec![topic_bar(&[], &m.chosen, writes(m))],
        Reading::Error { e } => {
            return vec![
                topic_bar(&[], &m.chosen, writes(m)),
                para("problem", format!("The rounds did not read: {e}")),
            ];
        }
        Reading::Val { v } => (&v.stream, &v.bar.topics),
    };
    let today = today(m);
    let nothing = if m.chosen.is_empty() {
        "Nothing asked yet.".to_owned()
    } else {
        format!("No round carries {}.", and(&m.chosen))
    };
    vec![
        Some(topic_bar(bar, &m.chosen, writes(m))),
        said(s, &m.chosen),
        (s.total == 0).then(|| chrome::empty(&nothing)),
        Some(view! { Div "streamrounds" => s.rounds[s.rounds.len().saturating_sub(m.shown.shown())..].iter().map(|r| (r.target.clone(), round(m, r, today))) }),
        sheet(m, bar),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn writes(m: &Model) -> bool {
    matches!(m.context.reach, Reach::Writes)
}

/// A chosen topic no round carries stays in the bar, so it can be taken out.
fn topic_bar(topics: &[TopicInUse], chosen: &[String], writes: bool) -> Node<Msg> {
    let extra = chosen
        .iter()
        .filter(|c| !topics.iter().any(|t| t.topic == **c))
        .map(|c| TopicInUse {
            topic: c.clone(),
            rounds: 0,
            disputed: Vec::new(),
        });
    let all: Vec<TopicInUse> = topics.iter().cloned().chain(extra).collect();
    if all.is_empty() {
        return viewtree::node::text("");
    }
    view! { Nav "topicbar" {AriaLabel => "Topics"} [all.iter().map(|t| {
        let on = chosen.contains(&t.topic);
        let disputed = !t.disputed.is_empty();
        view! { Span {Class => if on { "topicchip on" } else { "topicchip" }, Data("topic") => &t.topic} [
            view! { A "topiclink" {Href => Place::Secretary(&toggled(chosen, &t.topic)).href(), AriaPressed => if on { "true" } else { "false" }, Role => "button"} [
                t.topic.as_str(),
                view! { Span "topiccount" {AriaLabel => chrome::counted(t.rounds, "round", "rounds")} [t.rounds.to_string()] },
            ] },
            disputed.then(|| view! { Span "topicdisputed" {Title => format!("Renamed apart, also called {}: rename it to settle", and(&t.disputed))} ["≠"] }),
            (writes && (on || disputed)).then(|| view! { Button "topicrename" {Type => "button", AriaLabel => format!("Rename {}", t.topic)} @click(Msg::Rename(t.topic.clone())) ["Rename"] }),
        ] }
    }).collect::<Vec<_>>()] }
}

/// Earlier is shown while some rounds are not drawn.
fn said(s: &StreamPayload, chosen: &[String]) -> Option<Node<Msg>> {
    let shown = s.rounds.len();
    let more = shown < s.total;
    if chosen.is_empty() && !more {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let share = if s.total == 0 {
        1.0
    } else {
        shown as f64 / s.total as f64
    };
    let words = if chosen.is_empty() {
        format!("{shown} of {} rounds", s.total)
    } else {
        format!(
            "{}{} round{} on {}",
            if more {
                format!("{shown} of ")
            } else {
                String::new()
            },
            s.total,
            if s.total == 1 { "" } else { "s" },
            and(chosen)
        )
    };
    Some(view! { P "streamsaid legend" [
        more.then(|| vec![view! { Span {Title => format!("{shown} of {} rounds shown", s.total)} [pie(share)] }, viewtree::node::text(" ")]),
        words,
        more.then(|| vec![viewtree::node::text(" "), view! { Button "act act-earlier" {Type => "button"} @click(Msg::Earlier) ["Earlier"] }]),
        (!chosen.is_empty()).then(|| vec![viewtree::node::text(" "), link("streamall", Place::Secretary(&[]), "All rounds")]),
    ] })
}

fn round(m: &Model, r: &StreamRound, today: NaiveDate) -> Node<Msg> {
    let when = recent(&m.context.offsets, &r.opening.at, today);
    let meta = |class: &'static str, name: &str| {
        view! { P {Class => class} [
            view! { A {Href => Place::Conversation { id: &r.conversation, reference: "", focus: false, at: u32::try_from(r.turn).ok() }.href()} [name] },
            view! { Span "streamwhen" [when.clone()] },
        ] }
    };
    let asked = |class: &'static str| {
        prose(
            crate::part::Tag::Div,
            class,
            &r.opening.text,
            Some(crate::part::Who::of(&r.opening.who)),
            Chips::None,
        )
    };
    let (head, prompt) = match &r.asked {
        RoundAsked::Person {} => (
            vec![
                meta("streammeta", chats::named(&r.title)),
                asked(if Who::of(&r.opening.who) == Who::Person {
                    "turntext streamasked streamasked-person"
                } else {
                    "turntext streamasked streamasked-other"
                }),
            ],
            None,
        ),
        RoundAsked::Box { title } => (
            vec![meta("streammeta streamtitle", title)],
            Some(
                view! { Details "streamprompt" [view! { Summary ["What the box asked"] }, prose(crate::part::Tag::Div, "turntext", &r.opening.text, Some(crate::part::Who::of(&r.opening.who)), Chips::None)] },
            ),
        ),
    };
    let answer = r.answer.as_ref().map(|a| match &a.who {
        suzatary_core::wire::topics::Speaker::Model { blocks } if !blocks.is_empty() => {
            answer::blocks(&a.text, blocks, Msg::Copy)
        }
        suzatary_core::wire::topics::Speaker::Model { .. }
        | suzatary_core::wire::topics::Speaker::Person { .. }
        | suzatary_core::wire::topics::Speaker::Verb { .. }
        | suzatary_core::wire::topics::Speaker::Act { .. }
        | suzatary_core::wire::topics::Speaker::Box { .. } => {
            view! { Div "turntext" [a.text.as_str()] }
        }
    });
    let writes = writes(m);
    let topics = (!r.topics.is_empty() || writes).then(|| {
        view! { Div "roundtopics" [
            r.topics.iter().map(|t| topic_chip(&r.target, t, writes)).collect::<Vec<_>>(),
            (writes && r.room > 0).then(|| view! { Button "roundtopicadd" {Type => "button", AriaLabel => "Add a topic to this round"} @click(Msg::Add(r.target.clone())) [mark(crate::part::Mark::Plus), r.topics.is_empty().then_some("Topic")] }),
        ] }
    });
    view! { Article "streamround" {Data("target") => &r.target} [head, answer, prompt, topics] }
}

/// The owner's topic is settled; the secretary's reads as a suggestion, kept with one tap.
fn topic_chip(target: &str, t: &RoundTopic, writes: bool) -> Node<Msg> {
    let name = &t.topic;
    let off = || view! { Button "roundtopicoff" {Type => "button", AriaLabel => format!("Take {name} off this round")} @click(Msg::Chip(TopicWrite::Remove { target: target.to_owned(), topic: name.clone() })) [mark(crate::part::Mark::Cross)] };
    match t.by {
        Provenance::Owner => view! { Span "roundtopic" [name.as_str(), writes.then(off)] },
        Provenance::Inferred => {
            view! { Span "roundtopic suggested" {Title => "Suggested by the secretary"} [
                name.as_str(),
                writes.then(|| view! { Button "roundtopickeep" {Type => "button", AriaLabel => format!("Keep {name} on this round")} @click(Msg::Chip(TopicWrite::Add { target: target.to_owned(), topic: name.clone() })) [mark(crate::part::Mark::Check)] }),
                writes.then(off),
            ] }
        }
    }
}

fn sheet(m: &Model, bar: &[TopicInUse]) -> Option<Node<Msg>> {
    let (renaming, typed) = match &m.sheet {
        Sheet::Closed => return None,
        Sheet::Renaming { topic, typed } => (Some(topic), typed),
        Sheet::Adding { typed, .. } => (None, typed),
    };
    let on = on(m);
    let check = Check::of(typed);
    let in_use = |t: &str| bar.iter().any(|b| b.topic == t);
    let words = match (&check, renaming) {
        (Check::Blank, None) => "Pick one, or write a new one.".to_owned(),
        (Check::Blank, Some(_)) => String::new(),
        (Check::Breaks(why), _) => why.clone(),
        (Check::Fits(t), None) if on.contains(t) => "This round has it.".to_owned(),
        (Check::Fits(t), None) if in_use(t) => format!("Adds “{t}”."),
        (Check::Fits(t), None) => format!("“{t}” is a new topic."),
        (Check::Fits(t), Some(from)) if t == from => "That is its name now.".to_owned(),
        (Check::Fits(t), Some(from)) if in_use(t) => {
            format!("“{from}” and “{t}” become one topic, “{t}”, on every round of either.")
        }
        (Check::Fits(t), Some(from)) => format!("Every round on “{from}” keeps it, renamed “{t}”."),
    };
    let offered: Vec<&TopicInUse> = match renaming {
        Some(_) => Vec::new(),
        None => bar.iter().filter(|t| !on.contains(&t.topic)).collect(),
    };
    let breaks = matches!(check, Check::Breaks(_));
    Some(
        view! { Div "topicsheet" {Role => "dialog", AriaLabel => renaming.map_or_else(|| "Add a topic".to_owned(), |t| format!("Rename {t}"))} [
            view! { H2 "topicsheethead" [renaming.map_or_else(|| "Add a topic".to_owned(), |t| format!("Rename “{t}”"))] },
            (!offered.is_empty()).then(|| view! { Div "chiprow topicoffers" [offered.iter().map(|t| view! { Button "chipbtn" {Type => "button"} @click(Msg::Write(t.topic.clone())) [t.topic.as_str()] }).collect::<Vec<_>>()] }),
            view! { Form "topicform" @submit(Msg::Submit) [
                view! { Input "topicfield" {Type => "text", AriaLabel => if renaming.is_some() { "New name" } else { "Topic" }, Value => typed, Autocapitalize => "none", Autocomplete => "off", Spellcheck => "false", Data("field") => "topic"} @input(Msg::Typed) @keydown(escape) },
                view! { Button "act act-topicwrite" {Type => "submit", Disabled => ready(m).is_none()} [if renaming.is_some() { "Rename" } else { "Add" }] },
                view! { Button "act" {Type => "button"} @click(Msg::Close) ["Cancel"] },
            ] },
            view! { P {Class => if breaks { "topicsaid problem" } else { "topicsaid legend" }, Role => "status"} [words] },
        ] },
    )
}

fn escape(Key { name, .. }: Key) -> Option<Msg> {
    (name == "Escape").then_some(Msg::Close)
}
