//! Home: the open chain's todos as one tree of terms, grouped, the one place a todo is born, and
//! on a wide screen the todo it has selected in a pane beside it.
//!
//! The tree is the core's (`suzatary_core::tree`), grown and priced by the core in the tab's
//! worker: the rows the tab's fold holds are its members and their specs its ancestry, priced
//! under the box's rows at once and under the reading of the objects the tab holds once that
//! lands. The last tree drawn stays while the next is asked.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use suzatary_core::todo::{Price, TodoFields};
use suzatary_core::tree::Tree;
use suzatary_core::wire::{Authoring, BriefingsPayload, Entry, Fulfillment, NewTodo, Overview};
use ts_rs::TS;
use viewtree::Attr::{
    AriaControls, AriaCurrent, AriaExpanded, AriaLabel, AriaPressed, Class, Data, Disabled,
    EnterKeyHint, Hidden, Href, Id, Placeholder, Role, Title, Type, Value as Val,
};
use viewtree::kit::{Reading, para};
use viewtree::{Node, Program, view};

use crate::authoring::{author, sealed};
use crate::chrome::{self, Chrome, Masthead, Shape, Width, chip, empty, mark, percent, pie};
use crate::form::{self, Markup, PriceMsg, Pricing};
use crate::held::{Folded, Unsent, Unsents};
use crate::needsyou::Route;
use crate::places::{Place as To, Question};
use crate::runtime::{
    ActName, Answer, Context, Core, Effect, Gate, Input, KeyKind, Keyed, Local, Outcome, Placed,
    PlacedWorth, Register, Said, answered, read,
};
use crate::todo;
use crate::todoparts::{
    Acting, Came, Phase, Verb, applicable, begin, came, conflicts, finish, headline, is_open, kind,
    said,
};
use crate::zone::{day_ms, ms as ms_of};

/// The chain the project's own work is on: its list is titled Project.
const PROJECT: &str = "suzatary";

/// Which chain's list, and when it was opened.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "TodosFlags")]
pub struct Flags {
    pub context: Context,
    pub chain: String,
    /// When it was opened, in epoch milliseconds.
    #[ts(type = "number")]
    pub now: i64,
}

/// The page's held questions: the tab's readings, its registers, the box's answers, and its
/// pane's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "of")]
#[ts(rename = "TodosHold")]
pub enum Hold {
    Fold,
    Unsent,
    Waiting,
    Route,
    Kept(Kept),
    Briefings,
    /// What is connected, for whether setup is finished.
    Overview,
    Pane(todo::Hold),
}

/// A register the list keeps of its own: on this device across loads, or in this tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "TodosKept")]
pub enum Kept {
    Filter,
    By,
    /// The folds the viewer set, on this device, per chain.
    Folds,
    /// The todo open in place, in this tab, per chain.
    Opened,
    /// Whether the add sheet is up, in this tab, per chain.
    Sheet,
    /// The category the last todo was added in, on this device.
    Category,
    /// What is typed in the add sheet and not added, in this tab, per chain.
    Draft,
}

const KEPT: [Kept; 7] = [
    Kept::Filter,
    Kept::By,
    Kept::Folds,
    Kept::Opened,
    Kept::Sheet,
    Kept::Category,
    Kept::Draft,
];

/// What the page asks once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "TodosAsk")]
pub enum Ask {
    /// The reading of the objects the tab holds, at the fold's instant.
    Reading {
        at: String,
    },
    /// The tree, grown and priced: the answer to the page's ask of this number. A number, not
    /// the question, since a term's JSON does not come back from the host as it went (`-4.0` is
    /// read back as `-4`).
    Tree {
        n: u32,
    },
    Added,
    Sealed,
    /// An act on a line, or an undo or redo, at its phase.
    Line {
        todo: String,
        acting: Acting,
        phase: Phase,
    },
    Undo {
        todo: String,
    },
    /// The pane's todo page's, under a name of its own: the two asks are tagged by one key.
    Pane {
        ask: todo::Ask,
    },
}

/// The streams the page listens to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "TodosListen")]
pub enum Listen {
    Keys,
}

/// How the list groups; ancestry nests under each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grouping {
    Category,
    Urgency,
    Flat,
}

const GROUPINGS: [(Grouping, &str, &str); 3] = [
    (Grouping::Category, "category", "Category"),
    (Grouping::Urgency, "urgency", "Urgency"),
    (Grouping::Flat, "flat", "Flat"),
];

/// Which lifecycle states the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum States {
    #[default]
    Open,
    Done,
    All,
}

/// Any value, or one exact value, since `""` is itself a category.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    Any,
    Is(String),
}

impl Choice {
    fn takes(&self, v: &str) -> bool {
        match self {
            Choice::Any => true,
            Choice::Is(c) => c == v,
        }
    }
}

/// What the list is narrowed to, kept on this device as its print.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Filter {
    states: States,
    category: Choice,
    kind: Choice,
    query: String,
    attention: bool,
    waiting: bool,
    dated: bool,
}

impl Filter {
    /// Whatever the register holds becomes a filter: each field it cannot supply is the default.
    fn of(v: &Value) -> Filter {
        let choice = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .map_or(Choice::Any, |c| Choice::Is(c.to_owned()))
        };
        let flag = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or_default();
        Filter {
            states: v
                .get("state")
                .and_then(|s| serde_json::from_value(s.clone()).ok())
                .unwrap_or_default(),
            category: choice("category"),
            kind: choice("kind"),
            query: v
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            attention: flag("attention"),
            waiting: flag("waiting"),
            dated: flag("dated"),
        }
    }

    fn print(&self) -> Value {
        let choice = |c: &Choice| match c {
            Choice::Any => Value::Null,
            Choice::Is(v) => json!(v),
        };
        json!({"state": self.states, "category": choice(&self.category), "kind": choice(&self.kind),
            "query": self.query, "attention": self.attention, "waiting": self.waiting, "dated": self.dated})
    }

    /// What narrows the list, in words; nothing for the default.
    fn narrowing(&self) -> Vec<String> {
        let named = |c: &Choice, none: &str| match c {
            Choice::Any => None,
            Choice::Is(v) if v.is_empty() => Some(none.to_owned()),
            Choice::Is(v) => Some(v.clone()),
        };
        [
            match self.states {
                States::Open => None,
                States::Done => Some("Done".to_owned()),
                States::All => Some("Open and done".to_owned()),
            },
            named(&self.category, "uncategorised"),
            named(&self.kind, "no kind"),
            self.attention.then(|| "needs a look".to_owned()),
            self.waiting.then(|| "waiting".to_owned()),
            self.dated.then(|| "dated".to_owned()),
            (!self.query.trim().is_empty()).then(|| format!("“{}”", self.query.trim())),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Whether a row passes it at the fold's instant.
    fn takes(&self, e: &Entry, now: i64) -> bool {
        let done = !is_open(e);
        let states = match self.states {
            States::Open => !done,
            States::Done => done,
            States::All => true,
        };
        let a = e.authored.as_ref();
        let needle = self.query.trim().to_lowercase();
        states
            && (!self.attention || e.unconfirmed || !conflicts(e).is_empty())
            && (!self.waiting || a.is_some_and(|a| !a.waiting_on.is_empty()))
            && (!self.dated
                || a.and_then(|a| a.spec.as_ref())
                    .is_some_and(|s| dated(s, now)))
            && self.category.takes(a.map_or("", |a| a.category.as_str()))
            && self.kind.takes(a.map_or("", kind))
            && (needle.is_empty()
                || format!("{}\n{}", e.todo, a.map_or("", |a| a.body.as_str()))
                    .to_lowercase()
                    .contains(&needle))
    }
}

/// How long ahead a deadline counts as dated.
const FORTNIGHT_MS: i64 = 14 * 86_400_000;

/// Any `Decay` of the spec whose end falls in the fortnight from `now`, both ends in.
fn dated(spec: &Value, now: i64) -> bool {
    let own = spec.get("kind").and_then(Value::as_str) == Some("decay")
        && spec
            .get("endDate")
            .and_then(Value::as_str)
            .and_then(ms_of)
            .is_some_and(|at| at >= now && at <= now + FORTNIGHT_MS);
    own || children(spec).any(|c| dated(c, now))
}

/// Every term a term's fields hold, read structurally so a new constructor's child is never missed.
fn children(t: &Value) -> Box<dyn Iterator<Item = &Value> + '_> {
    match t {
        Value::Object(o) => Box::new(
            o.iter()
                .filter(|(k, _)| k.as_str() != "kind")
                .flat_map(|(_, v)| within(v)),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            Box::new(std::iter::empty())
        }
    }
}

fn within(v: &Value) -> Box<dyn Iterator<Item = &Value> + '_> {
    match v {
        Value::Object(o) if o.contains_key("kind") => Box::new(std::iter::once(v)),
        Value::Object(o) => Box::new(o.values().flat_map(within)),
        Value::Array(xs) => Box::new(xs.iter().flat_map(within)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            Box::new(std::iter::empty())
        }
    }
}

/// A node's worth: a number, `∅`, `∅` only while it refers into a chain the tab does not hold
/// yet, or why its term does not link.
#[derive(Debug, Clone, PartialEq)]
enum Worth {
    Value(f64),
    Absent,
    NotYet(String),
    Unlinked(String),
}

impl Worth {
    /// A term that does not link is a problem to fix and sorts first; `∅` has no number and
    /// sorts after every value, and one not yet priced after that.
    fn urgency(&self) -> f64 {
        match self {
            Worth::Value(v) => *v,
            Worth::Absent => f64::MAX,
            Worth::NotYet(_) => f64::INFINITY,
            Worth::Unlinked(_) => -1.0,
        }
    }

