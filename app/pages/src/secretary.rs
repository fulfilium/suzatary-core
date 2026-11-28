//! The secretary: every conversation's rounds as one stream, filtered by the address's topics.

use crate::part::Mark;
use crate::part::Tag;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use suzatary_base::conversation_id::DEFAULT;
use suzatary_topics::secretary::topics::provenance::Provenance;
use suzatary_topics::secretary::topics::topic::Topic;
use suzatary_topics::wire::topics::Speaker;
use suzatary_topics::wire::topics::{
    ProposalRow, RoundAsked, RoundTopic, StreamPayload, StreamRound, TopicInUse, TopicsPayload,
    TurnRow,
};
use ts_rs::TS;
use viewtree::Attr::{
    AriaLabel, AriaPressed, Autocapitalize, Autocomplete, Class, Data, Disabled, Href, Role,
    Spellcheck, Title, Type, Value,
};
use viewtree::kit::{Reading, para};
use viewtree::node::lazy;
use viewtree::{Key, Node, Printed, Program, view};

use crate::answer::{self, Dressable, Dressed};
use crate::chats::{self, Search, Searching};
use crate::chrome::{self, Chips, Chrome, Masthead, Shape, Status, link, mark, pie, prose};
use crate::composer::{self, Composer};
use crate::conversation::Spoken;
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
    Draft,
    /// The newest page of rounds, whatever the filter: where a question sent under topics is found.
    Placed,
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
    Composer(composer::Asking),
    Said(Box<Spoken>),
}

/// A topic write's follow-up once the tab's replica holds it: a rename of a topic the filter
/// holds moves the address to the new name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub struct Wrote {
    pub renamed: Option<(String, String)>,
}

pub(crate) type Effect = runtime::Effect<Hold, Ask>;
pub(crate) type Input = runtime::Input<Hold, Ask>;

/// `Local::Stream`, read from its dressed text (`answer::Dressed`).
#[derive(Debug, Clone, Deserialize)]
#[serde(from = "Dressed<Streamed>")]
pub struct StreamRead {
    pub(crate) stream: Stream,
    pub(crate) bar: TopicsPayload,
}

/// The stream a filter shows and the topics in use, as the core's worker reads them.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct Streamed {
    stream: StreamPayload,
    bar: TopicsPayload,
}

impl Dressable for Streamed {
    fn turns(&mut self) -> impl Iterator<Item = &mut TurnRow> {
        self.stream
            .rounds
            .iter_mut()
            .filter_map(|r| r.answer.as_mut())
    }
    fn proposals(&mut self) -> impl Iterator<Item = &mut ProposalRow> {
        std::iter::empty()
    }
}

impl From<Dressed<Streamed>> for StreamRead {
    fn from(d: Dressed<Streamed>) -> Self {
        let Streamed { stream, bar } = d.worn();
        StreamRead {
            stream: stream.into(),
            bar,
        }
    }
}

/// The stream as the page holds it: each round printed once, as it is read, so a later step
/// draws a round it drew before by comparing one number (`viewtree::node::lazy`).
#[derive(Debug, Clone)]
pub(crate) struct Stream {
    rounds: Vec<Printed<StreamRound>>,
    total: usize,
}

