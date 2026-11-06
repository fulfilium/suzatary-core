//! One todo as a page: the pushed screen on a phone, and the pane beside the list on a wide one.
//! What it says (its head, its detail, why its number), what can be done about it (its acts, its
//! edit, a deferral, its checklist), what else it says (who wrote it, concurrent writes, what the
//! box refused of this tab, what is proposed), its thirty days, and its history. Every write is an
//! event the box prints or the tab's core spells, sealed onto the chain's replica.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use suzatary_core::escape::{Escaped, unescape_markup};
use suzatary_core::reference::TodoRef;
use suzatary_core::todo::{Cycle, Patch, TodoPatch};
use suzatary_core::wire::{
    Authored, AuthoredSource, Authoring, ConversationPayload, Entry, FieldChange, Holds,
    InboxState, Link, Neighbour, Reverses, SubTodo, TodoPayload, UpdateTodo,
};
use ts_rs::TS;
use viewtree::Attr::{
    AriaLabel, Checked, Class, Data, Disabled, For, Href, Id, Open, Role, Title, Type,
};
use viewtree::kit::{Reading, para};
use viewtree::node::{foreign, text};
use viewtree::{Node, Program, view};

use crate::authoring::author;
use crate::chrome::{self, Chrome, Masthead, Shape, Status, Width, chip, link, mark};
use crate::form::{self, Kind, Markup, PriceMsg, Pricing};
use crate::held::{Folded, Unsents};
use crate::places::{Place, Question};
use crate::runtime::{
    Answer, Body, Context, Core, Effect, Gate, Held, Input, Local, answered, read,
};
use crate::todoparts::{
    Acting, Came, Phase, Verb, applicable, begin, came, chips, conflicts, is_open,
    mark as value_mark, markup, said, worded,
};
use crate::waiting::InboxRead;
use crate::zone::{Offset, day_ms, minute, next_day_ms};

/// Which todo, on which chain, and whether the page was pushed (a Back) or stands in a pane.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "TodoFlags")]
pub struct Flags {
    pub context: Context,
    pub chain: String,
    pub id: String,
    /// Whether the page was pushed over the list, so it has somewhere to go back to.
    pub pushed: bool,
    /// When it was opened, in epoch milliseconds.
    #[ts(type = "number")]
    pub now: i64,
}

/// The page's held questions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "TodoHold")]
pub enum Hold {
    Page,
    /// The tab's fold, the row the page falls back on.
    Fold,
    /// What this tab wrote that the box has not taken.
    Unsent,
    /// The Inbox's proposals, for those that serve this todo.
    Proposals,
}

/// A write the page asks for, one at a time of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "TodoWrite")]
pub enum Write {
    Act { acting: Acting },
    Save,
    Defer,
    Tick { line: usize },
}

impl Ask {
    /// The ask a write's answer comes back as, at its phase.
    fn of(phase: Phase, write: Write) -> Ask {
        match phase {
            Phase::Printed => Ask::Printed { write },
            Phase::Sealed => Ask::Sealed { write },
        }
    }
}

/// What the page asks once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "TodoAsk")]
pub enum Ask {
    Printed { write: Write },
    Sealed { write: Write },
    Undo,
    Built,
    Thirty,
}

/// Whether a part of the page that opens is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fold {
    /// Shut.
    Shut,
    /// Open.
    Open,
}

impl Fold {
    fn of(open: bool) -> Fold {
        if open { Fold::Open } else { Fold::Shut }
    }
}

/// Whether the priority is part of the edit: a price needs a rationale to move, so its controls
/// stay closed until asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reprice {
    Kept,
    Moved,
}

/// The edit as typed, over the record it was opened on.
#[derive(Debug, Clone)]
struct Edit {
    of: Authored,
    offered: Vec<String>,
    opening: (String, String),
    was_prose: bool,
    body: Markup,
    detail: Markup,
    category: String,
    waiting: String,
    reprice: Reprice,
    price: Pricing,
    rationale: String,
    note: String,
    said: Option<String>,
}

/// The edit's panel.
#[derive(Debug, Clone)]
enum Editing {
    Shut,
    Open(Box<Edit>),
}

/// The deferral's panel: shut, or the day typed and why it was refused.
#[derive(Debug, Clone)]
enum Deferring {
    Shut,
    Open { day: String, said: Option<String> },
}

/// The thirty days as the core sampled them.
#[derive(Debug, Clone, PartialEq)]
enum Thirty {
    Sampling,
    Unheld,
    Unpriced,
    Drawn(serde_json::Value),
    Failed(String),
}

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    chain: String,
    id: String,
    pushed: bool,
    today: NaiveDate,
    page: Held<TodoPayload>,
    fold: Reading<Option<Folded>>,
    unsent: Reading<Unsents>,
    proposed: usize,
    thirty: Thirty,
    /// The instant the thirty days were last asked at, so a fold that moves asks again.
    sampled: Option<i64>,
    running: BTreeSet<Write>,
    note: String,
    edit: Editing,
    defer: Deferring,
    provenance: Fold,
    history: Fold,
}

/// A move of the edit's form.
#[derive(Debug, Clone)]
pub enum EditMsg {
    Toggle(bool),
    Body(String),
    Detail(String),
    Category(String),
    Waiting(String),
    Reprice,
    Price(PriceMsg),
    Rationale(String),
    Note(String),
    Cancel,
    Save,
}

/// A move of the deferral.
#[derive(Debug, Clone)]
pub enum DeferMsg {
    Toggle(bool),
    Day(String),
    Send,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Ask>>),
    Back,
    Retry,
    Run(Verb),
    Note(String),
    Build,
    Edit(EditMsg),
    Defer(DeferMsg),
    Tick(usize),
    Provenance(bool),
    History(bool),
    Reset,
}

/// The page as a program.
#[derive(Debug)]
pub enum Todo {}

type Fx = Effect<Hold, Ask>;

impl Model {
    /// The page for `flags`, and what it asks first.
    #[must_use]
    pub(crate) fn open(f: Flags) -> (Model, Vec<Fx>) {
        let m = Model {
            today: day_ms(&f.context.offsets, f.now),
            context: f.context,
            chain: f.chain,
            id: f.id,
            pushed: f.pushed,
            page: Held::loading(),
            fold: Reading::Loading,
            unsent: Reading::Loading,
            proposed: 0,
            thirty: Thirty::Sampling,
            sampled: None,
            running: BTreeSet::new(),
            note: String::new(),
            edit: Editing::Shut,
            defer: Deferring::Shut,
            provenance: Fold::Shut,
            history: Fold::Shut,
        };
        let effects = vec![
            Effect::Hold {
                ask: Hold::Page,
                path: m.path(),
            },
            Effect::Watch {
                ask: Hold::Fold,
                local: Local::Fold,
            },
            Effect::Watch {
                ask: Hold::Unsent,
                local: Local::Unsent,
            },
            Effect::Watch {
                ask: Hold::Proposals,
                local: Local::Proposals,
            },
        ];
        (m, effects)
    }