    fn band(&self) -> Band {
        match self {
            Worth::Value(v) => Band::of(*v),
            Worth::Absent | Worth::NotYet(_) => Band::Unpriced,
            Worth::Unlinked(_) => Band::Problem,
        }
    }

    fn number(&self) -> Option<f64> {
        match self {
            Worth::Value(v) => Some(*v),
            Worth::Absent | Worth::NotYet(_) | Worth::Unlinked(_) => None,
        }
    }
}

/// The screen's whole percent: the core's `Observation::PERCENT`.
fn whole(v: f64) -> f64 {
    (v.clamp(0.0, 1.0) * 100.0 + 0.5).floor() / 100.0
}

/// A node of the tree as the list draws it.
#[derive(Debug, Clone, PartialEq)]
struct Line {
    place: Place,
    worth: Worth,
}

/// Where a node stands, as the core's tree names it: the root, a group or a todo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", content = "v")]
enum Place {
    All,
    Category(String),
    Band(Band),
    Todo(String),
}

/// Where a value falls, as the core bands it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
enum Band {
    Problem,
    Watch,
    Fine,
    Unpriced,
}

impl Band {
    fn of(v: f64) -> Band {
        if v >= 0.7 {
            Band::Fine
        } else if v >= 0.5 {
            Band::Watch
        } else {
            Band::Problem
        }
    }

    fn label(self) -> &'static str {
        match self {
            Band::Problem => "Problem",
            Band::Watch => "Watch",
            Band::Fine => "Fine",
            Band::Unpriced => "Unpriced",
        }
    }
}

fn key(p: &Place) -> String {
    match p {
        Place::All => "all".to_owned(),
        Place::Category(c) => format!("c:{c}"),
        Place::Band(b) => format!("b:{}", b.label()),
        Place::Todo(t) => format!("t:{t}"),
    }
}

fn label(p: &Place) -> String {
    match p {
        Place::All => "Everything".to_owned(),
        Place::Category(c) if c.is_empty() => "uncategorised".to_owned(),
        Place::Category(c) => c.clone(),
        Place::Band(b) => b.label().to_owned(),
        Place::Todo(t) => t.clone(),
    }
}

fn todo_of(p: &Place) -> Option<&str> {
    match p {
        Place::Todo(t) => Some(t.as_str()),
        Place::All | Place::Category(_) | Place::Band(_) => None,
    }
}

/// A node of the tree as the worker answers it: where it stands, its term and its price.
#[derive(Debug, Deserialize)]
struct Grown {
    place: Place,
    term: Value,
    priced: Value,
    children: Vec<Grown>,
}

/// What a tree answers: its basis, its members and its grouping.
#[derive(Debug, Clone, PartialEq)]
struct Asked {
    of: Of,
    members: Vec<Placed>,
    by: Grouping,
}

/// A term's price: its value (`None` for `∅`), or why it does not link.
#[derive(Debug, Clone, PartialEq)]
enum Priced {
    Value(Option<f64>),
    Refused(String),
}

impl Priced {
    fn of(v: &Value) -> Priced {
        match v.get("type").and_then(Value::as_str) {
            Some("Value") => Priced::Value(v.get("v").and_then(Value::as_f64)),
            Some(_) | None => Priced::Refused(
                v.get("why")
                    .and_then(Value::as_str)
                    .unwrap_or("not priced")
                    .to_owned(),
            ),
        }
    }
}

/// The reading of the objects the tab holds: its name, each todo's candidate specs, each
/// todo's function, and the chains held beside it.
#[derive(Debug, Clone, PartialEq)]
struct Read {
    /// The instant the replica was read at.
    at: String,
    name: Value,
    specs: BTreeMap<String, Vec<Value>>,
    functions: BTreeMap<String, Value>,
    held: BTreeSet<String>,
}

/// What prices the tree: the box's rows at their instant, or a reading the worker keeps.
#[derive(Debug, Clone, PartialEq)]
enum Basis {
    Rows,
    Read(Box<Read>),
}

/// What names a basis, so an answer about one since replaced is dropped.
#[derive(Debug, Clone, PartialEq)]
enum Of {
    Rows(String),
    Read(Value),
}

/// The tree asked of the worker and not answered yet, if any.
#[derive(Debug, Clone, PartialEq)]
enum Wants {
    Nothing,
    /// Asked as its number.
    Asking(u32, Asked),
}

/// The last tree the worker answered.
#[derive(Debug, Clone)]
enum Drawn {
    Unpriced,
    /// The tree, the instant of the reading that priced it, and the question it answers.
    Tree(Tree<Line>, PricedBy, Box<Asked>),
}

/// What priced a tree: the box's rows, or the tab's reading of its replica at an instant.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PricedBy {
    Rows,
    Read(String),
}

/// Whether the search, the filters and the grouping stand open under the title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Find {
    Shut,
    Open,
}

/// The box's word on today's briefing and on setup: unknown until it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Today {
    Unknown,
    None,
    Briefed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Setup {
    Unknown,
    Finished,
    Unfinished { done: usize, of: usize },
}

/// The todo a new one is added under, by its id and its title.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Parent {
    todo: String,
    title: String,
}

/// Which field of the add sheet a refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum About {
    Body,
    Category,
    Price,
    Whole,
}

/// The add sheet's draft.
#[derive(Debug, Clone)]
struct Sheet {
    body: Markup,
    category: String,
    price: Pricing,
    rationale: String,
    parent: Option<Parent>,
    said: Option<(About, String)>,
}

/// Where the sheet's last todo stands: none asked, asked of the box to print, or printed and
/// being sealed, with what is said once it is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Adding {
    Typing,
    Printing,
    Sealing(String),
}

impl Sheet {
    fn blank() -> Sheet {
        Sheet {
            body: Markup::of(String::new()),
            category: String::new(),
            price: Pricing::default(),
            rationale: String::new(),
            parent: None,
            said: None,
        }
    }

    /// Its price, or the refusal and the field it is about, so the sheet says it on that line.
    fn read(&self) -> Result<Price, (About, String)> {
        if self.body.text.trim().is_empty() {
            Err((About::Body, "Say what the todo is.".to_owned()))
        } else if self.category.is_empty() {
            Err((About::Category, "Pick a category.".to_owned()))
        } else {
            self.price.read().map_err(|e| (About::Price, e))
        }
    }

    /// Emptied, its field opened again on `body`: a field opened on what it last opened on would
    /// keep what was typed into it since.
    fn open_on(&mut self, body: String) {
        let mut field = std::mem::replace(&mut self.body, Markup::of(String::new()));
        field.reset(body);
        *self = Sheet {
            body: field,
            ..Sheet::blank()
        };
    }
}

/// The pane beside the list: the hint, or the selected todo's page.
#[derive(Debug, Clone)]
enum Pane {
    Hint,
    Todo(Box<todo::Model>),
}

/// Everything the page knows.
#[derive(Debug, Clone)]
pub struct Model {
    context: Context,
    chain: String,
    now: i64,
    fold: Option<Folded>,
    unsent: Unsents,
    waiting: u32,
    route: Route,
    shared: String,
    filter: Filter,
    by: Grouping,
    folds: BTreeMap<String, bool>,
    opened: String,
    up: bool,
    last: String,
    today: Today,
    setup: Setup,
    find: Find,
    basis: Basis,
    grown: Drawn,
    wants: Wants,
    /// The registers this tab keeps that were read back since the page opened.
    restored: BTreeSet<Kept>,
    adding: Adding,
    asked: u32,
    problem: Option<String>,
    cursor: String,
    sheet: Sheet,
    running: BTreeSet<(String, Acting)>,
    keys: Vec<KeyKind>,
    pane: Pane,
}

/// A move of the add sheet.
#[derive(Debug, Clone)]
pub enum SheetMsg {
    Handle,
    Down,
    Body(String),
    Category(String),
    Price(PriceMsg),
    Rationale(String),
    Unparent,
    Add,
}

/// Everything that can happen to the page.
#[derive(Debug, Clone)]
pub enum Msg {
    Host(Box<Input<Hold, Ask, Listen>>),
    /// A card's or a line's fold, by its key, and whether it opens.
    Fold(String, bool),
    Tap(String, String),
    Find,
    By(Grouping),
    Narrow(Narrow),
    Act(String, Verb),
    Under(String),
    Sheet(SheetMsg),
    Pane(todo::Msg),
    Retry,
}

/// A move of the filter.
#[derive(Debug, Clone)]
pub enum Narrow {
    Query(String),
    States(States),
    Attention,
    Waiting,
    Dated,
    Category(Choice),
    Kind(Choice),
}

impl Narrow {
    fn of(self, f: &Filter) -> Filter {
        let f = f.clone();
        match self {
            Narrow::Query(query) => Filter { query, ..f },
            Narrow::States(states) => Filter { states, ..f },
            Narrow::Attention => Filter {
                attention: !f.attention,
                ..f
            },
            Narrow::Waiting => Filter {
                waiting: !f.waiting,
                ..f
            },
            Narrow::Dated => Filter {
                dated: !f.dated,
                ..f
            },
            Narrow::Category(category) => Filter { category, ..f },
            Narrow::Kind(kind) => Filter { kind, ..f },
        }
    }
}

/// The page as a program.
#[derive(Debug)]
pub enum Todos {}

type Fx = Effect<Hold, Ask, Listen>;

fn pane_fx(e: Effect<todo::Hold, todo::Ask>) -> Fx {
    e.map(Hold::Pane, |ask| Ask::Pane { ask }, |n| match n {})
}