impl From<StreamPayload> for Stream {
    fn from(p: StreamPayload) -> Self {
        Self {
            rounds: p.rounds.into_iter().map(Printed::new).collect(),
            total: p.total,
        }
    }
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
    composer: Composer,
    /// The questions sent under topics whose rounds the reading does not hold yet.
    tagging: Vec<Tagging>,
    now: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tagging {
    text: String,
    topics: Vec<String>,
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
    Composer(composer::Ev),
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
    type Slot = crate::runtime::Never;
    type Sub = crate::runtime::Never;

    fn init(flags: Flags) -> (Model, Vec<Effect>) {
        let (composer, draft) = Composer::of(DEFAULT.as_str(), &[]);
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
            composer,
            tagging: Vec::new(),
            now: flags.now,
        };
        let effects = vec![
            watch(&m),
            Effect::Watch {
                ask: Hold::Scroll,
                local: Local::Scroll,
            },
            Effect::Watch {
                ask: Hold::Draft,
                local: draft,
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
            Msg::Chip(write) => wrote(write, Wrote { renamed: None }),
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
            Msg::Composer(ev) => {
                let outs = m.composer.update(ev);
                composed(m, outs)
            }
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
                view! { P "legend chatsmemory" [
                    link("", Place::Conversations, "Conversations"),
                    " · ",
                    link("", Place::Memory, "What the secretary remembers"),
                ] },
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
            Search::Off => {
                let mut body = stream(m);
                body.push(
                    composer::view(&m.composer, &m.context.reach, m.context.keys)
                        .map(Msg::Composer),
                );
                body
            }
        };
        chrome::page(chrome, status, body)
    }
}

/// The sheet's write: the sheet closes at the tap, and the bar draws it from the tab's replica.
fn update_write(m: &mut Model, topic: String) -> Vec<Effect> {
    let sheet = std::mem::replace(&mut m.sheet, Sheet::Closed);
    match sheet {
        Sheet::Closed => Vec::new(),
        Sheet::Renaming { topic: from, .. } => {
            let renamed = m
                .chosen
                .contains(&from)
                .then(|| (from.clone(), topic.clone()));
            wrote(TopicWrite::Rename { from, to: topic }, Wrote { renamed })
        }
        Sheet::Adding { target, .. } => {
            wrote(TopicWrite::Add { target, topic }, Wrote { renamed: None })
        }
    }
}

/// The composer's outs: a question is sealed into the default conversation. Nothing runs here to
/// stop, and no question is chosen to edit.
fn composed(m: &mut Model, outs: Vec<composer::Out>) -> Vec<Effect> {
    let (mut effects, then) = composer::effects(outs, Ask::Composer);
    for t in then {
        match t {
            composer::Then::Send(sent) => effects.extend(say(m, &sent)),
            composer::Then::Stop | composer::Then::Up => {}
        }
    }
    effects
}

/// A question asked in the default conversation, under the topics chosen: the filter would hide
/// its round until it carries them, so the round is found in the newest page and tagged.
fn say(m: &mut Model, sent: &composer::Sent) -> Vec<Effect> {
    let mut effects = Vec::new();
    if !m.chosen.is_empty() {
        m.tagging.push(Tagging {
            text: sent.text.clone(),
            topics: m.chosen.clone(),
        });
        effects.push(Effect::Watch {
            ask: Hold::Placed,
            local: Local::Stream {
                chosen: Vec::new(),
                pages: 1,
            },
        });
    }
    effects.push(Effect::Core {
        ask: Ask::Said(Box::new(Spoken::from(sent))),
        question: runtime::Core::Line {
            line: runtime::Sealing::Say {
                conversation: DEFAULT.as_str().to_owned(),
                question: runtime::Asking::of(sent),
            },
        },
    });
    effects
}

/// A question kept in the tab empties the box; one refused comes back into it, with why.
fn kept(m: &mut Model, spoken: Spoken, answer: Answer) -> Vec<Effect> {
    let Err(e) = answered::<runtime::Sealed>(answer) else {
        let outs = m.composer.sealed();
        return composed(m, outs);
    };
    m.tagging.retain(|t| t.text != spoken.text);
    let outs = m.composer.failed(spoken.into());
    let mut effects = composed(m, outs);
    effects.push(Effect::Toast { words: e });
    effects
}

/// Each question waiting to be tagged whose round the newest page holds takes its topics.
fn tag(m: &mut Model, stream: &Stream) -> Vec<Effect> {
    let mut effects = Vec::new();
    for t in std::mem::take(&mut m.tagging) {
        let round = stream
            .rounds
            .iter()
            .rev()
            .find(|r| r.conversation == DEFAULT.as_str() && r.opening.text == t.text);
        match round {
            Some(r) => {
                for topic in &t.topics {
                    let write = TopicWrite::Add {
                        target: r.target.clone(),
                        topic: topic.clone(),
                    };
                    effects.extend(wrote(write, Wrote { renamed: None }));
                }
            }
            None => m.tagging.push(t),
        }
    }
    effects
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
        } => streamed(m, reading),
        Input::Held {
            ask: Hold::Found,
            reading,
            ..
        } => {
            m.search.found(reading);
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
            ask: Hold::Placed,
            reading,
            ..
        } => match read::<StreamRead>(reading) {
            Reading::Val { v } => tag(m, &v.stream),
            Reading::Loading | Reading::Error { .. } => Vec::new(),
        },
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
            m.shown.more(rounds(m), |i| round_words(&m.stream, i));
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
            ask: Ask::Composer(c),
            answer,
        } => {
            let outs = m.composer.answered(c, answer);
            composed(m, outs)
        }
        Input::Answered {
            ask: Ask::Said(spoken),
            answer,
        } => kept(m, *spoken, answer),
        Input::Answered {
            ask: Ask::Topic(w),
            answer,
        } => topic_answered(m, &w, answer),
        Input::Heard { ask, .. } | Input::Opened { ask } | Input::Lost { ask } => match ask {},
    }
}