    /// The todo it shows.
    #[must_use]
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    fn path(&self) -> String {
        Question::Todo {
            chain: &self.chain,
            id: &self.id,
        }
        .path()
    }

    fn writes(&self) -> bool {
        self.context.gate == Gate::Writes
    }

    /// The todo's row: the box's page of it, else the fold's.
    /// Its row as the tab's fold reads it, which moves with every write the replica takes; the
    /// box's page's row before the fold holds one.
    fn entry(&self) -> Option<&Entry> {
        let folded = match &self.fold {
            Reading::Val { v: Some(f) } => f.entry(&self.id),
            Reading::Val { v: None } | Reading::Loading | Reading::Error { .. } => None,
        };
        folded.or(self.payload().map(|p| &p.entry))
    }

    fn payload(&self) -> Option<&TodoPayload> {
        match &self.page.reading {
            Reading::Val { v } => Some(v),
            Reading::Loading | Reading::Error { .. } => None,
        }
    }

    /// The instant the thirty days are sampled at: the fold's, else when the page opened.
    fn sample_at(&self) -> Option<i64> {
        match &self.fold {
            Reading::Val { v: Some(f) } => crate::zone::ms(&f.at),
            Reading::Val { v: None } | Reading::Error { .. } | Reading::Loading => None,
        }
    }

    fn reference(&self) -> String {
        suzatary_core::chains::ChainName::new(self.chain.clone())
            .ok()
            .zip(prodrome_core::event::TodoId::new(self.id.clone()).ok())
            .map_or_else(
                || format!("todo:{}/{}", self.chain, self.id),
                |(chain, id)| TodoRef { chain, id }.to_string(),
            )
    }

    /// The thirty days asked again when the instant they are read at moved.
    fn resample(&mut self) -> Vec<Fx> {
        let at = self.sample_at();
        if at.is_none() || at == self.sampled {
            return Vec::new();
        }
        self.sampled = at;
        vec![Effect::Core {
            ask: Ask::Thirty,
            question: Core::Thirty {
                chain: self.chain.clone(),
                id: self.id.clone(),
                ms: at.unwrap_or_default(),
            },
        }]
    }

    /// One step: what the host or the person did.
    pub(crate) fn update(&mut self, msg: Msg) -> Vec<Fx> {
        match msg {
            Msg::Host(input) => self.heard(*input),
            Msg::Back => vec![Effect::Back {
                fallback: Place::Todos.href(),
            }],
            Msg::Retry => vec![Effect::Again { path: self.path() }],
            Msg::Note(words) => {
                self.note = words;
                Vec::new()
            }
            Msg::Run(verb) => self.run(Acting::Verb(verb)),
            Msg::Build => {
                if !self.writes() {
                    return Vec::new();
                }
                vec![Effect::Post {
                    ask: Ask::Built,
                    path: Question::Build {
                        chain: &self.chain,
                        id: &self.id,
                    }
                    .path(),
                    body: Body::Build {},
                }]
            }
            Msg::Edit(m) => self.edited(m),
            Msg::Defer(m) => self.deferred(m),
            Msg::Tick(line) => self.tick(line),
            Msg::Provenance(open) => {
                self.provenance = Fold::of(open);
                Vec::new()
            }
            Msg::History(open) => {
                self.history = Fold::of(open);
                Vec::new()
            }
            Msg::Reset => match &self.unsent {
                Reading::Val { v } => vec![Effect::Reset {
                    losses: v.losses.clone(),
                }],
                Reading::Loading | Reading::Error { .. } => Vec::new(),
            },
        }
    }

    fn begin(&mut self, w: Write) -> bool {
        self.writes() && self.running.insert(w)
    }

    fn print(&mut self, w: Write, body: Authoring) -> Vec<Fx> {
        if !self.begin(w) {
            return Vec::new();
        }
        vec![author(Ask::Printed { write: w }, &self.chain, body)]
    }

    fn run(&mut self, acting: Acting) -> Vec<Fx> {
        let write = Write::Act { acting };
        if !self.begin(write) {
            return Vec::new();
        }
        let note = self.note.trim().to_owned();
        vec![begin(&self.chain, &self.id, acting, note, |phase| {
            Ask::of(phase, write)
        })]
    }

    fn update_todo(&mut self, w: Write, patch: TodoPatch, note: String) -> Vec<Fx> {
        self.print(
            w,
            Authoring::Update(Box::new(UpdateTodo {
                todo: self.id.clone(),
                patch,
                note,
            })),
        )
    }

    fn tick(&mut self, line: usize) -> Vec<Fx> {
        let Some(subs) = self
            .entry()
            .and_then(|e| e.authored.as_ref())
            .map(|a| a.subtodos.clone())
        else {
            return Vec::new();
        };
        let subtodos: Vec<suzatary_core::record::SubTodo> = subs
            .into_iter()
            .enumerate()
            .map(|(i, s)| suzatary_core::record::SubTodo {
                done: if i == line { !s.done } else { s.done },
                body: s.body,
            })
            .collect();
        self.update_todo(
            Write::Tick { line },
            TodoPatch {
                subtodos: Patch::Set(subtodos),
                ..TodoPatch::default()
            },
            String::new(),
        )
    }

    fn edited(&mut self, m: EditMsg) -> Vec<Fx> {
        let categories = self.categories();
        match (m, &mut self.edit) {
            (EditMsg::Toggle(true), Editing::Shut) => {
                if let Some(a) = self.entry().and_then(|e| e.authored.clone()) {
                    self.edit = Editing::Open(Box::new(Edit::of(a, &categories)));
                }
                Vec::new()
            }
            (EditMsg::Toggle(false) | EditMsg::Cancel, Editing::Open(_)) => {
                self.edit = Editing::Shut;
                Vec::new()
            }
            (EditMsg::Save, Editing::Open(e)) => match e.read() {
                Err(why) => {
                    e.said = Some(why.clone());
                    vec![Effect::Toast { words: why }]
                }
                Ok(update) => {
                    e.said = None;
                    let note = e.note.trim().to_owned();
                    self.update_todo(Write::Save, update, note)
                }
            },
            (m, Editing::Open(e)) => {
                e.typed(m);
                Vec::new()
            }
            (_, Editing::Shut) => Vec::new(),
        }
    }