impl Model {
    fn open(f: Flags) -> (Model, Vec<Fx>) {
        let m = Model {
            context: f.context,
            chain: f.chain,
            now: f.now,
            fold: None,
            unsent: Unsents::default(),
            waiting: 0,
            route: Route::default(),
            shared: String::new(),
            filter: Filter::default(),
            by: Grouping::Category,
            folds: BTreeMap::new(),
            opened: String::new(),
            up: false,
            last: String::new(),
            today: Today::Unknown,
            setup: Setup::Unknown,
            find: Find::Shut,
            basis: Basis::Rows,
            grown: Drawn::Unpriced,
            wants: Wants::Nothing,
            restored: BTreeSet::new(),
            adding: Adding::Typing,
            asked: 0,
            problem: None,
            cursor: String::new(),
            sheet: Sheet::blank(),
            running: BTreeSet::new(),
            keys: Vec::new(),
            pane: Pane::Hint,
        };
        let watch = |ask: Hold, local: Local| Effect::Watch { ask, local };
        let mut effects = vec![
            watch(Hold::Fold, Local::Fold),
            watch(Hold::Unsent, Local::Unsent),
            watch(Hold::Waiting, Local::Waiting),
            watch(Hold::Route, Local::Route),
            Effect::Hold {
                ask: Hold::Briefings,
                path: Question::Briefings.path(),
            },
            Effect::Hold {
                ask: Hold::Overview,
                path: Question::Overview.path(),
            },
        ];
        effects.extend(KEPT.map(|k| watch(Hold::Kept(k), m.local(k))));
        let mut m = m;
        effects.extend(m.declare());
        (m, effects)
    }

    /// Where a register is kept: its key, and whether this device or this tab keeps it.
    fn kept(&self, k: Kept) -> (String, bool) {
        match k {
            Kept::Filter => ("todos.filter".to_owned(), true),
            Kept::By => ("todos.by".to_owned(), true),
            Kept::Folds => (format!("todos.folds/{}", self.chain), true),
            Kept::Opened => (format!("todos.opened/{}", self.chain), false),
            Kept::Sheet => (format!("todos.sheet/{}", self.chain), false),
            Kept::Category => ("todos.category".to_owned(), true),
            Kept::Draft => (format!("todos.draft/{}", self.chain), false),
        }
    }

    fn local(&self, k: Kept) -> Local {
        match self.kept(k) {
            (key, true) => Local::Kept { key },
            (key, false) => Local::Ui { key },
        }
    }

    fn keep(&self, k: Kept, value: Value) -> Fx {
        let register = match self.kept(k) {
            (key, true) => Register::Kept { key },
            (key, false) => Register::Ui { key },
        };
        Effect::Keep { register, value }
    }

    fn writes(&self) -> bool {
        self.context.gate == Gate::Writes
    }

    fn entries(&self) -> &[Entry] {
        self.fold.as_ref().map_or(&[][..], |f| f.entries.as_slice())
    }

    fn entry(&self, todo: &str) -> Option<&Entry> {
        self.entries().iter().find(|e| e.todo == todo)
    }

    fn at_ms(&self) -> i64 {
        self.fold
            .as_ref()
            .and_then(|f| ms_of(&f.at))
            .unwrap_or(self.now)
    }

    fn of(&self) -> Of {
        match &self.basis {
            Basis::Rows => Of::Rows(self.fold.as_ref().map(|f| f.at.clone()).unwrap_or_default()),
            Basis::Read(r) => Of::Read(r.name.clone()),
        }
    }

    /// The members the tree places: the rows with content that are not decisions, that pass the
    /// filter or whose act this tab holds unsent, so its line can say where the act stands. The
    /// worker finds the parents in their specs.
    fn members(&self) -> Vec<Placed> {
        let now = self.at_ms();
        self.entries()
            .iter()
            .filter(|e| self.filter.takes(e, now) || self.unsent.todos.contains_key(&e.todo))
            .filter_map(|e| {
                let a = e.authored.as_ref().filter(|a| kind(a) != "decision")?;
                let specs = match &self.basis {
                    Basis::Read(r) => r.specs.get(&e.todo).cloned().unwrap_or_default(),
                    Basis::Rows => a.spec.clone().into_iter().collect(),
                };
                Some(Placed {
                    todo: e.todo.clone(),
                    title: headline(e),
                    category: a.category.clone(),
                    worth: match e.value {
                        Some(Fulfillment::Number(v)) => PlacedWorth::Value { v: Some(v) },
                        Some(Fulfillment::Absent(_)) => PlacedWorth::Value { v: None },
                        None => PlacedWorth::Unlinked,
                    },
                    specs,
                })
            })
            .collect()
    }

    fn question(&self) -> Asked {
        Asked {
            of: self.of(),
            members: self.members(),
            by: self.by,
        }
    }

    fn worth(&self, priced: Priced, term: &Value) -> Worth {
        match priced {
            Priced::Value(Some(v)) => Worth::Value(whole(v)),
            Priced::Value(None) => match &self.basis {
                Basis::Read(r) => unheld(term, &r.functions, &r.held, &mut BTreeSet::new())
                    .map_or(Worth::Absent, Worth::NotYet),
                Basis::Rows => Worth::Absent,
            },
            Priced::Refused(why) => Worth::Unlinked(why),
        }
    }

    /// The worker's tree as the list draws it: each node's worth, children ascending.
    fn lines(&self, g: Grown) -> Tree<Line> {
        let worth = self.worth(Priced::of(&g.priced), &g.term);
        let mut children: Vec<Tree<Line>> = g.children.into_iter().map(|c| self.lines(c)).collect();
        children.sort_by(|a, b| a.node.worth.urgency().total_cmp(&b.node.worth.urgency()));
        Tree {
            node: Line {
                place: g.place,
                worth,
            },
            children,
        }
    }

    /// The tree asked of the worker when what it answers moved; the last tree drawn stays
    /// while the next is grown.
    fn regrow(&mut self) -> Vec<Fx> {
        if self.fold.is_none() {
            return Vec::new();
        }
        let q = self.question();
        let drawn = matches!(&self.grown, Drawn::Tree(_, _, d) if **d == q);
        let asked = matches!(&self.wants, Wants::Asking(_, a) if *a == q);
        if drawn || asked {
            return Vec::new();
        }
        let n = self.asked.wrapping_add(1);
        self.asked = n;
        let question = Core::Tree {
            basis: self.basis_question(),
            members: q.members.clone(),
            by: match q.by {
                Grouping::Category => "category",
                Grouping::Urgency => "urgency",
                Grouping::Flat => "flat",
            }
            .to_owned(),
        };
        self.wants = Wants::Asking(n, q);
        vec![Effect::Core {
            ask: Ask::Tree { n },
            question,
        }]
    }

    fn basis_question(&self) -> crate::runtime::Basis {
        match &self.basis {
            Basis::Read(r) => crate::runtime::Basis::Fold {
                reading: r.name.clone(),
            },
            Basis::Rows => crate::runtime::Basis::Rows {
                at: self.fold.as_ref().map(|f| f.at.clone()).unwrap_or_default(),
                functions: self
                    .entries()
                    .iter()
                    .filter_map(|e| {
                        let t = match e.value? {
                            Fulfillment::Number(v) => json!({"kind": "flat", "value": v}),
                            Fulfillment::Absent(_) => json!({"kind": "absent"}),
                        };
                        Some((e.todo.clone(), t))
                    })
                    .collect(),
            },
        }
    }

    /// The folds the list draws: a card holding a line whose act is on its way stays open so
    /// the line can say so, unless the viewer set it.
    fn shown_folds(&self) -> BTreeMap<String, bool> {
        let mut out = BTreeMap::new();
        if let Drawn::Tree(t, ..) = &self.grown {
            let sending = |t: &Tree<Line>| {
                t.nodes()
                    .iter()
                    .any(|n| todo_of(&n.place).is_some_and(|id| self.unsent.todos.contains_key(id)))
            };
            for t in std::iter::once(t).chain(&t.children).filter(|t| sending(t)) {
                out.insert(key(&t.node.place), true);
            }
        }
        out.extend(self.folds.clone());
        out
    }