/// The stream read anew: drawn, and the box placed where the reader is or the filter moved it.
fn streamed(m: &mut Model, reading: Reading<Answer>) -> Vec<Effect> {
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

fn rounds(m: &Model) -> usize {
    match &m.stream {
        Reading::Val { v } => v.stream.rounds.len(),
        Reading::Loading | Reading::Error { .. } => 0,
    }
}

/// The bytes of the words the `i`-th round held draws, its opening's and its answer's
/// (`slices::WORDS`).
fn round_words(stream: &Reading<StreamRead>, i: usize) -> usize {
    match stream {
        Reading::Val { v } => v.stream.rounds.get(i).map_or(0, |r| {
            answer::words(&r.opening) + r.answer.as_ref().map_or(0, answer::words)
        }),
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
        }) => match &w.renamed {
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
        },
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
        Some(view! { Div "streamrounds" => s.rounds[s.rounds.len().saturating_sub(m.shown.shown())..].iter().map(|r| {
            let when = recent(&m.context.offsets, &r.opening.at, today);
            (r.target.clone(), lazy(round, (r.clone(), when, writes(m))))
        }) }),
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
fn said(s: &Stream, chosen: &[String]) -> Option<Node<Msg>> {
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

fn round((r, when, writes): &(Printed<StreamRound>, String, bool)) -> Node<Msg> {
    let writes = *writes;
    let meta = |class: &'static str, name: &str| {
        view! { P {Class => class} [
            view! { A {Href => Place::Conversation { id: &r.conversation, reference: "", focus: false, at: u32::try_from(r.turn).ok() }.href()} [name] },
            view! { Span "streamwhen" [when.clone()] },
        ] }
    };
    let asked = |class: &'static str| {
        prose(
            Tag::Div,
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
                view! { Details "streamprompt" [view! { Summary ["What the box asked"] }, prose(Tag::Div, "turntext", &r.opening.text, Some(crate::part::Who::of(&r.opening.who)), Chips::None)] },
            ),
        ),
    };
    let answer = r.answer.as_ref().map(|a| match &a.who {
        Speaker::Model { blocks } if !blocks.is_empty() => {
            answer::blocks(&a.text, blocks, Msg::Copy)
        }
        Speaker::Model { .. }
        | Speaker::Person { .. }
        | Speaker::Verb { .. }
        | Speaker::Act { .. }
        | Speaker::Box { .. } => {
            view! { Div "turntext" [a.text.as_str()] }
        }
    });
    let topics = (!r.topics.is_empty() || writes).then(|| {
        view! { Div "roundtopics" [
            r.topics.iter().map(|t| topic_chip(&r.target, t, writes)).collect::<Vec<_>>(),
            (writes && r.room > 0).then(|| view! { Button "roundtopicadd" {Type => "button", AriaLabel => "Add a topic to this round"} @click(Msg::Add(r.target.clone())) [mark(Mark::Plus), r.topics.is_empty().then_some("Topic")] }),
        ] }
    });
    view! { Article "streamround" {Data("target") => &r.target} [head, answer, prompt, topics] }
}