    /// The first day a deferral may name, on the owner's clock.
    fn tomorrow(&self) -> String {
        day_ms(
            &self.context.offsets,
            next_day_ms(&self.context.offsets, self.page_now()),
        )
        .to_string()
    }

    fn deferred(&mut self, m: DeferMsg) -> Vec<Fx> {
        // Every message but a close opens it first: the field is drawn in the shut box too, so it
        // can be typed in before the box's opening is heard.
        if let (DeferMsg::Toggle(true) | DeferMsg::Day(_) | DeferMsg::Send, Deferring::Shut) =
            (&m, &self.defer)
        {
            self.defer = Deferring::Open {
                day: self.tomorrow(),
                said: None,
            };
        }
        let Deferring::Open { day, said } = &mut self.defer else {
            return Vec::new();
        };
        match m {
            DeferMsg::Toggle(true) => Vec::new(),
            DeferMsg::Toggle(false) => {
                self.defer = Deferring::Shut;
                Vec::new()
            }
            DeferMsg::Day(d) => {
                *day = d;
                Vec::new()
            }
            DeferMsg::Send => match NaiveDate::parse_from_str(day, "%Y-%m-%d") {
                Err(_) => {
                    *said = Some("Pick a date to defer to.".to_owned());
                    Vec::new()
                }
                Ok(to) => {
                    *said = None;
                    self.update_todo(
                        Write::Defer,
                        TodoPatch {
                            defer_to: Patch::Set(to),
                            ..TodoPatch::default()
                        },
                        String::new(),
                    )
                }
            },
        }
    }

    /// The instant on the owner's clock the page reads as now: the fold's.
    fn page_now(&self) -> i64 {
        self.sample_at().unwrap_or_else(|| {
            self.today
                .and_hms_opt(12, 0, 0)
                .map_or(0, |t| t.and_utc().timestamp_millis())
        })
    }

    fn categories(&self) -> Vec<String> {
        self.context.categories.clone()
    }

    fn heard(&mut self, input: Input<Hold, Ask>) -> Vec<Fx> {
        match input {
            Input::Context(c) => {
                self.context = c;
                Vec::new()
            }
            Input::Held {
                ask: Hold::Page,
                reading,
                behind,
            } => {
                self.page = Held::of(reading, behind);
                // An edit opened over content another writer has since changed starts afresh.
                let categories = self.categories();
                let now = self.payload().and_then(|p| p.entry.authored.clone());
                if let (Editing::Open(e), Some(now)) = (&mut self.edit, now)
                    && now != e.of
                {
                    **e = Edit::of(now, &categories);
                }
                self.rendered()
            }
            Input::Held {
                ask: Hold::Fold,
                reading,
                ..
            } => {
                self.fold = read(reading);
                self.resample()
            }
            Input::Held {
                ask: Hold::Unsent,
                reading,
                ..
            } => {
                let was = self.unsent_here();
                self.unsent = read(reading);
                // A commit to the replica moves the thirty days.
                self.sampled = None;
                let mut out = self.resample();
                // What this tab wrote of it reached the box: the box's page now says it.
                if was && !self.unsent_here() {
                    out.push(Effect::Again { path: self.path() });
                }
                out
            }
            Input::Held {
                ask: Hold::Proposals,
                reading,
                ..
            } => {
                let serves = self.reference();
                self.proposed = match read::<InboxRead>(reading) {
                    Reading::Val { v } => v
                        .payload
                        .proposals
                        .iter()
                        .filter(|p| matches!(p.state, InboxState::Open {}))
                        .filter(|p| p.serves.as_ref().is_some_and(|s| s.to_string() == serves))
                        .count(),
                    Reading::Loading | Reading::Error { .. } => 0,
                };
                Vec::new()
            }
            Input::Answered { ask, answer } => self.answered(ask, answer),
            Input::Heard { ask, .. } | Input::Opened { ask } => match ask {},
        }
    }

    fn answered(&mut self, ask: Ask, answer: Answer) -> Vec<Fx> {
        match ask {
            Ask::Thirty => {
                self.thirty = match answered::<serde_json::Value>(answer) {
                    Err(e) => Thirty::Failed(e),
                    Ok(v) => match v.get("type").and_then(serde_json::Value::as_str) {
                        Some("Unheld") => Thirty::Unheld,
                        Some("Unpriced") => Thirty::Unpriced,
                        Some("Drawn") => Thirty::Drawn(v),
                        Some(_) | None => Thirty::Sampling,
                    },
                };
                Vec::new()
            }
            Ask::Built => match answered::<ConversationPayload>(answer) {
                Ok(c) => vec![Effect::Go {
                    href: Place::conversation(&c.id).href(),
                }],
                Err(words) => vec![Effect::Toast { words }],
            },
            Ask::Undo => self.run(Acting::Undo),
            Ask::Printed { write } => self.came(Phase::Printed, write, answer),
            Ask::Sealed { write } => self.came(Phase::Sealed, write, answer),
        }
    }

    fn came(&mut self, phase: Phase, write: Write, answer: Answer) -> Vec<Fx> {
        match came(&self.chain, phase, answer, |phase| Ask::of(phase, write)) {
            Came::Seals(seals) => seals,
            Came::Already => {
                self.running.remove(&write);
                Vec::new()
            }
            Came::Taken => self.done(write),
            Came::Refused(words) => self.refused(write, words),
        }
    }

    fn refused(&mut self, write: Write, words: String) -> Vec<Fx> {
        self.running.remove(&write);
        if let (Write::Save, Editing::Open(e)) = (write, &mut self.edit) {
            e.said = Some(words.clone());
        }
        vec![Effect::Toast { words }]
    }

    /// The HTML the box's page carries for a confirmed record, kept on this device, so its blocks
    /// show it wherever they are placed by key; a todo's page is the box's answer about an edit
    /// this tab made, which no fold of the box has carried yet.
    /// Whether this tab holds a write of it the box has not taken.
    fn unsent_here(&self) -> bool {
        match &self.unsent {
            Reading::Val { v } => v.todos.contains_key(&self.id),
            Reading::Loading | Reading::Error { .. } => false,
        }
    }