    fn cards(&self) -> Vec<Card<'_>> {
        let Drawn::Tree(root, ..) = &self.grown else {
            return Vec::new();
        };
        let folds = self.shown_folds();
        match self.by {
            Grouping::Flat if root.children.is_empty() => Vec::new(),
            Grouping::Flat => vec![card(&folds, root)],
            Grouping::Category | Grouping::Urgency => {
                root.children.iter().map(|t| card(&folds, t)).collect()
            }
        }
    }

    /// Where the cursor can stand: each card's head, then its lines.
    fn stops(&self) -> Vec<Stop> {
        self.cards()
            .into_iter()
            .flat_map(|c| {
                let head = Stop {
                    key: c.key.clone(),
                    at: At::Card(c.key.clone(), c.open),
                };
                std::iter::once(head).chain(c.lines.into_iter().map(|l| Stop {
                    key: l.key.clone(),
                    at: At::Line(l),
                }))
            })
            .collect()
    }

    /// The keys the page answers now: Escape only while it has something to close.
    fn wanted_keys(&self) -> Vec<KeyKind> {
        let mut keys = vec![
            KeyKind::Move,
            KeyKind::Open,
            KeyKind::Close,
            KeyKind::Act,
            KeyKind::Undo,
            KeyKind::Redo,
            KeyKind::Search,
            KeyKind::Add,
        ];
        if self.up || !self.opened.is_empty() || !self.cursor.is_empty() {
            keys.push(KeyKind::Escape);
        }
        keys
    }

    fn declare(&mut self) -> Vec<Fx> {
        let keys = self.wanted_keys();
        if keys == self.keys {
            return Vec::new();
        }
        self.keys.clone_from(&keys);
        vec![Effect::Keys {
            ask: Listen::Keys,
            keys,
        }]
    }

    fn update(&mut self, msg: Msg) -> Vec<Fx> {
        let mut out = self.step(msg);
        out.extend(self.regrow());
        out.extend(self.declare());
        out
    }

    fn step(&mut self, msg: Msg) -> Vec<Fx> {
        match msg {
            Msg::Host(input) => self.heard(*input),
            Msg::Fold(k, open) => {
                self.folds.insert(k, open);
                vec![self.keep(Kept::Folds, json!(self.folds))]
            }
            Msg::Tap(todo, at) => {
                self.cursor = at;
                if self.context.wide {
                    return vec![Effect::Go {
                        href: To::Todo(&todo).href(),
                    }];
                }
                self.opened = if self.opened == todo {
                    String::new()
                } else {
                    todo
                };
                vec![self.keep(Kept::Opened, json!(self.opened))]
            }
            Msg::Find => {
                self.find = match self.find {
                    Find::Shut => Find::Open,
                    Find::Open => Find::Shut,
                };
                Vec::new()
            }
            Msg::By(by) => {
                self.by = by;
                vec![self.keep(Kept::By, json!(by))]
            }
            Msg::Narrow(n) => {
                self.filter = n.of(&self.filter);
                vec![self.keep(Kept::Filter, self.filter.print())]
            }
            Msg::Act(todo, verb) => self.act(&todo, Acting::Verb(verb)),
            Msg::Under(todo) => {
                let title = self.entry(&todo).map_or_else(|| todo.clone(), headline);
                let category = self
                    .entry(&todo)
                    .and_then(|e| e.authored.as_ref())
                    .map(|a| a.category.clone())
                    .unwrap_or_default();
                self.sheet.open_on(String::new());
                self.sheet.category = if self.context.categories.contains(&category) {
                    category
                } else {
                    String::new()
                };
                self.sheet.parent = Some(Parent { todo, title });
                self.raise()
            }
            Msg::Sheet(m) => self.sheeted(m),
            Msg::Retry => self.reread(),
            Msg::Pane(m) => match &mut self.pane {
                Pane::Todo(p) => p.update(m).into_iter().map(pane_fx).collect(),
                Pane::Hint => Vec::new(),
            },
        }
    }

    /// The sheet up, its category the list's narrowing or the last one used, the keyboard in
    /// its body.
    fn raise(&mut self) -> Vec<Fx> {
        self.up = true;
        if self.sheet.category.is_empty() {
            self.sheet.category = self.preselect();
        }
        vec![
            self.keep(Kept::Sheet, json!(true)),
            Effect::Focus {
                field: "sheetbody".to_owned(),
            },
        ]
    }

    /// The category a new todo starts in, never one the deployment no longer has.
    fn preselect(&self) -> String {
        let narrowed = match &self.filter.category {
            Choice::Is(c) => c.clone(),
            Choice::Any => String::new(),
        };
        [narrowed, self.last.clone()]
            .into_iter()
            .find(|c| !c.is_empty() && self.context.categories.contains(c))
            .unwrap_or_default()
    }

    fn lower(&mut self) -> Vec<Fx> {
        self.up = false;
        self.sheet.parent = None;
        vec![self.keep(Kept::Sheet, json!(false))]
    }

    fn sheeted(&mut self, m: SheetMsg) -> Vec<Fx> {
        let typed = matches!(
            m,
            SheetMsg::Body(_) | SheetMsg::Category(_) | SheetMsg::Rationale(_)
        );
        let mut out = self.sheet_step(m);
        if typed {
            out.push(self.keep(
                Kept::Draft,
                json!({"body": self.sheet.body.text, "category": self.sheet.category, "rationale": self.sheet.rationale}),
            ));
        }
        out
    }

    fn sheet_step(&mut self, m: SheetMsg) -> Vec<Fx> {
        match m {
            SheetMsg::Handle if self.up => self.lower(),
            SheetMsg::Handle => self.raise(),
            SheetMsg::Down => self.lower(),
            SheetMsg::Body(t) => {
                self.sheet.body.text = t;
                Vec::new()
            }
            SheetMsg::Category(c) => {
                self.sheet.category = c;
                Vec::new()
            }
            SheetMsg::Price(p) => {
                self.sheet.price.update(p);
                Vec::new()
            }
            SheetMsg::Rationale(r) => {
                self.sheet.rationale = r;
                Vec::new()
            }
            SheetMsg::Unparent => {
                self.sheet.parent = None;
                Vec::new()
            }
            SheetMsg::Add => self.add(),
        }
    }

    fn add(&mut self) -> Vec<Fx> {
        if self.adding != Adding::Typing || !self.writes() {
            return Vec::new();
        }
        let s = &self.sheet;
        let body = s.body.text.trim();
        match s.read() {
            Err((about, why)) => {
                self.sheet.said = Some((about, why.clone()));
                vec![Effect::Toast { words: why }]
            }
            Ok(price) => {
                let todo = NewTodo {
                    fields: TodoFields {
                        body: body.to_owned(),
                        rationale: s.rationale.trim().to_owned(),
                        kind: "todo".to_owned(),
                        category: s.category.clone(),
                        price,
                        detail: String::new(),
                        waiting_on: String::new(),
                    },
                    id: None,
                    parent: s.parent.as_ref().map(|p| p.todo.clone()),
                };
                self.sheet.said = None;
                self.adding = Adding::Printing;
                vec![author(
                    Ask::Added,
                    &self.chain,
                    Authoring::Add(Box::new(todo)),
                )]
            }
        }
    }

    fn act(&mut self, todo: &str, acting: Acting) -> Vec<Fx> {
        if !self.writes() || !self.running.insert((todo.to_owned(), acting)) {
            return Vec::new();
        }
        vec![begin(&self.chain, todo, acting, String::new(), |phase| {
            Ask::Line {
                todo: todo.to_owned(),
                acting,
                phase,
            }
        })]
    }

    fn heard(&mut self, input: Input<Hold, Ask, Listen>) -> Vec<Fx> {
        match input {
            Input::Context(c) => {
                let mut out = Vec::new();
                if let Pane::Todo(p) = &mut self.pane {
                    out = p
                        .update(todo::Msg::Host(Box::new(Input::Context(c.clone()))))
                        .into_iter()
                        .map(pane_fx)
                        .collect();
                }
                self.context = c;
                out
            }
            Input::Held {
                ask,
                reading,
                behind,
            } => self.held(ask, reading, behind),
            Input::Answered { ask, answer } => self.answered(ask, answer),
            Input::Heard {
                ask: Listen::Keys,
                frame,
            } => match serde_json::from_str::<Keyed>(&frame) {
                Ok(k) => self.key(k),
                Err(_) => Vec::new(),
            },
            Input::Opened { ask: Listen::Keys } => Vec::new(),
        }
    }

    /// A kept register read back: a value it cannot parse reads as its default.
    ///
    /// A register this tab keeps is read back once, when the page opens: it holds what the page
    /// wrote itself, so a later reading is an echo of a write the model already made, and one
    /// heard late would undo a newer one. A register this device keeps is heard always, as
    /// another tab may write it.
    fn kept_read(&mut self, k: Kept, v: Value) {
        if let (_, false) = self.kept(k)
            && !self.restored.insert(k)
        {
            return;
        }
        match k {
            Kept::Filter => self.filter = Filter::of(&v),
            Kept::By => self.by = serde_json::from_value(v).unwrap_or(Grouping::Category),
            Kept::Folds => {
                self.folds = serde_json::from_value::<BTreeMap<String, Value>>(v)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|(k, v)| v.as_bool().map(|b| (k, b)))
                    .collect();
            }
            Kept::Opened => v.as_str().unwrap_or_default().clone_into(&mut self.opened),
            Kept::Sheet => self.up = v.as_bool().unwrap_or_default(),
            Kept::Category => v.as_str().unwrap_or_default().clone_into(&mut self.last),
            // A sheet drawn again opens on what was typed into it before, once.
            Kept::Draft => {
                let word = |k: &str| {
                    v.get(k)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                if self.sheet.body.text.is_empty() && !word("body").is_empty() {
                    self.sheet.body.reset(word("body"));
                    self.sheet.category = word("category");
                    self.sheet.rationale = word("rationale");
                }
            }
        }
    }

    fn held(
        &mut self,
        ask: Hold,
        reading: Reading<Answer>,
        behind: Option<crate::runtime::Behind>,
    ) -> Vec<Fx> {
        let value = |r: Reading<Answer>| match read::<Value>(r) {
            Reading::Val { v } => Some(v),
            Reading::Loading | Reading::Error { .. } => None,
        };
        match ask {
            Hold::Fold => {
                match read::<Option<Folded>>(reading) {
                    Reading::Val { v } => {
                        let moved = self.fold.as_ref().map(|f| f.at.clone())
                            != v.as_ref().map(|f| f.at.clone());
                        self.fold = v;
                        self.problem = None;
                        if moved {
                            return self.reread();
                        }
                    }
                    Reading::Error { e } => self.problem = Some(e),
                    Reading::Loading => {}
                }
                Vec::new()
            }
            Hold::Unsent => {
                if let Reading::Val { v } = read::<Unsents>(reading) {
                    self.unsent = v;
                    return self.reread();
                }
                Vec::new()
            }
            Hold::Waiting => {
                self.waiting = value(reading)
                    .and_then(|v| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok())
                    .unwrap_or_default();
                Vec::new()
            }
            Hold::Route => {
                let Reading::Val { v } = read::<Route>(reading) else {
                    return Vec::new();
                };
                self.routed(v)
            }
            Hold::Kept(k) => {
                self.kept_read(k, value(reading).unwrap_or(Value::Null));
                Vec::new()
            }
            Hold::Briefings => {
                let today = day_ms(&self.context.offsets, self.now).to_string();
                self.today = match read::<BriefingsPayload>(reading) {
                    Reading::Val { v } if v.briefings.iter().any(|b| b.date == today) => {
                        Today::Briefed(today)
                    }
                    Reading::Val { .. } | Reading::Error { .. } => Today::None,
                    Reading::Loading => Today::Unknown,
                };
                Vec::new()
            }
            Hold::Overview => {
                if let Reading::Val { v } = read::<Overview>(reading) {
                    let lines: Vec<_> = v.groups.iter().flat_map(|g| &g.lines).collect();
                    let done = lines.iter().filter(|l| !l.standing.asks()).count();
                    self.setup = if done == lines.len() {
                        Setup::Finished
                    } else {
                        Setup::Unfinished {
                            done,
                            of: lines.len(),
                        }
                    };
                }
                Vec::new()
            }
            Hold::Pane(h) => match &mut self.pane {
                Pane::Todo(p) => p
                    .update(todo::Msg::Host(Box::new(Input::Held {
                        ask: h,
                        reading,
                        behind,
                    })))
                    .into_iter()
                    .map(pane_fx)
                    .collect(),
                Pane::Hint => Vec::new(),
            },
        }
    }

    /// The reading of the objects the tab holds asked again at the fold's instant: another
    /// commit, another chain moving or the fold moving reprices what refers into them.
    fn reread(&self) -> Vec<Fx> {
        self.fold
            .as_ref()
            .map(|f| Effect::Core {
                ask: Ask::Reading { at: f.at.clone() },
                question: Core::Reading {
                    chain: self.chain.clone(),
                    at: f.at.clone(),
                },
            })
            .into_iter()
            .collect()
    }

    /// Where the app went: a todo selected for the pane, or text shared into the sheet.
    fn routed(&mut self, r: Route) -> Vec<Fx> {
        let mut out = Vec::new();
        let shown = match &self.pane {
            Pane::Todo(p) => p.id().to_owned(),
            Pane::Hint => String::new(),
        };
        if r.selected != shown {
            if r.selected.is_empty() {
                self.pane = Pane::Hint;
            } else {
                let (p, effects) = todo::Model::open(todo::Flags {
                    context: self.context.clone(),
                    chain: self.chain.clone(),
                    id: r.selected.clone(),
                    pushed: false,
                    now: self.now,
                });
                self.pane = Pane::Todo(Box::new(p));
                out.extend(effects.into_iter().map(pane_fx));
            }
        }
        // A share is consumed, not a place: its words go into the sheet and the address back to
        // the list's, with no history entry; the address gone back, the same link shared again
        // is a new share.
        if r.shared.is_empty() {
            self.shared.clear();
        } else if r.shared != self.shared {
            self.shared.clone_from(&r.shared);
            self.sheet.open_on(r.shared.clone());
            out.extend(self.raise());
            out.push(Effect::Replace {
                href: if self.chain == PROJECT {
                    "#/project".to_owned()
                } else {
                    To::Todos.href()
                },
            });
        }
        self.route = r;
        out
    }

    fn answered(&mut self, ask: Ask, answer: Answer) -> Vec<Fx> {
        match ask {
            Ask::Reading { at } => self.read_at(&at, answer),
            Ask::Tree { n } => self.tree(n, answer),
            Ask::Added => match sealed(&Ask::Sealed, &self.chain, answer) {
                Ok((todo, seals)) => {
                    let mut out = self.printed(&todo);
                    out.extend(seals);
                    out
                }
                Err(words) => {
                    self.adding = Adding::Typing;
                    self.sheet.said = Some((About::Whole, words.clone()));
                    vec![Effect::Toast { words }]
                }
            },
            Ask::Sealed => self.taken(answer),
            Ask::Line {
                todo,
                acting,
                phase,
            } => self.lined(todo, acting, phase, answer),
            Ask::Undo { todo } => self.act(&todo, Acting::Undo),
            Ask::Pane { ask: a } => match &mut self.pane {
                Pane::Todo(p) => p
                    .update(todo::Msg::Host(Box::new(Input::Answered {
                        ask: a,
                        answer,
                    })))
                    .into_iter()
                    .map(pane_fx)
                    .collect(),
                Pane::Hint => Vec::new(),
            },
        }
    }

    /// The box printed the sheet's todo: the sheet goes down, emptied, its category kept, and its
    /// seals go to the replica. It goes down now, not once they are taken, as the page may be
    /// left before they are, and its registers would then open the next one on this one.
    fn printed(&mut self, todo: &str) -> Vec<Fx> {
        self.adding = Adding::Sealing(match &self.sheet.parent {
            Some(p) => format!("added {todo} under {}", p.todo),
            None => format!("added {todo}"),
        });
        let category = self.sheet.category.clone();
        self.sheet.open_on(String::new());
        self.last.clone_from(&category);
        let mut out = vec![
            self.keep(Kept::Category, json!(category)),
            self.keep(Kept::Draft, Value::Null),
        ];
        out.extend(self.lower());
        out
    }

    /// The sheet's todo taken by the replica, or refused, said once: a subtodo is two seals, the
    /// parent's revision after it, and the first says it.
    fn taken(&mut self, answer: Answer) -> Vec<Fx> {
        let Adding::Sealing(added) = std::mem::replace(&mut self.adding, Adding::Typing) else {
            return Vec::new();
        };
        let words = match answered::<Outcome>(answer) {
            Ok(
                Outcome::Answered {
                    answer: Said::Taken,
                    ..
                }
                | Outcome::Already,
            ) => added,
            Ok(Outcome::Answered {
                answer: Said::Refused { e } | Said::Lost { e },
                ..
            })
            | Err(e) => e,
        };
        vec![Effect::Toast { words }]
    }

    fn lined(&mut self, todo: String, acting: Acting, phase: Phase, answer: Answer) -> Vec<Fx> {
        let ask = |phase| Ask::Line {
            todo: todo.clone(),
            acting,
            phase,
        };
        match came(&self.chain, phase, answer, ask) {
            Came::Seals(seals) => seals,
            Came::Already => {
                self.running.remove(&(todo, acting));
                Vec::new()
            }
            Came::Taken => {
                self.running.remove(&(todo.clone(), acting));
                said(acting, Ask::Undo { todo })
            }
            Came::Refused(words) => {
                self.running.remove(&(todo, acting));
                vec![Effect::Toast { words }]
            }
        }
    }

    fn read_at(&mut self, at: &str, answer: Answer) -> Vec<Fx> {
        if self.fold.as_ref().map(|f| f.at.as_str()) != Some(at) {
            return Vec::new();
        }
        match answered::<Value>(answer) {
            Err(e) => self.problem = Some(e),
            Ok(v) => {
                let read = &v["read"];
                if read.get("type").and_then(Value::as_str) == Some("Read") {
                    self.basis = Basis::Read(Box::new(Read {
                        at: at.to_owned(),
                        name: read["reading"].clone(),
                        specs: serde_json::from_value(read["specs"].clone()).unwrap_or_default(),
                        functions: serde_json::from_value(read["functions"].clone())
                            .unwrap_or_default(),
                        held: serde_json::from_value(v["held"].clone()).unwrap_or_default(),
                    }));
                    self.problem = None;
                }
            }
        }
        Vec::new()
    }

    /// The worker's tree asked as `n`, drawn while its basis stands; an answer to an ask since
    /// replaced is dropped, as the newer one is on its way, and a failed one keeps the last tree.
    fn tree(&mut self, n: u32, answer: Answer) -> Vec<Fx> {
        let Wants::Asking(asked, q) = std::mem::replace(&mut self.wants, Wants::Nothing) else {
            return Vec::new();
        };
        if asked != n {
            self.wants = Wants::Asking(asked, q);
            return Vec::new();
        }
        if q.of != self.of() {
            return Vec::new();
        }
        match answered::<Grown>(answer) {
            Ok(g) => {
                let priced = match &self.basis {
                    Basis::Read(r) => PricedBy::Read(r.at.clone()),
                    Basis::Rows => PricedBy::Rows,
                };
                self.grown = Drawn::Tree(self.lines(g), priced, Box::new(q));
            }
            Err(why) => self.problem = Some(why),
        }
        Vec::new()
    }

    /// `l`, Enter: unfold, then open in place (a pane when wide), then go to the todo.
    fn open_at(&mut self, at: Option<&At>) -> Vec<Fx> {
        match at {
            Some(At::Card(k, false)) => self.step(Msg::Fold(k.clone(), true)),
            Some(At::Line(l)) => {
                let folds = self.shown_folds();
                let fk = key(&l.place);
                let id = todo_of(&l.place).unwrap_or_default().to_owned();
                if l.below && !is_open_at(&folds, &fk, &l.worth) {
                    self.step(Msg::Fold(fk, true))
                } else if self.context.wide || self.opened == id {
                    vec![Effect::Go {
                        href: To::Todo(&id).href(),
                    }]
                } else {
                    self.opened = id;
                    vec![self.keep(Kept::Opened, json!(self.opened))]
                }
            }
            Some(At::Card(_, true)) | None => Vec::new(),
        }
    }

    /// `h`: close what is open in place, then fold, then climb to the parent.
    fn close_at(&mut self, at: Option<&At>) -> Vec<Fx> {
        match at {
            Some(At::Card(k, true)) => self.step(Msg::Fold(k.clone(), false)),
            Some(At::Line(l)) => {
                let folds = self.shown_folds();
                let fk = key(&l.place);
                let id = todo_of(&l.place).unwrap_or_default().to_owned();
                if self.opened == id {
                    self.opened.clear();
                    vec![self.keep(Kept::Opened, json!(""))]
                } else if l.below && is_open_at(&folds, &fk, &l.worth) {
                    self.step(Msg::Fold(fk, false))
                } else {
                    let parent = l.parent.clone();
                    self.cursor_to(parent)
                }
            }
            Some(At::Card(_, false)) | None => Vec::new(),
        }
    }

    fn cursor_to(&mut self, k: String) -> Vec<Fx> {
        self.cursor.clone_from(&k);
        vec![Effect::Focus { field: k }]
    }

    fn key(&mut self, k: Keyed) -> Vec<Fx> {
        let stops: Vec<(String, At)> = self.stops().into_iter().map(|s| (s.key, s.at)).collect();
        let here = stops.iter().position(|(k, _)| *k == self.cursor);
        let todo_here = here.and_then(|i| match &stops[i].1 {
            At::Line(l) => todo_of(&l.place).map(str::to_owned),
            At::Card(..) => None,
        });
        match k {
            Keyed::Move { by } => {
                if stops.is_empty() {
                    return Vec::new();
                }
                let last = stops.len() - 1;
                let to = match here {
                    None if by > 0 => 0,
                    None => last,
                    Some(i) => {
                        usize::try_from(i64::try_from(i).unwrap_or_default() + i64::from(by))
                            .unwrap_or_default()
                            .min(last)
                    }
                };
                let k = stops[to].0.clone();
                self.cursor_to(k)
            }
            Keyed::Open => self.open_at(here.map(|i| &stops[i].1)),
            Keyed::Close => self.close_at(here.map(|i| &stops[i].1)),
            Keyed::Act { name } => {
                let Some(e) = todo_here.as_deref().and_then(|id| self.entry(id)).cloned() else {
                    return Vec::new();
                };
                let verb = match name {
                    ActName::Done => finish(&e),
                    ActName::Tend => Verb::Tend,
                    ActName::Cancel => Verb::Cancel,
                    ActName::Reopen => Verb::Reopen,
                    ActName::Confirm => Verb::Confirm,
                };
                if applicable(&e).contains(&verb) {
                    self.act(&e.todo, Acting::Verb(verb))
                } else {
                    let state = serde_json::to_value(e.state)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default();
                    vec![Effect::Toast {
                        words: format!(
                            "{} is {state} — nothing to {}",
                            e.todo,
                            if verb == Verb::Done {
                                "complete"
                            } else {
                                verb.label()
                            }
                        ),
                    }]
                }
            }
            Keyed::Undo => todo_here
                .map(|t| self.act(&t, Acting::Undo))
                .unwrap_or_default(),
            Keyed::Redo => todo_here
                .map(|t| self.act(&t, Acting::Redo))
                .unwrap_or_default(),
            Keyed::Search => {
                self.find = Find::Open;
                vec![Effect::Focus {
                    field: "search".to_owned(),
                }]
            }
            Keyed::Add => self.raise(),
            Keyed::Escape => {
                if self.up {
                    self.lower()
                } else if !self.opened.is_empty() {
                    self.opened.clear();
                    vec![self.keep(Kept::Opened, json!(""))]
                } else {
                    self.cursor.clear();
                    Vec::new()
                }
            }
        }
    }
}