/// The owner's topic is settled; the secretary's reads as a suggestion, kept with one tap.
fn topic_chip(target: &str, t: &RoundTopic, writes: bool) -> Node<Msg> {
    let name = &t.topic;
    let off = || view! { Button "roundtopicoff" {Type => "button", AriaLabel => format!("Take {name} off this round")} @click(Msg::Chip(TopicWrite::Remove { target: target.to_owned(), topic: name.clone() })) [mark(Mark::Cross)] };
    match t.by {
        Provenance::Owner => view! { Span "roundtopic" [name.as_str(), writes.then(off)] },
        Provenance::Inferred => {
            view! { Span "roundtopic suggested" {Title => "Suggested by the secretary"} [
                name.as_str(),
                writes.then(|| view! { Button "roundtopickeep" {Type => "button", AriaLabel => format!("Keep {name} on this round")} @click(Msg::Chip(TopicWrite::Add { target: target.to_owned(), topic: name.clone() })) [mark(Mark::Check)] }),
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
        view! { Div "topicsheet" {Role => "dialog", Data("layer") => "window", AriaLabel => renaming.map_or_else(|| "Add a topic".to_owned(), |t| format!("Rename {t}"))} [
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Gate, Keys, Opens, Text};
    use serde_json::json;
    use viewtree::Runtime;

    fn page(topics: &[&str]) -> Runtime<Secretary> {
        let context = Context {
            gate: Gate::Writes,
            reach: Reach::Writes,
            offsets: Vec::new(),
            keys: Keys::Touch,
            opens: Opens::Over,
            categories: Vec::new(),
            clock: None,
        };
        let topics = topics.iter().map(|t| (*t).to_owned()).collect();
        Runtime::<Secretary>::start(Flags {
            context,
            topics,
            now: 0,
        })
        .0
    }

    fn typed_and_sent(rt: &mut Runtime<Secretary>, words: &str) -> Vec<Effect> {
        rt.step(Msg::Composer(composer::Ev::Typed(words.to_owned())));
        rt.step(Msg::Composer(composer::Ev::Send)).1
    }

    fn sealed(rt: &mut Runtime<Secretary>, words: &str) -> Vec<Effect> {
        let spoken = Spoken {
            text: words.to_owned(),
            references: Vec::new(),
            files: Vec::new(),
            mode: composer::Mode::Asking,
        };
        let body = Text::printed(&json!({"conversation": "default"}));
        rt.step(Msg::Host(Box::new(Input::Answered {
            ask: Ask::Said(Box::new(spoken)),
            answer: Answer::Done { body },
        })))
        .1
    }

    fn placed(rt: &mut Runtime<Secretary>, words: &[&str]) -> Vec<Effect> {
        let rounds: Vec<_> = words
            .iter()
            .enumerate()
            .map(|(turn, text)| {
                json!({
                    "target": format!("round:default/{turn}"),
                    "conversation": "default",
                    "title": "Secretary",
                    "turn": turn,
                    "opening": {"index": turn, "text": text, "at": "2026-11-26T09:00:00Z",
                        "attempt": "current", "gone": null, "who": "person",
                        "references": [], "attachments": [], "queued": false},
                    "asked": {"by": "person"},
                    "answer": null,
                    "topics": [],
                    "room": 3,
                })
            })
            .collect();
        let read = json!({
            "read": {"stream": {"rounds": rounds, "total": rounds.len()}, "bar": {"topics": []}},
            "html": {},
            "drawn": [],
        });
        rt.step(Msg::Host(Box::new(Input::Held {
            ask: Hold::Placed,
            reading: Reading::Val {
                v: Answer::Done {
                    body: Text::printed(&read),
                },
            },
            behind: None,
        })))
        .1
    }

    fn lines(effects: &[Effect]) -> Vec<runtime::Sealing> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Core {
                    question: runtime::Core::Line { line },
                    ..
                } => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    fn tags(effects: &[Effect]) -> Vec<TopicWrite> {
        effects
            .iter()
            .filter_map(|e| match e {
                Effect::Act {
                    act: Act::Topic { write },
                    ..
                } => Some(write.clone()),
                _ => None,
            })
            .collect()
    }

    /// A question typed at the foot is sealed into the default conversation; with no topic
    /// chosen the stream already shows its round, so nothing is watched or tagged.
    #[test]
    fn a_question_is_asked_in_the_default_conversation() {
        let mut rt = page(&[]);
        let effects = typed_and_sent(&mut rt, "  water the ferns ");
        assert_eq!(
            lines(&effects),
            [runtime::Sealing::Say {
                conversation: "default".to_owned(),
                question: runtime::Asking {
                    text: "water the ferns".to_owned(),
                    references: Vec::new(),
                    files: Vec::new(),
                },
            }]
        );
        assert!(!effects.iter().any(|e| matches!(
            e,
            Effect::Watch {
                ask: Hold::Placed,
                ..
            }
        )));
        sealed(&mut rt, "water the ferns");
        assert!(rt.model().composer.empty(), "kept, the box empties");
        assert!(placed(&mut rt, &["water the ferns"]).is_empty());
    }

    /// Under topics the filter would hide the new round, so it takes them once the newest page
    /// holds it, and only then.
    #[test]
    fn a_question_sent_under_topics_takes_them_when_its_round_is_read() {
        let mut rt = page(&["garden", "trip"]);
        let effects = typed_and_sent(&mut rt, "water the ferns");
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Watch {
                ask: Hold::Placed,
                ..
            }
        )));
        sealed(&mut rt, "water the ferns");
        assert!(tags(&placed(&mut rt, &["an older question"])).is_empty());
        assert_eq!(
            tags(&placed(&mut rt, &["an older question", "water the ferns"])),
            ["garden", "trip"].map(|topic| TopicWrite::Add {
                target: "round:default/1".to_owned(),
                topic: topic.to_owned(),
            })
        );
        assert!(
            tags(&placed(&mut rt, &["an older question", "water the ferns"])).is_empty(),
            "tagged once"
        );
    }

    /// A question the tab refuses comes back into the box, and its round is never tagged.
    #[test]
    fn a_refused_question_returns_to_the_box_untagged() {
        let mut rt = page(&["garden"]);
        typed_and_sent(&mut rt, "water the ferns");
        let spoken = Spoken {
            text: "water the ferns".to_owned(),
            references: Vec::new(),
            files: Vec::new(),
            mode: composer::Mode::Asking,
        };
        let (_, effects) = rt.step(Msg::Host(Box::new(Input::Answered {
            ask: Ask::Said(Box::new(spoken)),
            answer: Answer::Refused {
                status: 400,
                words: "say something".to_owned(),
            },
        })));
        assert!(effects.iter().any(|e| matches!(e, Effect::Toast { .. })));
        assert!(!rt.model().composer.empty(), "its words are back");
        assert!(tags(&placed(&mut rt, &["water the ferns"])).is_empty());
    }
}