    fn rendered(&self) -> Vec<Fx> {
        let Some(e) = self.payload().map(|p| &p.entry).filter(|e| !e.unconfirmed) else {
            return Vec::new();
        };
        let Some(r) = e.authored.as_ref().map(|a| &a.rich) else {
            return Vec::new();
        };
        let pairs: Vec<(String, String)> = [
            (&r.keys.body, &r.body),
            (&r.keys.detail, &r.detail),
            (&r.keys.rationale, &r.rationale),
            (&r.keys.waiting_on, &r.waiting_on),
        ]
        .into_iter()
        .filter_map(|(k, html)| Some((k.clone()?, html.clone()?)))
        .collect();
        if pairs.is_empty() {
            Vec::new()
        } else {
            vec![Effect::Rendered { pairs }]
        }
    }

    fn done(&mut self, write: Write) -> Vec<Fx> {
        self.running.remove(&write);
        match write {
            Write::Act { acting } => {
                self.note.clear();
                said(acting, Ask::Undo)
            }
            Write::Save => {
                self.edit = Editing::Shut;
                vec![Effect::Toast {
                    words: format!("updated {}", self.id),
                }]
            }
            Write::Defer => {
                let day = match &self.defer {
                    Deferring::Open { day, .. } => day.clone(),
                    Deferring::Shut => String::new(),
                };
                self.defer = Deferring::Shut;
                vec![Effect::Toast {
                    words: format!("deferred {} to {day}", self.id),
                }]
            }
            Write::Tick { .. } => Vec::new(),
        }
    }

    /// The page drawn.
    #[must_use]
    pub(crate) fn view(&self) -> Node<Msg> {
        let offsets = &self.context.offsets;
        let entry = self.entry();
        let status = match (&self.page.reading, entry) {
            // A page never fetched is drawn from the fold, and says so.
            (Reading::Error { .. }, Some(_)) => Status::Quiet,
            (r, _) => chrome::status(r, None, offsets, Msg::Retry),
        };
        let c = Chrome {
            class: "todoview",
            width: Width::Column,
            title: String::new(),
            masthead: Masthead::Page,
            back: self.pushed.then_some(("Todos", Msg::Back)),
            legend: None,
            now: Vec::new(),
            shape: Shape::Lines,
        };
        let (head, body) = match entry {
            Some(e) => (self.head(e), self.body(e)),
            None => (view! { Div "todohead" }, Vec::new()),
        };
        let offline = self.offline().map(|said| {
            view! { P "legend offlinenote" {Role => "status"} [said] }
        });
        chrome::page_headed(c, view! { Div "todomast" [offline, head] }, status, body)
    }

    fn offline(&self) -> Option<String> {
        match (&self.page.reading, &self.page.behind) {
            (Reading::Val { .. }, Some(crate::runtime::Behind { why: Some(why), .. })) => {
                Some(format!(
                    "The box did not answer ({why}); this is the page as this tab last fetched it."
                ))
            }
            (Reading::Error { e }, _) if self.entry().is_some() => Some(format!(
                "The box did not answer ({e}); this tab never fetched this todo's page."
            )),
            (Reading::Val { .. } | Reading::Loading | Reading::Error { .. }, _) => None,
        }
    }

    /// Whether the body is drawn from the box's page, rendered, or from the fold as its source.
    fn fetched(&self) -> bool {
        self.payload().is_some()
    }

    /// Whether its markup is drawn as Typst: a confirmed record, on a page the box answered (a
    /// page never fetched is drawn as plain text, and says so).
    fn trusted(&self, e: &Entry) -> bool {
        self.fetched() && !e.unconfirmed
    }

    fn head(&self, e: &Entry) -> Node<Msg> {
        let a = e.authored.as_ref();
        let body = a.map(|a| {
            let key = a.rich.keys.body.as_deref().filter(|_| self.trusted(e));
            view! { Div "todolead" [markup("todobody", key, &a.body)] }
        });
        let rendered = a.is_some_and(|a| self.fetched() && !e.unconfirmed && a.rich.body.is_some());
        let note = match (a, e.unconfirmed, rendered, self.fetched()) {
            (None, _, _, _) | (Some(_), false, true, _) => None,
            (Some(_), true, _, _) => Some("Shown the way it was written until you confirm it."),
            (Some(_), false, false, true) => {
                Some("Shown the way it was written: Typst could not draw it.")
            }
            (Some(_), false, false, false) => {
                Some("Plain text: this tab has no rendered page for this todo yet.")
            }
        };
        view! { Div "todohead" [
            view! { Div "todofulfil" [view! { Span "todopct" [worded(e.value)] }] },
            body,
            view! { Code "todoid" [e.todo.as_str()] },
            chips(e),
            note.map(|n| para("legend", n)),
        ] }
    }