/// A card of the list: a group, or the whole list when nothing groups it.
struct Card<'a> {
    key: String,
    tree: &'a Tree<Line>,
    open: bool,
    lines: Vec<Shown>,
}

/// A line as drawn: its node, how deep, its key, its parent's key, and whether it has lines
/// under it and how many todos.
#[derive(Debug, Clone)]
struct Shown {
    place: Place,
    worth: Worth,
    depth: usize,
    key: String,
    parent: String,
    below: bool,
    count: usize,
}

/// Where the cursor can stand: a card's head (its key, whether it is open, its worth), or a
/// line.
struct Stop {
    key: String,
    at: At,
}

enum At {
    Card(String, bool),
    Line(Shown),
}

fn card<'a>(folds: &BTreeMap<String, bool>, t: &'a Tree<Line>) -> Card<'a> {
    let k = key(&t.node.place);
    let open = is_open_at(folds, &k, &t.node.worth);
    let mut lines = Vec::new();
    if open {
        visible(folds, &t.children, 0, &k, &mut lines);
    }
    Card {
        key: k,
        tree: t,
        open,
        lines,
    }
}

/// The first chain a term refers into that the tab does not hold, following its own chain's
/// references through `functions`: what its `∅` waits on.
fn unheld(
    t: &Value,
    functions: &BTreeMap<String, Value>,
    held: &BTreeSet<String>,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    let field = |k: &str| t.get(k).and_then(Value::as_str);
    match (
        t.get("kind").and_then(Value::as_str),
        field("store"),
        field("todo"),
    ) {
        (Some("refIn"), Some(store), _) if !held.contains(store) => Some(store.to_owned()),
        (Some("ref"), _, Some(todo)) => {
            if !seen.insert(todo.to_owned()) {
                return None;
            }
            functions
                .get(todo)
                .and_then(|f| unheld(f, functions, held, seen))
        }
        _ => children(t).find_map(|c| unheld(c, functions, held, seen)),
    }
}

/// A fold nobody has touched is open exactly when its worth is short of Fine.
fn is_open_at(folds: &BTreeMap<String, bool>, k: &str, w: &Worth) -> bool {
    folds
        .get(k)
        .copied()
        .unwrap_or(matches!(w.band(), Band::Problem | Band::Watch))
}

/// The distinct todos a subtree places.
fn todos_in(t: &Tree<Line>) -> BTreeSet<&str> {
    t.nodes().iter().filter_map(|n| todo_of(&n.place)).collect()
}

/// Preorder, below a card, of every line whose ancestors are all open.
fn visible(
    folds: &BTreeMap<String, bool>,
    kids: &[Tree<Line>],
    depth: usize,
    parent: &str,
    out: &mut Vec<Shown>,
) {
    for k in kids {
        let at = format!("{parent}/{}", key(&k.node.place));
        out.push(Shown {
            place: k.node.place.clone(),
            worth: k.node.worth.clone(),
            depth,
            key: at.clone(),
            parent: parent.to_owned(),
            below: !k.children.is_empty(),
            count: todos_in(k).len(),
        });
        if !k.children.is_empty() && is_open_at(folds, &key(&k.node.place), &k.node.worth) {
            visible(folds, &k.children, depth + 1, &at, out);
        }
    }
}

/// The most urgent todo below, the one a folded card keeps in sight.
fn peek(t: &Tree<Line>) -> Option<&Line> {
    t.nodes()
        .into_iter()
        .filter(|n| todo_of(&n.place).is_some())
        .fold(None, |best: Option<&Line>, n| match best {
            Some(b) if b.worth.urgency() <= n.worth.urgency() => Some(b),
            Some(_) | None => Some(n),
        })
}

/// A node's mark: its pie with its number on hover, or the words for one with no number.
fn worth_mark(w: &Worth) -> Node<Msg> {
    match w {
        Worth::Value(v) => view! { Span "mark" {Title => percent(*v)} [pie(*v)] },
        Worth::Absent => view! { Span "mark" [chip("unpriced", "")] },
        Worth::NotYet(chain) => {
            view! { Span "mark" {Title => format!("it refers into {chain}, which this device does not hold yet")} [chip("not yet priced", "")] }
        }
        Worth::Unlinked(why) => {
            view! { Span "mark unlinked" {Role => "img", Title => why.clone(), AriaLabel => "no value"} }
        }
    }
}

impl Model {
    fn title(e: Option<&Entry>, id: &str) -> Node<Msg> {
        match e.and_then(|e| e.authored.as_ref().map(|a| (e, a))) {
            // A title is a block once its row carries the HTML (a fold fills it from this
            // device's kept blocks): placed by key alone, it would draw its source and then its
            // HTML whenever a join landed, so the same readings would draw two lists.
            Some((e, a)) if !e.unconfirmed && a.rich.body.is_some() => {
                view! { Span "rowtitle" [a.rich.keys.body.as_deref().map(|k| viewtree::node::block(k, headline(e)))] }
            }
            Some((e, _)) => view! { Span "rowtitle assource" [headline(e)] },
            None => view! { Span "rowtitle" [id] },
        }
    }