    fn body(&self, e: &Entry) -> Vec<Node<Msg>> {
        let payload = self.payload();
        vec![
            self.acts(e),
            self.edit_panel(e),
            Some(self.reading(e)),
            Some(self.content(e)),
            Some(chrome::ask(&self.reference())),
            self.provenance(e),
            conflicts_block(e),
            self.news(),
            claim(e),
            self.proposed(),
            Some(self.curve()),
            Some(payload.map_or_else(
                || {
                    para(
                        "legend",
                        "Its links and history come with its page from the box.",
                    )
                },
                |p| self.history(p),
            )),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn acts(&self, e: &Entry) -> Option<Node<Msg>> {
        if !self.writes() {
            return None;
        }
        let button = |verb: Verb| {
            let busy = self.running.contains(&Write::Act {
                acting: Acting::Verb(verb),
            });
            view! { Button {Type => "button", Class => verb.class(), Disabled => busy} @click(Msg::Run(verb)) [verb.label()] }
        };
        let build = self
            .payload()
            .filter(|p| p.builder.is_some() && is_open(e))
            .map(|_| view! { Button "act act-build" {Type => "button"} @click(Msg::Build) ["Build it"] });
        Some(view! { Div "actrow" [
            applicable(e).into_iter().map(button).collect::<Vec<_>>(),
            build,
            form::line("noteline", "Why, or what next", "Why, or what next", &self.note, Msg::Note),
        ] })
    }

    fn edit_panel(&self, e: &Entry) -> Option<Node<Msg>> {
        e.authored.as_ref().filter(|_| self.writes())?;
        let edit = match &self.edit {
            Editing::Shut => view! { Div "pickerhead draftform" },
            Editing::Open(f) => f.view(
                self.today,
                e.unconfirmed,
                self.running.contains(&Write::Save),
            ),
        };
        let open = matches!(self.edit, Editing::Open(_));
        let editbox = view! { Details "why editbox" {Open => open} @toggle(|o| Msg::Edit(EditMsg::Toggle(o))) [
            view! { Summary ["Edit", mark("chevron")] },
            edit,
        ] };
        let defer = is_open(e).then(|| {
            let tomorrow = self.tomorrow();
            let (open, day, said) = match &self.defer {
                Deferring::Shut => (false, &tomorrow, None),
                Deferring::Open { day, said } => (true, day, said.as_deref()),
            };
            let inner = vec![
                view! { Div "draftrow" [
                    form::day("defer to this date", Some(self.today), day, |d| Msg::Defer(DeferMsg::Day(d))),
                    view! { Button "addbtn" {Type => "button", Disabled => self.running.contains(&Write::Defer)} @click(Msg::Defer(DeferMsg::Send)) ["Defer"] },
                ] },
                form::refusal(said),
            ];
            view! { Details "why editbox deferbox" {Open => open} @toggle(|o| Msg::Defer(DeferMsg::Toggle(o))) [
                view! { Summary ["Defer to…", mark("chevron")] },
                view! { Div "pickerhead draftform" [inner] },
            ] }
        });
        Some(view! { Div "editpanels" [editbox, defer] })
    }

    fn reading(&self, e: &Entry) -> Node<Msg> {
        let detail = e
            .authored
            .as_ref()
            .filter(|a| !a.detail.is_empty())
            .map(|a| {
                let key = a.rich.keys.detail.as_deref().filter(|_| self.trusted(e));
                view! { Details "why" {Open => true} [
                    view! { Summary ["Detail", mark("chevron")] },
                    markup("todobody", key, &a.detail),
                ] }
            });
        let why = self.payload().map(|p| why(&p.why));
        view! { Div "reading" [detail, why] }
    }

    fn content(&self, e: &Entry) -> Node<Msg> {
        let Some(a) = &e.authored else {
            return view! { Section [para("legend", "The chain holds no Authored record for this todo — only lifecycle events.")] };
        };
        let rich = |key: Option<&str>, source: &str, class: &str| {
            markup(class, key.filter(|_| self.trusted(e)), source)
        };
        let waiting = (!a.waiting_on.is_empty()).then(|| {
            vec![
                view! { H2 ["Waiting on"] },
                rich(
                    a.rich.keys.waiting_on.as_deref(),
                    &a.waiting_on,
                    "todobody waitingbody",
                ),
            ]
        });
        let checklist =
            (!a.subtodos.is_empty()).then(|| vec![view! { H2 ["Checklist"] }, self.checklist(a)]);
        let rationale = (!a.rationale.is_empty()).then(|| {
            let joined = a.rationale.join(" ");
            vec![
                view! { H2 ["Rationale"] },
                match a.rich.rationale.as_deref() {
                    None => view! { Ul "rationale" [a.rationale.iter().map(|l| view! { Li [l.as_str()] }).collect::<Vec<_>>()] },
                    Some(_) => rich(a.rich.keys.rationale.as_deref(), &joined, "todobody rationalebody"),
                },
            ]
        });
        let source = a
            .source
            .as_ref()
            .map(|s| vec![view! { H2 ["Source message"] }, source_block(s)]);
        view! { Section "todocontent" [waiting, checklist, rationale, source] }
    }

    fn checklist(&self, a: &Authored) -> Node<Msg> {
        let line = |(i, s): (usize, &SubTodo)| {
            let ticking = self.running.contains(&Write::Tick { line: i });
            let done = if ticking { !s.done } else { s.done };
            let id = format!("subtodo-{}-{i}", self.id);
            let tick = self.writes().then(|| {
                view! { Input {Type => "checkbox", Id => id.clone(), Checked => done, Disabled => ticking} @click(Msg::Tick(i)) }
            });
            let mark =
                (!self.writes()).then(|| view! { Span "box" [if s.done { "✓" } else { "" }] });
            view! { Li {Class => if done { "done" } else { "" }} [
                tick,
                mark,
                view! { Label {For => id} [s.body.as_str()] },
            ] }
        };
        view! { Ul "checklist" [a.subtodos.iter().enumerate().map(line).collect::<Vec<_>>()] }
    }

    fn provenance(&self, e: &Entry) -> Option<Node<Msg>> {
        let a = e.authored.as_ref()?;
        let who = if a.actor.is_empty() {
            "someone unnamed"
        } else {
            &a.actor
        };
        let on = crate::zone::day(&self.context.offsets, &a.at)
            .map(|d| format!(" on {}", d.format("%b %-d")))
            .unwrap_or_default();
        let class = if e.unconfirmed {
            "provline unconfirmed"
        } else {
            "provline"
        };
        Some(
            view! { Details {Class => class, Open => self.provenance == Fold::Open} @toggle(Msg::Provenance) [
                view! { Summary [format!("Written by {who}{on}"), mark("chevron")] },
                view! { P [if e.unconfirmed {
                    "Not confirmed yet, so the number above does not use it. Confirm it to make it count."
                } else {
                    "Confirmed, so the number above uses it."
                }] },
            ] },
        )
    }

    /// What the box refused of this tab's objects, and what waits for this device's trust.
    fn news(&self) -> Option<Node<Msg>> {
        let Reading::Val { v: u } = &self.unsent else {
            return None;
        };
        let reasons = u.refused.len();
        let refusal = (reasons > 0 || u.stranded > 0).then(|| {
            let others = (u.blocked as usize).saturating_sub(reasons);
            let refused = (reasons > 0).then(|| {
                let said = if others > 0 {
                    format!(
                        "The box refused {reasons} of this browser's objects, and {others} written since {} on {} — none of them can be sent.",
                        if others == 1 { "rests" } else { "rest" },
                        if reasons == 1 { "it" } else { "them" }
                    )
                } else {
                    format!(
                        "The box refused {reasons} of this browser's {} — {} cannot be sent.",
                        if reasons == 1 { "object" } else { "objects" },
                        if reasons == 1 { "it" } else { "they" }
                    )
                };
                vec![
                    view! { P "sincewhat" [said] },
                    view! { Ul "sincelist" [u.refused.iter().map(|r| view! { Li [r.as_str()] }).collect::<Vec<_>>()] },
                ]
            });
            let away = (u.stranded > 0).then(|| {
                view! { P "sincewhat" [format!("{} of this browser's unsent {} written in a store the box no longer serves.", u.stranded, if u.stranded == 1 { "acts was" } else { "acts were" })] }
            });
            view! { Div "sincebox refusedbox" {Role => "alert"} [
                refused,
                away,
                view! { P "sincewhat losses" [format!("A reset drops every change this browser holds that has not reached the box: {} in all.", u.losses.len())] },
                view! { Button "sincedismiss" {Type => "button"} @click(Msg::Reset) ["Reset this replica from the box"] },
            ] }
        });
        let waiting = (u.waiting > 0).then(|| {
            view! { Div "sincebox waitingbox" {Role => "status"} [view! { P "sincewhat" [format!(
                "{} of this browser's {} {} for this device to be trusted; nothing is lost, and {} once it is.",
                u.waiting,
                if u.waiting == 1 { "act" } else { "acts" },
                if u.waiting == 1 { "waits" } else { "wait" },
                if u.waiting == 1 { "it goes" } else { "they go" }
            )] }] }
        });
        (refusal.is_some() || waiting.is_some()).then(|| view! { Div "news" [refusal, waiting] })
    }

    fn proposed(&self) -> Option<Node<Msg>> {
        let n = self.proposed;
        (n > 0).then(|| {
            view! { P "proposednote" {Role => "status"} [
                format!("{n} proposed change{} to this todo {} waiting in the ", if n == 1 { "" } else { "s" }, if n == 1 { "is" } else { "are" }),
                link("", Place::Inbox(""), "Inbox"),
                ". Nothing here reflects it until you tap it.",
            ] }
        })
    }

    fn curve(&self) -> Node<Msg> {
        let say = |s: &str| para("legend", s);
        let inside = match &self.thirty {
            Thirty::Sampling => {
                view! { Div "placeholder ph-lines" {Role => "status", AriaLabel => "Loading"} [mark("loadmark")] }
            }
            Thirty::Unheld => say(
                "No curve yet: this browser does not hold this todo's events. It draws once they sync.",
            ),
            Thirty::Unpriced => say(
                "No curve: nothing recorded by now gives this todo a spec or a checklist, so it has no function to draw.",
            ),
            Thirty::Failed(e) => para("problem", format!("No curve: {e}")),
            Thirty::Drawn(v) => foreign("curve", &self.id, v.to_string()),
        };
        view! { Section "pricebox" [
            view! { H2 "priceheading" ["Fulfillment, 30 days"] },
            view! { Div "curvebox" [inside] },
        ] }
    }

    fn history(&self, p: &TodoPayload) -> Node<Msg> {
        let n = p.stream.len();
        view! { Div "historyparts" [
            links(p),
            view! { Details "history" {Open => self.history == Fold::Open} @toggle(Msg::History) [
                view! { Summary [view! { H2 [format!("History ({n} {})", if n == 1 { "event" } else { "events" })] }, mark("chevron")] },
                para("legend", "Oldest first."),
                view! { Ol "timeline" [p.stream.iter().map(|l| event(&self.context.offsets, l, p.diffs.get(&l.hash), &p.stream)).collect::<Vec<_>>()] },
            ] },
        ] }
    }
}

trait Kinded {
    fn kind(&self) -> &str;
}

impl Kinded for Link {
    fn kind(&self) -> &str {
        match &self.holds {
            Holds::Envelope { kind } | Holds::Event { kind, .. } => kind,
        }
    }
}

fn why(w: &suzatary_core::reading::Why) -> Node<Msg> {
    use suzatary_core::reading::{Part, Why};
    fn item(p: &Part) -> Node<Msg> {
        view! { Li [
            view! { Div "node" [p.says.as_str()] },
            (!p.parts.is_empty()).then(|| view! { Ul "tree" [p.parts.iter().map(item).collect::<Vec<_>>()] }),
        ] }
    }
    match w {
        Why::Unpriced { says } => {
            view! { Section "whytree" [view! { H2 ["Why this number"] }, view! { P [says.as_str()] }] }
        }
        Why::Priced {
            heading,
            says,
            passes,
            parts,
        } => view! { Section "whytree" [
            view! { H2 [heading.as_str()] },
            para("says", says.as_str()),
            passes.iter().map(|l| para("pass", l.as_str())).collect::<Vec<_>>(),
            (!parts.is_empty()).then(|| view! { Ul "tree" [parts.iter().map(item).collect::<Vec<_>>()] }),
            (!parts.is_empty()).then(|| para("legend", "Each part is priced at the moment its parent read it. The box computed every number.")),
        ] },
    }
}

fn source_block(s: &AuthoredSource) -> Node<Msg> {
    let facts = view! { Dl "facts" [
        view! { Dt ["sender"] }, view! { Dd [s.sender.as_str()] },
        view! { Dt ["subject"] }, view! { Dd [s.subject.as_str()] },
        view! { Dt ["date"] }, view! { Dd [s.date.as_str()] },
    ] };
    if s.hash.is_empty() {
        return view! { Div [facts, para("legend", "No message hash was recorded with this todo, so there is nothing to open.")] };
    }
    view! { Div [
        view! { A "sourcelink" {Href => Place::Message(&s.hash).href()} [
            view! { Span "sourcewho" [s.sender.as_str()] },
            view! { Span "sourcesubject" [if s.subject.is_empty() { "(no subject)" } else { s.subject.as_str() }] },
            view! { Span "meta" [s.date.as_str()] },
            mark("chevron"),
        ] },
        facts,
    ] }
}

fn claim(e: &Entry) -> Option<Node<Msg>> {
    use suzatary_core::wire::ClaimedState;
    let claimed = match e.claimed {
        ClaimedState::Agrees => return None,
        ClaimedState::Completed => "completed",
        ClaimedState::Cancelled => "cancelled",
    };
    Some(view! { P "claim" [
        chip(&format!("claimed {claimed}"), "prov"),
        " An untrusted actor recorded this outcome. The pricing fold does not apply it until a trusted actor re-appends it, so the number above still treats this todo as open.",
    ] })
}

fn short(hash: &str) -> Node<Msg> {
    view! { Code "hash" {Title => hash.to_owned()} [hash.get(..12).unwrap_or(hash)] }
}

/// Concurrent writes to one register, until a later write descends from all of them: every
/// candidate is drawn, and no version is the page's pick.
fn conflicts_block(e: &Entry) -> Option<Node<Msg>> {
    let cs = conflicts(e);
    if cs.is_empty() {
        return None;
    }
    let states = (e.states.len() > 1).then(|| {
        let said: Vec<String> = e
            .states
            .iter()
            .map(|s| {
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            })
            .collect();
        para(
            "conflictstate",
            format!(
                "Its state is {}; it is filed open while any version is.",
                said.join(" or ")
            ),
        )
    });
    let candidates = (e.candidates.len() > 1).then(|| {
        view! { Ol "candidates" [e.candidates.iter().map(|v| view! { Li [
            para("legend", format!("{}, {}", v.actor, v.at.get(..10).unwrap_or_default())),
            markup("todobody", v.rich.keys.body.as_deref().filter(|_| v.rich.body.is_some()), &v.body),
        ] }).collect::<Vec<_>>()] }
    });
    let list = view! { Ul "conflictlist" [cs.iter().map(|(kind, hashes)| {
        let mut parts: Vec<Node<Msg>> = vec![view! { Strong [*kind] }, text(": ")];
        for (i, h) in hashes.iter().enumerate() {
            if i > 0 {
                parts.push(text(", "));
            }
            parts.push(short(h));
        }
        view! { Li [parts] }
    }).collect::<Vec<_>>()] };
    Some(view! { Section "conflictbox" {Role => "status"} [
        view! { H2 ["Concurrent writes"] },
        states,
        candidates,
        list,
        para("conflictwhat", "Two writers changed this at once. Every version stands until one is chosen, and the number is the most urgent version's. Any change you make now settles it."),
    ] })
}

fn neighbour(n: &Neighbour) -> Node<Msg> {
    view! { A "linkrow" {Data("todo") => n.todo.clone(), Href => Place::Todo(&n.todo).href()} [
        value_mark(n.value),
        view! { Code "linkid" [n.todo.as_str()] },
        chip(&serde_json::to_value(n.state).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(), if n.state == suzatary_core::wire::EntryState::Open { "" } else { "done" }),
        n.unconfirmed.then(|| chip("unconfirmed", "prov")),
        mark("chevron"),
    ] }
}

/// Links are derived from content, an explicit link or an id in prose; the chain stores none.
fn links(p: &TodoPayload) -> Option<Node<Msg>> {
    let half = |ns: &[Neighbour], head: &str| {
        (!ns.is_empty()).then(|| {
            vec![
                view! { H3 "linkhead" [format!("{head} ({})", ns.len())] },
                view! { Div "linklist" [ns.iter().map(neighbour).collect::<Vec<_>>()] },
            ]
        })
    };
    let (out, back) = (
        half(&p.links.out, "Links to"),
        half(&p.links.inward, "Linked from"),
    );
    (out.is_some() || back.is_some()).then(|| {
        view! { Section "linksection" [
            view! { H2 ["Links"] },
            out,
            back,
            para("legend", "Derived from content — an explicit #link(\"todo:…\"), or an id spelled in prose. The chain stores no link."),
        ] }
    })
}

/// A quoted string is prose and wraps; any other literal is structure, shown whole in its own
/// scroller.
fn printed(v: &str) -> Node<Msg> {
    if v.is_empty() {
        view! { Span "meta" ["—"] }
    } else if v.starts_with(['\'', '"']) {
        view! { Span "prose" [v] }
    } else {
        view! { Code "literal" [v] }
    }
}

fn diff(changes: Option<&Vec<FieldChange>>) -> Option<Node<Msg>> {
    let changes = changes?;
    if changes.is_empty() {
        return Some(para(
            "meta",
            "no content change against the previous Authored record",
        ));
    }
    Some(
        view! { Dl "diff" [changes.iter().flat_map(|c| if c.before.is_empty() {
            vec![view! { Dt [c.field.as_str()] }, view! { Dd "now" [printed(&c.after)] }]
        } else {
            vec![
                view! { Dt [c.field.as_str()] },
                view! { Dd "was" [printed(&c.before)] },
                view! { Dd "now" [view! { Span "arrow" ["→"] }, printed(&c.after)] },
            ]
        }).collect::<Vec<_>>()] },
    )
}

/// What an undo or a redo says of the event it reverses, by that event's kind in `stream`.
fn reversal(r: &Reverses, stream: &[Link]) -> Option<&'static str> {
    match r {
        Reverses::Nothing => None,
        Reverses::Redo { .. } => Some("redid"),
        Reverses::Undo { of } => Some(
            match stream.iter().find(|l| l.hash == *of).map(Kinded::kind) {
                Some("Completed") => "undid completion",
                Some("Cancelled") => "undid cancellation",
                Some("Reopened") => "undid reopening",
                Some("SpecRevised") => "undid a price change",
                Some("Authored") => "undid a content edit",
                Some(_) | None => "undid the previous act",
            },
        ),
    }
}

fn event(
    offsets: &[Offset],
    l: &Link,
    changes: Option<&Vec<FieldChange>>,
    stream: &[Link],
) -> Node<Msg> {
    let (act, meta) = match &l.holds {
        Holds::Envelope { .. } => (None, None),
        Holds::Event {
            actor,
            at,
            reverses,
            ..
        } => (
            reversal(reverses, stream).map(|a| chip(a, "")),
            Some(view! { Span "meta" [format!("{} · {actor}", minute(offsets, at))] }),
        ),
    };
    view! { Li "tlitem" [
        view! { Div "tlhead" [view! { Strong [l.kind()] }, act, meta] },
        diff(changes),
        view! { Div "hashes" ["hash ", short(&l.hash), l.parents.is_empty().then_some(" · genesis")] },
    ] }
}

impl Edit {
    /// The edit opened on a record: a title stored as escaped prose opens in that prose, so
    /// editing it never shows the escape's backslashes; markup a writer wrote opens unchanged.
    fn of(a: Authored, categories: &[String]) -> Edit {
        let opened = |stored: &str| match unescape_markup(&Escaped::from_chain(stored.to_owned())) {
            Ok(p) if p != stored => (p, true),
            Ok(_) | Err(_) => (stored.to_owned(), false),
        };
        let (body, was_body) = opened(&a.body);
        let (detail, was_detail) = opened(&a.detail);
        let price = Pricing::of(a.spec.as_ref().and_then(cycle_of));
        // The deployment's categories, with the todo's own kept where the vocabulary lost it.
        let mut offered = categories.to_vec();
        if !a.category.is_empty() && !offered.contains(&a.category) {
            offered.push(a.category.clone());
        }
        Edit {
            offered,
            opening: (body.clone(), detail.clone()),
            was_prose: was_body || was_detail,
            body: Markup::of(body),
            detail: Markup::of(detail),
            category: a.category.clone(),
            waiting: a.waiting_on.clone(),
            reprice: Reprice::Kept,
            price,
            rationale: String::new(),
            note: String::new(),
            said: None,
            of: a,
        }
    }