    fn said(u: Unsent, e: Option<&Entry>) -> &'static str {
        match (u, e.map(|e| e.state)) {
            (Unsent::Refused, _) => "refused: open it",
            (Unsent::AwaitingTrust, _) => "waits for this device to be trusted",
            (Unsent::Sending, Some(suzatary_core::wire::EntryState::Completed)) => "done · sending",
            (Unsent::Sending, Some(suzatary_core::wire::EntryState::Cancelled)) => {
                "cancelled · sending"
            }
            (Unsent::Sending, Some(suzatary_core::wire::EntryState::Open) | None) => "sending",
        }
    }

    fn line(&self, l: &Shown) -> Vec<(String, Node<Msg>)> {
        let id = todo_of(&l.place).unwrap_or_default();
        let e = self.entry(id);
        let folds = self.shown_folds();
        let fk = key(&l.place);
        let open = is_open_at(&folds, &fk, &l.worth);
        let pending = self.unsent.todos.get(id).copied();
        let below = l.count.saturating_sub(1);
        let opened = self.opened == id;
        let selected = self.route.selected == id;
        let class = [
            Some("row"),
            (below > 0).then_some("parent"),
            pending.map(|_| "pending"),
            pending.map(|u| match u {
                Unsent::Sending => "act-Sending",
                Unsent::AwaitingTrust => "act-AwaitingTrust",
                Unsent::Refused => "act-Refused",
            }),
            opened.then_some("inplace"),
            selected.then_some("selected"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        let fold = if l.below {
            let todo = fk.clone();
            view! { Button "rowfold" {Type => "button", AriaExpanded => if open { "true" } else { "false" }, AriaLabel => format!("{} {id}", if open { "fold" } else { "unfold" })} @click(Msg::Fold(todo, !open)) [mark("chevron")] }
        } else {
            view! { Span "rowgap" }
        };
        let main = view! { Button "rowmain" {Type => "button", Data("key") => l.key.clone(), Data("field") => l.key.clone(), AriaExpanded => if opened { "true" } else { "false" }} @click(Msg::Tap(id.to_owned(), l.key.clone())) [
            worth_mark(&l.worth),
            Self::title(e, id),
            e.filter(|e| e.unconfirmed).map(|_| view! { Span "chip prov" {Title => "Not confirmed yet. The list shows its words; open it to read what was written and confirm it."} ["unconfirmed"] }),
            e.filter(|e| !conflicts(e).is_empty()).map(|_| chip("conflict", "prov")),
            pending.map(|u| chip(Self::said(u, e), "queuedmark")),
            (below > 0).then(|| view! { Span "rowcount" {Title => format!("{below} below")} [below.to_string()] }),
        ] };
        let row = view! { Div {Class => class, Data("depth") => l.depth.to_string(), Data("todo") => id.to_owned(), Data("key") => l.key.clone(), Data("worth") => l.worth.number().map_or_else(|| "null".to_owned(), |v| v.to_string()), AriaCurrent => selected.then_some("true")} [fold, main] };
        let mut out = vec![(l.key.clone(), row)];
        if let Some(e) = e.filter(|_| opened) {
            out.push((format!("{}#open", l.key), self.panel(e, l.depth)));
        }
        out
    }

    /// A line opened in place: what it waits on, then Open, its act, Ask and Add a subtodo.
    fn panel(&self, e: &Entry, depth: usize) -> Node<Msg> {
        let id = e.todo.as_str();
        let waiting = e
            .authored
            .as_ref()
            .map(|a| a.waiting_on.as_str())
            .filter(|w| !w.is_empty());
        let acts: Vec<Node<Msg>> = if self.writes() {
            applicable(e)
                .into_iter()
                .filter(|v| matches!(v, Verb::Done | Verb::Tend | Verb::Reopen))
                .map(|v| {
                    let busy = self.running.contains(&(id.to_owned(), Acting::Verb(v)));
                    view! { Button {Type => "button", Class => format!("{} capsule", v.class()), Disabled => busy} @click(Msg::Act(id.to_owned(), v)) [v.label()] }
                })
                .collect()
        } else {
            Vec::new()
        };
        let ask = format!("todo:{}/{id}", self.chain);
        view! { Div "rowopen" {Data("depth") => depth.to_string(), Data("todo") => id.to_owned()} [
            waiting.map(|w| view! { P "waiting" [view! { Span "waitlabel" ["waiting on "] }, view! { Span "waitingtext" [crate::todoparts::preview(w)] }] }),
            view! { Div "rowacts" [
                view! { A "capsule primary" {Href => To::Todo(id).href()} ["Open"] },
                acts,
                view! { A "capsule" {Href => To::Ask(&ask).href()} ["Ask"] },
                (self.writes() && is_open(e)).then(|| view! { Button "capsule addsub" {Type => "button"} @click(Msg::Under(id.to_owned())) ["Add a subtodo"] }),
            ] },
        ] }
    }

    fn card(&self, c: &Card<'_>) -> Node<Msg> {
        let n = &c.tree.node;
        let count = todos_in(c.tree).len();
        let head = view! { Button "grouphead" {Type => "button", Data("key") => c.key.clone(), Data("field") => c.key.clone(), AriaExpanded => if c.open { "true" } else { "false" }} @click(Msg::Fold(c.key.clone(), !c.open)) [
            mark("chevron"),
            view! { Span "grouplabel" [label(&n.place)] },
            worth_mark(&n.worth),
            view! { Span "groupcount" [format!("{count} open")] },
        ] };
        let rows: Vec<(String, Node<Msg>)> = if c.open {
            c.lines.iter().flat_map(|l| self.line(l)).collect()
        } else {
            peek(c.tree)
                .map(|p| {
                    let id = todo_of(&p.place).unwrap_or_default();
                    (format!("{}#peek", c.key), view! { Div "peek" {Data("todo") => id.to_owned()} [worth_mark(&p.worth), Self::title(self.entry(id), id)] })
                })
                .into_iter()
                .collect()
        };
        view! { Section {Class => if c.open { "group open" } else { "group" }, Data("group") => c.key.clone(), Data("worth") => n.worth.number().map_or_else(|| "null".to_owned(), |v| v.to_string())} [
            view! { Div "grouptop" [head] },
            mark("rule"),
            view! { Div "grouprows" => rows },
        ] }
    }

    /// Every category present, in the deployment's order first, then the rest alphabetically.
    fn facet(&self, of: impl Fn(&Entry) -> String, order: &[String]) -> Vec<String> {
        let mut present: Vec<String> = self
            .entries()
            .iter()
            .map(&of)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let rank = |v: &String| order.iter().position(|o| o == v).unwrap_or(order.len());
        present.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
        present
    }

    fn finder(&self) -> Node<Msg> {
        let toggle = |class: &str, label: &str, on: bool, m: Msg| {
            view! { Button {Type => "button", Class => if on { format!("chipbtn {class} on") } else { format!("chipbtn {class}") }, AriaPressed => if on { "true" } else { "false" }} @click(m) [label] }
        };
        let states = [(States::Open, "Open"), (States::Done, "Done"), (States::All, "All")].map(|(s, label)| {
            let on = self.filter.states == s;
            view! { Button {Type => "button", Class => if on { "segment on" } else { "segment" }, AriaPressed => if on { "true" } else { "false" }} @click(Msg::Narrow(Narrow::States(s))) [label] }
        });
        let chips = |values: Vec<String>,
                     any: &str,
                     none: &str,
                     now: &Choice,
                     make: &dyn Fn(Choice) -> Msg| {
            let chip = |c: Choice, label: String| {
                let on = *now == c;
                view! { Button {Type => "button", Class => if on { "chipbtn on" } else { "chipbtn" }, AriaPressed => if on { "true" } else { "false" }} @click(make(c)) [label] }
            };
            let mut row = vec![chip(Choice::Any, any.to_owned())];
            row.extend(values.into_iter().map(|v| {
                let label = if v.is_empty() {
                    none.to_owned()
                } else {
                    v.clone()
                };
                chip(Choice::Is(v), label)
            }));
            view! { Div "chiprow" [row] }
        };
        let categories = self.facet(
            |e| {
                e.authored
                    .as_ref()
                    .map(|a| a.category.clone())
                    .unwrap_or_default()
            },
            &self.context.categories,
        );
        let kinds = self.facet(
            |e| {
                e.authored
                    .as_ref()
                    .map(|a| kind(a).to_owned())
                    .unwrap_or_default()
            },
            &[],
        );
        let pills = GROUPINGS.map(|(g, by, label)| {
            view! { Button "pill" {Type => "button", Data("by") => by, AriaPressed => if self.by == g { "true" } else { "false" }} @click(Msg::By(g)) [label] }
        });
        view! { Div "finder" {Id => "finder", Hidden => self.find == Find::Shut} [
            view! { Div "filterbar" [
                view! { Div "searchbox" [view! { Input "search" {Type => "search", Placeholder => "Search todos", AriaLabel => "search todos by id or body", EnterKeyHint => "search", Data("field") => "search", Val => self.filter.query.clone()} @input(|q| Msg::Narrow(Narrow::Query(q))) }] },
                view! { Div "filterline" [
                    view! { Div "segmented" {Role => "group", AriaLabel => "state"} [states.to_vec()] },
                    toggle("attentionbtn", "Needs a look", self.filter.attention, Msg::Narrow(Narrow::Attention)),
                    toggle("waitingbtn", "Waiting", self.filter.waiting, Msg::Narrow(Narrow::Waiting)),
                    toggle("datedbtn", "Dated", self.filter.dated, Msg::Narrow(Narrow::Dated)),
                ] },
                view! { Div "facets" [
                    chips(categories, "All", "uncategorised", &self.filter.category, &|c| Msg::Narrow(Narrow::Category(c))),
                    (kinds.len() > 1).then(|| chips(kinds, "Any kind", "no kind", &self.filter.kind, &|c| Msg::Narrow(Narrow::Kind(c)))),
                ] },
            ] },
            view! { Div "groupby" {Role => "group", AriaLabel => "group by"} [view! { Span "groupbylabel" ["Group by"] }, pills.to_vec()] },
        ] }
    }

    fn head(&self) -> Vec<Node<Msg>> {
        let said = self.filter.narrowing().join(" · ");
        let title = if self.chain == PROJECT {
            "Project"
        } else {
            "Todos"
        };
        let attention = match &self.grown {
            Drawn::Tree(t, ..) => Some(
                t.nodes()
                    .into_iter()
                    .filter(|n| n.worth.band() == Band::Problem)
                    .filter_map(|n| todo_of(&n.place))
                    .collect::<BTreeSet<_>>()
                    .len(),
            ),
            Drawn::Unpriced => None,
        };
        let needs = attention.map(|n| {
            let m = self.waiting;
            vec![
                viewtree::node::text(if n == 0 {
                    "Nothing needs attention".to_owned()
                } else {
                    format!("{n} {} attention", if n == 1 { "needs" } else { "need" })
                }),
                mark("diamond"),
                view! { A "inboxlink" {Href => To::Inbox("").href()} [if m == 0 { "nothing waits on you".to_owned() } else { format!("{m} {} on you", if m == 1 { "waits" } else { "wait" }) }] },
            ]
        });
        vec![
            view! { Div "titlerow" [
                view! { H1 {Class => if self.chain == PROJECT { "pagetitle" } else { "pagetitle destination" }} [title] },
                view! { Button "findbtn" {Type => "button", AriaControls => "finder", AriaExpanded => if self.find == Find::Open { "true" } else { "false" }, AriaLabel => if said.is_empty() { "search, filter and group".to_owned() } else { format!("search, filter and group: {said}") }} @click(Msg::Find) [
                    mark("search"),
                    view! { Span "findsaid" [said.clone()] },
                ] },
            ] },
            view! { P "needs" [needs] },
            self.finder(),
        ]
    }

    fn sheet_view(&self) -> Vec<Node<Msg>> {
        let s = &self.sheet;
        let by = |a: About| {
            s.said
                .as_ref()
                .filter(|(b, _)| *b == a)
                .map(|(_, w)| w.as_str())
        };
        let subof = view! { P "subof" {Hidden => s.parent.is_none()} [s.parent.as_ref().map(|p| vec![
            viewtree::node::text("A subtodo of "),
            view! { Strong [p.title.as_str()] },
            view! { Button "subofclear" {Type => "button", AriaLabel => "add it as a todo of its own"} @click(Msg::Sheet(SheetMsg::Unparent)) ["×"] },
        ])] };
        let head = view! { Div "pickerhead draftform" [
            subof,
            s.body.view("sheetbody", "draftfield draftbody", "the todo's action line, in Typst markup", 2, |t| Msg::Sheet(SheetMsg::Body(t))),
            form::refusal(by(About::Body)),
            para("meta", "Typst markup: *bold*, _emphasis_, #link(\"https://…\")[text]. The preview is what the list shows."),
            form::chips("Pick a category", &self.context.categories, &s.category, |c| Msg::Sheet(SheetMsg::Category(c))),
            form::refusal(by(About::Category)),
            s.price.view(day_ms(&self.context.offsets, self.now)).map(|p| Msg::Sheet(SheetMsg::Price(p))),
            form::refusal(by(About::Price)),
            form::line("draftfield", "why this priority", "why this number (optional)", &s.rationale, |r| Msg::Sheet(SheetMsg::Rationale(r))),
            form::refusal(by(About::Whole)),
            view! { Button "addbtn" {Type => "button", Disabled => self.adding != Adding::Typing} @click(Msg::Sheet(SheetMsg::Add)) [mark("plus"), "Add todo"] },
        ] };
        vec![
            view! { Div {Class => if self.up { "picker addsheet open" } else { "picker addsheet" }, Id => "addsheet"} [
                view! { Button "pickerhandle" {Type => "button", AriaControls => "addsheet", AriaExpanded => if self.up { "true" } else { "false" }} @click(Msg::Sheet(SheetMsg::Handle)) [
                    view! { Span "addicon" [mark("plus")] },
                    view! { Strong ["Add a todo"] },
                    mark("chevron"),
                ] },
                view! { Div "pickerbody" [head] },
            ] },
            view! { Div "scrim" {Hidden => !self.up} @click(Msg::Sheet(SheetMsg::Down)) },
        ]
    }

    /// Why the list is empty has three answers, and they are different news.
    fn nothing(&self) -> &'static str {
        if self.entries().is_empty() {
            "This list has nothing on it yet. Add the first todo with the sheet at the bottom."
        } else if self.filter == Filter::default() {
            "Nothing is open. Everything on this list has been settled."
        } else {
            "Nothing matches. Tap the magnifier to widen the search."
        }
    }

    fn view(&self) -> Node<Msg> {
        let waiting = self.fold.is_none() || matches!(self.grown, Drawn::Unpriced);
        let status = match (&self.problem, waiting) {
            (Some(e), true) => chrome::Status::Failed {
                e: e.clone(),
                retry: Msg::Retry,
            },
            (_, true) => chrome::Status::Waiting,
            (_, false) => chrome::Status::Quiet,
        };
        let c = Chrome {
            class: "listpane todospane",
            width: Width::Wide,
            title: String::new(),
            masthead: Masthead::Page,
            back: None,
            legend: None,
            now: Vec::new(),
            shape: Shape::Rows,
        };
        let cards = self.cards();
        let tree = view! { Div "tree" {Data("at") => match &self.grown { Drawn::Tree(_, PricedBy::Read(at), _) => at.clone(), Drawn::Tree(_, PricedBy::Rows, _) | Drawn::Unpriced => String::new() }} => cards.iter().map(|c| (c.key.clone(), self.card(c))) };
        let body = vec![
            match &self.setup {
                Setup::Unfinished { done, of } => Some(view! { P "setupunfinished" [view! { A {Href => To::Settings.href()} [format!("Finish setting up · {done} of {of} done"), mark("chevron")] }] }),
                Setup::Unknown | Setup::Finished => None,
            },
            match &self.today {
                Today::Briefed(day) => Some(view! { P "todaybriefing" [view! { A {Href => To::Briefing(day).href()} ["Today's briefing", mark("chevron")] }] }),
                Today::Unknown | Today::None => None,
            },
            self.problem.as_ref().filter(|_| !waiting).map(|p| para("problem", p.as_str())),
            Some(tree),
            (!waiting && cards.is_empty()).then(|| empty(self.nothing())),
            Some(view! { Div "sheetspacer" }),
        ]
        .into_iter()
        .flatten()
        .chain(self.sheet_view())
        .collect();
        let head = view! { Div "listhead" [self.head()] };
        let column = chrome::page_headed(c, head, status, body);
        let pane = match &self.pane {
            Pane::Hint => view! { Div "panehint" [
                para("panehintlead", "Pick a todo."),
                para("legend", "Its detail, what it costs, why that number, and every event on the chain that made it."),
            ] },
            Pane::Todo(p) => p.view().map(Msg::Pane),
        };
        view! { Div "split todosplit" [column, view! { Div "detailpane" [pane] }] }
    }
}

impl Program for Todos {
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

    // What this device keeps is read, never trusted: the filter round-trips through its print,
    // anything else reads as the default, and a partial one keeps the fields it carries.
    #[test]
    fn the_kept_filter_is_parsed_rather_than_trusted() {
        let filter = Filter {
            states: States::All,
            category: Choice::Is("work".to_owned()),
            kind: Choice::Any,
            query: "deed".to_owned(),
            attention: true,
            waiting: true,
            dated: true,
        };
        assert_eq!(Filter::of(&filter.print()), filter);
        assert_eq!(Filter::of(&Filter::default().print()), Filter::default());
        for kept in [
            json!(null),
            json!(""),
            json!([]),
            json!("open"),
            json!(3),
            json!({"state": "sideways"}),
            json!({}),
        ] {
            assert_eq!(Filter::of(&kept), Filter::default(), "{kept}");
        }
        assert_eq!(
            Filter::of(&json!({"state": "all", "query": "work"})),
            Filter {
                states: States::All,
                query: "work".to_owned(),
                ..Filter::default()
            }
        );
    }

    // The host holds an ask as its JSON and answers with that JSON read back, so an ask that does
    // not read back as itself is an answer the page never hears.
    #[test]
    fn every_ask_reads_back_as_itself() {
        let write = todo::Write::Defer;
        for ask in [
            Ask::Tree { n: 3 },
            Ask::Reading {
                at: "2026-09-04T12:00:00".to_owned(),
            },
            Ask::Pane {
                ask: todo::Ask::Printed { write },
            },
            Ask::Pane {
                ask: todo::Ask::Sealed { write },
            },
            Ask::Pane {
                ask: todo::Ask::Thirty,
            },
        ] {
            let printed = serde_json::to_string(&ask).expect("an ask prints");
            let back: Value = serde_json::from_str(&printed).expect("as JSON");
            assert_eq!(
                serde_json::from_value::<Ask>(back).ok(),
                Some(ask),
                "{printed}"
            );
        }
    }

    // A tree asked, then asked again before its answer: the late answer to the first is not
    // drawn, and the second's is.
    #[test]
    fn an_answer_to_a_replaced_ask_draws_nothing() {
        let (mut m, _) = Model::open(Flags {
            context: Context {
                gate: Gate::Writes,
                reach: crate::runtime::Reach::Writes,
                offsets: Vec::new(),
                wide: false,
                categories: Vec::new(),
            },
            chain: "life".to_owned(),
            now: 0,
        });
        m.asked = 2;
        m.wants = Wants::Asking(2, m.question());
        let worth = |v: f64| Answer::Done {
            body: json!({"place": {"type": "All"}, "term": {"kind": "flat", "value": v}, "priced": {"type": "Value", "v": v}, "children": []}),
        };
        m.tree(1, worth(0.88));
        assert!(matches!(m.grown, Drawn::Unpriced));
        m.tree(2, worth(0.66));
        assert!(matches!(&m.grown, Drawn::Tree(t, ..) if t.node.worth == Worth::Value(0.66)));
    }

    #[test]
    fn each_refusal_of_the_add_sheet_names_the_field_it_is_about() {
        let about = |s: &Sheet| s.read().err().map(|(about, _)| about);
        let mut sheet = Sheet::blank();
        assert_eq!(about(&sheet), Some(About::Body), "no action line");
        sheet.body = Markup::of("Renew the library card".to_owned());
        assert_eq!(about(&sheet), Some(About::Category), "no category");
        sheet.category = "errands".to_owned();
        assert!(sheet.read().is_ok());
        sheet
            .price
            .update(PriceMsg::Kind(crate::form::Kind::Deadline));
        assert_eq!(about(&sheet), Some(About::Price), "a deadline with no date");
    }

    #[test]
    fn an_absent_term_waits_on_the_first_chain_not_held_through_its_own_references() {
        let functions: BTreeMap<String, Value> = [
            ("a".to_owned(), json!({"kind": "ref", "todo": "b"})),
            ("b".to_owned(), json!({"kind": "conj", "p": -4.0, "terms": [{"kind": "ref", "todo": "a"}, {"kind": "refIn", "store": "work", "todo": "x"}]})),
        ]
        .into();
        let held = |names: &[&str]| {
            names
                .iter()
                .map(|n| (*n).to_owned())
                .collect::<BTreeSet<_>>()
        };
        let a = json!({"kind": "ref", "todo": "a"});
        assert_eq!(
            unheld(&a, &functions, &held(&["life"]), &mut BTreeSet::new()),
            Some("work".to_owned())
        );
        assert_eq!(
            unheld(
                &a,
                &functions,
                &held(&["life", "work"]),
                &mut BTreeSet::new()
            ),
            None
        );
    }
}