    fn typed(&mut self, m: EditMsg) {
        match m {
            EditMsg::Body(t) => self.body.text = t,
            EditMsg::Detail(t) => self.detail.text = t,
            EditMsg::Category(c) => self.category = c,
            EditMsg::Waiting(w) => self.waiting = w,
            EditMsg::Reprice => {
                self.reprice = match self.reprice {
                    Reprice::Kept => Reprice::Moved,
                    Reprice::Moved => Reprice::Kept,
                };
            }
            EditMsg::Price(p) => self.price.update(p),
            EditMsg::Rationale(r) => self.rationale = r,
            EditMsg::Note(n) => self.note = n,
            EditMsg::Toggle(_) | EditMsg::Cancel | EditMsg::Save => {}
        }
    }

    /// What moved, as the box's patch: a field left as it opened is kept, so a page that sent
    /// everything would not overwrite what moved on the chain meanwhile.
    fn read(&self) -> Result<TodoPatch, String> {
        let a = &self.of;
        let (body, detail, waiting) = (
            self.body.text.trim(),
            self.detail.text.trim(),
            self.waiting.trim(),
        );
        if body != self.opening.0 && body.is_empty() {
            return Err("The action line cannot be empty.".to_owned());
        }
        let set = |now: &str, was: &str| {
            if now == was.trim() {
                Patch::Keep
            } else {
                Patch::Set(now.to_owned())
            }
        };
        let mut patch = TodoPatch {
            body: set(body, &self.opening.0),
            detail: set(detail, &self.opening.1),
            category: if self.category == a.category {
                Patch::Keep
            } else {
                Patch::Set(self.category.clone())
            },
            waiting_on: if waiting == a.waiting_on || waiting.is_empty() {
                Patch::Keep
            } else {
                Patch::Set(waiting.to_owned())
            },
            clear_waiting_on: waiting.is_empty() && !a.waiting_on.is_empty(),
            ..TodoPatch::default()
        };
        if self.reprice == Reprice::Moved {
            let price = self.price.read()?;
            let rationale = self.rationale.trim();
            if rationale.is_empty() {
                return Err("Say why the priority is changing.".to_owned());
            }
            patch.price = Patch::Set(price);
            patch.rationale = Patch::Set(rationale.to_owned());
        }
        Ok(patch)
    }

    fn view(&self, today: NaiveDate, unconfirmed: bool, saving: bool) -> Node<Msg> {
        let e = |m: EditMsg| Msg::Edit(m);
        view! { Div "pickerhead draftform" [
            unconfirmed.then(|| para("meta editprov", "Written by triage and not yet confirmed: saving an edit makes it yours, like confirming it.")),
            self.was_prose.then(|| para("meta", "Saved in plain text before, so it opens the way you would type it; characters like $ # @ * now read in Typst, which the preview shows.")),
            self.body.view("body", "draftfield draftbody", "the todo's action line, in Typst markup", 2, |t| Msg::Edit(EditMsg::Body(t))),
            para("meta", "Typst markup: *bold*, _emphasis_, - lists, #link(\"https://…\")[text]. The preview is what the list shows."),
            self.detail.view("detail", "draftfield", "detail, in Typst markup — empty clears it", 3, |t| Msg::Edit(EditMsg::Detail(t))),
            form::chips("Pick a category", &self.offered, &self.category, |c| e(EditMsg::Category(c))),
            form::line("draftfield", "what it is blocked on — empty clears it", "", &self.waiting, |w| Msg::Edit(EditMsg::Waiting(w))),
            view! { Label "editrepricelabel" [
                view! { Input {Type => "checkbox", Checked => self.reprice == Reprice::Moved} @click(e(EditMsg::Reprice)) },
                " change the priority",
            ] },
            (self.reprice == Reprice::Moved).then(|| view! { Div "editprice" [
                self.price.view(today).map(|p| Msg::Edit(EditMsg::Price(p))),
                form::line("draftfield", "why the new priority", "why this number", &self.rationale, |r| Msg::Edit(EditMsg::Rationale(r))),
            ] }),
            view! { Div "editactions" [
                view! { Button "cancelbtn" {Type => "button"} @click(e(EditMsg::Cancel)) ["Cancel"] },
                view! { Button "addbtn" {Type => "button", Disabled => saving} @click(e(EditMsg::Save)) ["Save changes"] },
                form::line("noteline", "Why, or what next", "Why, or what next", &self.note, |n| Msg::Edit(EditMsg::Note(n))),
            ] },
            form::refusal(self.said.as_deref()),
        ] }
    }
}

/// The cycle a `Recur` or `Periodic` term carries, read back off the term the box authored: its
/// ramp's two points, their days apart and their values.
fn cycle_of(spec: &serde_json::Value) -> Option<(Kind, Cycle)> {
    let own = |t: &serde_json::Value| -> Option<(Kind, Cycle)> {
        let kind = match t.get("kind")?.as_str()? {
            "recur" => Kind::Recur,
            "periodic" => Kind::Periodic,
            _ => return None,
        };
        let points = t.get("term")?.get("points")?.as_array()?;
        let [a, b] = points.as_slice() else {
            return None;
        };
        let at = |p: &serde_json::Value| crate::zone::ms(p.get("at")?.as_str()?);
        let percent = |p: &serde_json::Value| -> Option<u32> {
            // A ratio in [0, 1], so its percent fits.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let p = (p.get("value")?.as_f64()? * 100.0).round() as u32;
            Some(p)
        };
        let days = u32::try_from((at(b)? - at(a)? + 43_200_000) / 86_400_000).ok()?;
        Some((
            kind,
            Cycle {
                days,
                from_percent: percent(a)?,
                to_percent: percent(b)?,
            },
        ))
    };
    own(spec).or_else(|| match spec {
        serde_json::Value::Object(o) => o
            .iter()
            .filter(|(k, _)| k.as_str() != "kind")
            .find_map(|(_, v)| cycle_of(v)),
        serde_json::Value::Array(xs) => xs.iter().find_map(cycle_of),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => None,
    })
}

impl Program for Todo {
    type Model = Model;
    type Msg = Msg;
    type Effect = Fx;
    type Flags = Flags;

    fn init(f: Flags) -> (Model, Vec<Fx>) {
        Model::open(f)
    }

    fn update(m: &mut Model, msg: Msg) -> Vec<Fx> {
        m.update(msg)
    }

    fn view(m: &Model) -> Node<Msg> {
        m.view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recurring_term_opens_with_its_cycle() {
        let spec = serde_json::json!({"kind": "recur", "todo": "x", "term": {"kind": "curve", "points": [
            {"at": "2026-09-01T00:00:00", "value": 0.7}, {"at": "2026-09-15T00:00:00", "value": 0.3}]}});
        assert_eq!(
            cycle_of(&spec),
            Some((
                Kind::Recur,
                Cycle {
                    days: 14,
                    from_percent: 70,
                    to_percent: 30
                }
            ))
        );
    }
}
