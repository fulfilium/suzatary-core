//! A proposal card, drawn the same in a conversation, the Inbox and Settings. Its text is set as
//! text, a yes is confirmed by a second press, and a tap carries an index, never a body.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use suzatary_core::reference::Reference;
use suzatary_core::wire::{
    Asker, Asks, CardAnswer, CardFile, CardLink, CodingFile, CodingSession, FileFrom, ProposalRow,
    ProposalState, ProposedAct, ProposedObject,
};
use ts_rs::TS;
use viewtree::Attr::{Autocomplete, Class, Data, Disabled, Href, Open, Placeholder, Title, Type};
use viewtree::kit::para;
use viewtree::{Node, view};

use crate::chrome::{Chips, link, prose};
use crate::digest;
use crate::intents::{Intents, Shown};
use crate::places::Place;
use crate::runtime::{
    Act, Answer, Answering, Asked, Local, Outcome, Reach, Register, Said, Secret, Untrusted,
    answered,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(rename = "CardKey")]
pub struct Key {
    pub conversation: String,
    pub index: u32,
}

impl Key {
    #[must_use]
    pub(crate) fn of(conversation: &str, row: &ProposalRow) -> Key {
        Key {
            conversation: conversation.to_owned(),
            index: u32::try_from(row.index).unwrap_or(u32::MAX),
        }
    }

    #[must_use]
    pub(crate) fn print(&self) -> String {
        format!("{}/{}", self.conversation, self.index)
    }

    /// `proposal:<conversation>/<index>`.
    #[must_use]
    pub(crate) fn reference(&self) -> String {
        format!("proposal:{}", self.print())
    }

    fn register(&self) -> Local {
        Local::Ui {
            key: format!("card/{}", self.print()),
        }
    }

    fn kept(&self) -> Register {
        Register::Ui {
            key: format!("card/{}", self.print()),
        }
    }

    fn field(&self, what: &str) -> String {
        format!("{what}/{}", self.print())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Step {
    #[default]
    Idle,
    Confirming {
        verb: String,
    },
    Saying {
        /// `dismiss` or `drop`.
        verb: String,
    },
}

/// Whether a value is typed is held for the page alone: the value never leaves its field but in
/// the tap.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ui {
    step: Step,
    text: String,
    details: bool,
    #[serde(skip)]
    typed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    Open(Step),
    Typed(String),
    Close,
    Details(bool),
    Yes(usize),
    Value(bool),
    Dismiss,
    Acknowledge,
    Drop,
    Reply,
    Setup,
}

/// A link's card follows a yes by sending the link again.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(rename = "CardThen")]
pub struct Then {
    pub done: String,
    pub yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Out {
    Watch(Key, Local),
    Keep(Register, serde_json::Value),
    Act(Key, Act, Secret, Then),
    /// Sign the registration of a key, then tap the card.
    Endorse(Key, String, String, Then, String, String),
    Reply(Key),
    Focus(String),
    Toast(String),
    Setup(String),
    Go(String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Cards {
    uis: BTreeMap<Key, Ui>,
    watched: BTreeSet<Key>,
    /// Taps whose answer was lost, by the card's signature when tapped.
    lost: BTreeMap<Key, String>,
}

/// What moves when the box records an answer to a proposal: its machine and the session it
/// started (`asks.signature`).
#[must_use]
pub(crate) fn signature(p: &ProposalRow) -> String {
    format!(
        "{}:{}:{}",
        word(&p.state),
        p.tries,
        p.coding.as_ref().map_or("", coding_state)
    )
}

fn word<T: Serialize>(t: &T) -> String {
    match serde_json::to_value(t) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(_) | Err(_) => String::new(),
    }
}

fn tag_of<T: Serialize>(t: &T, field: &str) -> String {
    serde_json::to_value(t)
        .ok()
        .and_then(|v| v.get(field).and_then(|k| k.as_str().map(str::to_owned)))
        .unwrap_or_default()
}

fn coding_state(c: &CodingSession) -> &'static str {
    match c {
        CodingSession::Fired { .. } => "fired",
        CodingSession::Reported { .. } => "reported",
        CodingSession::Merged { .. } => "merged",
        CodingSession::Dropped { .. } => "dropped",
    }
}

#[must_use]
pub const fn answerable(s: ProposalState) -> bool {
    matches!(
        s,
        ProposalState::Standing
            | ProposalState::Failed
            | ProposalState::Refused
            | ProposalState::Unfinished
    )
}

fn tried(n: u32) -> String {
    match n {
        0 => String::new(),
        1 => "tried once".to_owned(),
        n => format!("tried {n} times"),
    }
}

#[must_use]
pub(crate) fn said(p: &ProposalRow) -> String {
    let again = if p.tries > 1 {
        format!(" ({})", tried(p.tries))
    } else {
        String::new()
    };
    let o = &p.outcome;
    match p.state {
        ProposalState::Standing => String::new(),
        ProposalState::Sent => format!("Sent to {o}{again}"),
        ProposalState::Applied => format!("Applied: {o}{again}"),
        ProposalState::Started => format!("Running: {o}"),
        ProposalState::Acted => format!("Done: {o}{again}"),
        ProposalState::Failed => format!("{}, {}: {o}", p.card.failed, tried(p.tries)),
        ProposalState::Refused => format!(
            "{}, {}: {o}. Trying again would be refused the same way.",
            p.card.failed,
            tried(p.tries)
        ),
        ProposalState::Unfinished => format!("Unfinished, {}: {o}", tried(p.tries)),
        ProposalState::Unknown => {
            format!("Failed, {}: {o}. {}", tried(p.tries), p.card.uncertain)
        }
        ProposalState::Checked => format!(
            "Failed, {}: {o}. You checked it; it is never tried again.",
            tried(p.tries)
        ),
        ProposalState::Dismissed => format!("Dismissed: {o}"),
        // Housekeeping, never the owner's words: `outcome` is not read.
        ProposalState::Superseded => "Replaced by a later draft.".to_owned(),
        ProposalState::Withdrawn => format!("Withdrawn: {o}"),
        ProposalState::DoneElsewhere => format!("Done another way: {o}"),
        ProposalState::Unreadable => format!("Cannot be read: {o}"),
    }
}

fn asker_tag(a: &Asker) -> Option<(String, String)> {
    match a {
        Asker::Secretary {} => None,
        Asker::Triage {} => Some(("Triage".to_owned(), "The triage agent asks for this from mail anyone can send. Nothing changes until you answer.".to_owned())),
        Asker::Plugin { name } => Some((format!("{name} plugin"), format!("The {name} plugin asks for this. It only proposes; nothing happens until you answer."))),
        Asker::Setup {} => Some(("Setup".to_owned(), "The app asks for this itself, not the secretary: something it needs is not set up yet.".to_owned())),
        Asker::Box {} => Some(("App".to_owned(), "The app asks for this itself, not the secretary.".to_owned())),
    }
}

#[must_use]
pub(crate) fn asker_name(a: &Asker) -> String {
    match a {
        Asker::Secretary {} => "Secretary".to_owned(),
        Asker::Triage {} => "Triage".to_owned(),
        Asker::Plugin { name } => format!("{name} plugin"),
        Asker::Setup {} => "Setup".to_owned(),
        Asker::Box {} => "App".to_owned(),
    }
}

#[must_use]
pub(crate) const fn is_setup(a: &Asker) -> bool {
    match a {
        Asker::Setup {} => true,
        Asker::Secretary {} | Asker::Triage {} | Asker::Plugin { .. } | Asker::Box {} => false,
    }
}

pub(crate) type Titles = BTreeMap<String, String>;

#[must_use]
pub(crate) fn spoken(r: &Reference, titles: &Titles) -> String {
    match r {
        Reference::Todo(t) => titles
            .get(t.id.as_str())
            .cloned()
            .unwrap_or_else(|| "a todo".to_owned()),
        Reference::Mail { .. } => "a message".to_owned(),
        Reference::Thread { .. } => "a thread".to_owned(),
        Reference::Briefing { date } => format!("the briefing of {}", date.format("%Y-%m-%d")),
        Reference::Proposal(_) => "an earlier card".to_owned(),
        Reference::Batch { .. } => "a held batch".to_owned(),
        Reference::Target(suzatary_core::topic_target::Target::Round { .. }) => {
            "a round".to_owned()
        }
        Reference::Target(suzatary_core::topic_target::Target::Note { .. }) => "a note".to_owned(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Placed<'a> {
    Conversation,
    Inbox(&'a str),
    Setup,
    Settings,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Drawn<'a> {
    pub(crate) conversation: &'a str,
    pub(crate) row: &'a ProposalRow,
    pub(crate) placed: Placed<'a>,
    pub(crate) reach: &'a Reach,
    pub(crate) intents: &'a Intents,
    pub(crate) titles: &'a Titles,
    pub(crate) opened: bool,
}

impl Cards {
    fn ui(&self, key: &Key) -> Ui {
        self.uis.get(key).cloned().unwrap_or_default()
    }

    /// Each watched once; a tap whose answer was lost is said once its row moves.
    pub(crate) fn seen<'a>(
        &mut self,
        rows: impl IntoIterator<Item = (&'a str, &'a ProposalRow)>,
    ) -> Vec<Out> {
        let mut out = Vec::new();
        for (conversation, row) in rows {
            let key = Key::of(conversation, row);
            if self.watched.insert(key.clone()) {
                let r = key.register();
                out.push(Out::Watch(key, r));
            }
        }
        out
    }

    pub(crate) fn settled<'a>(
        &mut self,
        row: impl Fn(&Key) -> Option<&'a ProposalRow>,
    ) -> Vec<Out> {
        let moved: Vec<(Key, String)> = self
            .lost
            .iter()
            .filter_map(|(k, was)| {
                row(k)
                    .filter(|r| signature(r) != *was)
                    .map(|r| (k.clone(), said(r)))
            })
            .collect();
        moved
            .into_iter()
            .map(|(k, words)| {
                self.lost.remove(&k);
                Out::Toast(words)
            })
            .collect()
    }

    pub(crate) fn held(&mut self, key: Key, v: serde_json::Value) {
        let typed = self.uis.get(&key).is_some_and(|u| u.typed);
        let ui = serde_json::from_value::<Ui>(v).unwrap_or_default();
        self.uis.insert(key, Ui { typed, ..ui });
    }

    pub(crate) fn update(&mut self, key: &Key, row: &ProposalRow, ev: Ev) -> Vec<Out> {
        let was = self.ui(key);
        let rest = Ui {
            details: was.details,
            ..Ui::default()
        };
        let (ui, out) = match wants(key, row, &was, ev) {
            Wants::Ui(ui) => (ui, Vec::new()),
            Wants::Out(o) => (was.clone(), vec![o]),
            Wants::Answer(answer, done, secret) => {
                let yes = matches!(answer, Answering::Tap { .. });
                let at = Asked {
                    conversation: key.conversation.clone(),
                    index: key.index,
                    from: signature(row),
                };
                let then = Then { done, yes };
                (
                    rest,
                    vec![Out::Act(
                        key.clone(),
                        Act::Answer { at, answer },
                        secret,
                        then,
                    )],
                )
            }
            Wants::Endorse(o) => (rest, vec![o]),
        };
        let mut out = out;
        if was.step != ui.step && matches!(ui.step, Step::Saying { .. }) {
            out.push(Out::Focus(key.field("reason")));
        }
        if (Ui {
            typed: false,
            ..ui.clone()
        }) != (Ui {
            typed: false,
            ..was
        }) {
            out.push(Out::Keep(
                key.kept(),
                serde_json::to_value(&ui).unwrap_or(serde_json::Value::Null),
            ));
        }
        self.uis.insert(key.clone(), ui);
        out
    }

    #[must_use]
    pub(crate) fn endorsed(
        key: &Key,
        signed: String,
        note: String,
        then: Then,
        a: Answer,
    ) -> Vec<Out> {
        match answered::<crate::runtime::Endorsement>(a) {
            Ok(endorsement) => vec![Out::Act(
                key.clone(),
                Act::Answer {
                    at: Asked {
                        conversation: key.conversation.clone(),
                        index: key.index,
                        from: signed,
                    },
                    answer: Answering::Tap {
                        note,
                        always: false,
                        endorsement: Some(endorsement),
                    },
                },
                Secret::None,
                then,
            )],
            Err(e) => vec![Out::Toast(e)],
        }
    }

    pub(crate) fn answered(&mut self, key: &Key, signed: &str, then: &Then, a: Answer) -> Vec<Out> {
        match answered::<Outcome>(a) {
            Ok(Outcome::Answered {
                answer: Said::Taken,
                ..
            }) => vec![Out::Toast(then.done.clone())],
            Ok(Outcome::Answered {
                answer: Said::Lost { .. },
                held: true,
            }) => {
                self.lost.insert(key.clone(), signed.to_owned());
                Vec::new()
            }
            Ok(
                Outcome::Already
                | Outcome::Answered {
                    answer: Said::Lost { .. } | Said::Refused { .. },
                    ..
                },
            ) => Vec::new(),
            Err(e) => vec![Out::Toast(e)],
        }
    }

    /// `within` stands inside it, after its own parts.
    #[must_use]
    pub(crate) fn view<M: 'static>(
        &self,
        d: Drawn<'_>,
        before: Vec<Node<M>>,
        within: Vec<Node<M>>,
        lift: fn(Key, Ev) -> M,
    ) -> Node<M> {
        let key = Key::of(d.conversation, d.row);
        let (attrs, parts) = view(d, &self.ui(&key), &key);
        let mut el = viewtree::node::el(viewtree::Tag::Div);
        for (a, v) in attrs {
            el = el.set(a, v);
        }
        let mapped: Vec<Node<M>> = parts
            .into_iter()
            .map(|n| {
                let k = key.clone();
                n.map(move |ev| lift(k.clone(), ev))
            })
            .collect();
        el.child(before).child(mapped).child(within).node()
    }
}

enum Wants {
    Ui(Ui),
    Out(Out),
    Answer(Answering, String, Secret),
    Endorse(Out),
}

fn wants(key: &Key, row: &ProposalRow, ui: &Ui, ev: Ev) -> Wants {
    let performing: &[CardAnswer] = if row.card.problem.is_empty() {
        &row.card.answers
    } else {
        &[]
    };
    let note = || ui.text.trim().to_owned();
    let reason = |answer: fn(String) -> Answering, done: &str| {
        let r = note();
        if r.is_empty() {
            Wants::Out(Out::Focus(key.field("reason")))
        } else {
            Wants::Answer(answer(r), done.to_owned(), Secret::None)
        }
    };
    match ev {
        Ev::Open(step) => Wants::Ui(Ui {
            details: ui.details || matches!(step, Step::Confirming { .. }),
            step,
            text: String::new(),
            typed: ui.typed,
        }),
        Ev::Typed(text) => Wants::Ui(Ui { text, ..ui.clone() }),
        Ev::Close => Wants::Ui(Ui {
            details: ui.details,
            typed: ui.typed,
            ..Ui::default()
        }),
        Ev::Details(details) => Wants::Ui(Ui {
            details,
            ..ui.clone()
        }),
        Ev::Value(typed) => Wants::Ui(Ui {
            typed,
            ..ui.clone()
        }),
        Ev::Yes(n) => match performing.get(n) {
            Some(a) => yes(key, row, a, ui),
            None => Wants::Ui(ui.clone()),
        },
        Ev::Dismiss => reason(|reason| Answering::Dismiss { reason }, "dismissed"),
        Ev::Drop => reason(|reason| Answering::Drop { reason }, "Dropped"),
        Ev::Acknowledge => {
            Wants::Answer(Answering::Acknowledge, row.card.check.clone(), Secret::None)
        }
        Ev::Reply => Wants::Out(Out::Reply(key.clone())),
        Ev::Setup => match &row.card.link {
            Some(CardLink::Setup { template, .. }) => Wants::Out(Out::Setup(template.clone())),
            Some(CardLink::Page { .. }) | None => Wants::Ui(ui.clone()),
        },
    }
}

fn yes(key: &Key, row: &ProposalRow, a: &CardAnswer, ui: &Ui) -> Wants {
    let done = a.done.clone();
    let note = ui.text.trim().to_owned();
    match &a.asks {
        Asks::Now => Wants::Answer(tap(String::new(), false), done, Secret::None),
        Asks::Value { .. } => Wants::Answer(
            tap(String::new(), false),
            done,
            Secret::Field {
                field: key.field("secret"),
                keep: key.print(),
            },
        ),
        Asks::Confirm { .. } => Wants::Answer(tap(note, false), done, Secret::None),
        Asks::Always { .. } => Wants::Answer(tap(note, true), done, Secret::None),
        Asks::Endorse { .. } => match &row.act {
            ProposedAct::Device { actor, key: device, .. } => Wants::Endorse(Out::Endorse(
                key.clone(),
                actor.clone(),
                device.clone(),
                Then { done, yes: true },
                note,
                signature(row),
            )),
            ProposedAct::Email { .. }
            | ProposedAct::Forward { .. }
            | ProposedAct::Changes { .. }
            | ProposedAct::Auth { .. }
            | ProposedAct::AuthSend { .. }
            | ProposedAct::Secret { .. }
            | ProposedAct::Web { .. }
            | ProposedAct::Patch { .. }
            | ProposedAct::Drive { .. }
            | ProposedAct::CodingSession { .. }
            | ProposedAct::Afresh { .. }
            | ProposedAct::Noise { .. }
            | ProposedAct::Restore { .. }
            | ProposedAct::SignIn { .. }
            | ProposedAct::UnlockStore { .. }
            | ProposedAct::Erase { .. }
            | ProposedAct::Retract { .. }
            | ProposedAct::SignInCode { .. }
            | ProposedAct::GoogleClient { .. }
            | ProposedAct::GoogleSignIn { .. }
            | ProposedAct::CalendarEvent { .. }
            | ProposedAct::Connect { .. }
            | ProposedAct::Disconnect { .. }
            | ProposedAct::Grant { .. }
            | ProposedAct::Remember { .. }
            | ProposedAct::Consolidate { .. }
            // Only a device's registration is signed: the box asks no other card to be.
            | ProposedAct::Decision { .. } => Wants::Ui(ui.clone()),
        },
    }
}

const fn tap(note: String, always: bool) -> Answering {
    Answering::Tap {
        note,
        always,
        endorsement: None,
    }
}

fn asked_line<M>(shown: &Shown, lost_words: &str) -> Option<Node<M>> {
    match shown {
        Shown::Absent => None,
        Shown::Pending | Shown::Taken => Some(crate::chrome::inline_wait()),
        Shown::Failed { e, lost: true } => {
            Some(view! { Span "problem" [format!("No answer ({e}){lost_words}")] })
        }
        Shown::Failed { e, lost: false } => Some(view! { Span "problem" [e.as_str()] }),
    }
}

fn button<M>(class: &str, label: &str, m: M) -> Node<M> {
    view! { Button {Type => "button", Class => class.to_owned()} @click(m) [label] }
}

fn changed(o: &ProposedObject) -> Option<Node<Ev>> {
    let changes = o.act.changes();
    (!changes.is_empty()).then(|| {
        let dash = |s: &str| {
            if s.is_empty() {
                "—".to_owned()
            } else {
                s.to_owned()
            }
        };
        let rows: Vec<Node<Ev>> = changes
            .iter()
            .flat_map(|c| {
                [
                    view! { Dt [c.field.as_str()] },
                    view! { Dd [
                        view! { Span "was" [dash(&c.before)] },
                        view! { Span "arrow" ["→"] },
                        view! { Span "now" [dash(&c.after)] },
                    ] },
                ]
            })
            .collect();
        view! { Dl "proposedchanges" [rows] }
    })
}

/// `new` marks a row the card it replaces did not ask for.
fn object_row(o: &ProposedObject, new: bool) -> Node<Ev> {
    let t = o.act.todo();
    let note = o.act.note().filter(|n| !n.is_empty());
    let class = format!(
        "proposedobject act-{}{}",
        o.act.word(),
        if new { " proposednew" } else { "" }
    );
    view! { Div {Class => class, Data("object") => o.position.to_string()} [
        view! { P "proposedhead" [
            view! { Span "proposedact" [digest::sentence(o)] },
            new.then(|| view! { Span "proposedfresh" ["new"] }),
        ] },
        (!t.body.is_empty()).then(|| para("proposedbody", t.body.as_str())),
        note.map(|n| para("proposednote", n)),
        changed(o),
        t.pending_todo.then(|| para("proposedpending", "This todo is not on the chain yet: another card adds it, and this one records an outcome for a todo with no content until that one is tapped.")),
        link("proposedlink", Place::Todo(&t.todo), "open the todo"),
    ] }
}

fn file_row(f: &CardFile) -> Node<Ev> {
    let opening = match &f.from {
        FileFrom::Repo { repo, path, at } => {
            view! { Span "proposalfilepath" [format!("{repo}: {path} at {}", at.get(..12).unwrap_or(at))] }
        }
        FileFrom::Mail { message } => link(
            "proposalfileopen",
            Place::Message(message),
            "open the message",
        ),
        FileFrom::Upload { id } => {
            view! { A "proposalfileopen" {Href => Place::Attachment(id).href()} ["open the file"] }
        }
        FileFrom::Kept {} => view! { Span "proposalfilepath" ["the copy you allowed once"] },
        FileFrom::Drive { path } => view! { Span "proposalfilepath" [path.as_str()] },
    };
    view! { Li "proposalfile" {Data("file") => &f.name} [
        view! { Span "proposalfilename" [f.name.as_str()] },
        view! { Span "proposalfilemeta" [format!("{} · {} · sha256 {}", f.media, f.size, f.sha256.get(..12).unwrap_or(&f.sha256))] },
        opening,
    ] }
}

fn source(text: &str, titles: &Titles) -> Node<Ev> {
    match text.parse::<Reference>() {
        Ok(r) => {
            view! { A {Class => format!("reflink proposalsource ref-{}", r.scheme()), Href => Place::Reference(&r).href()} [spoken(&r, titles)] }
        }
        Err(_) => viewtree::node::text(text),
    }
}

fn coding(s: &CodingSession, shown: &Shown, ui: &Ui, key: &Key, writes: bool) -> Node<Ev> {
    let (state, id, head): (&str, &str, String) = match s {
        CodingSession::Fired { id, at, .. } => ("fired", id, format!("Running since {at}")),
        CodingSession::Reported { id, branch, .. } => {
            ("reported", id, format!("Reported on {branch}"))
        }
        CodingSession::Merged { id, sha } => (
            "merged",
            id,
            format!("Merged at {}", sha.get(..12).unwrap_or(sha)),
        ),
        CodingSession::Dropped { id, reason } => ("dropped", id, format!("Dropped: {reason}")),
    };
    let lines = |f: &CodingFile| match (f.added, f.removed) {
        (Some(a), Some(r)) => format!("+{a} −{r}"),
        (None, _) | (_, None) => "binary".to_owned(),
    };
    let body: Vec<Node<Ev>> = match s {
        CodingSession::Fired { url, .. } => {
            vec![view! { P [view! { A "codingurl" {Href => url} [url.as_str()] }] }]
        }
        CodingSession::Reported {
            path, report, stat, ..
        } => {
            let (files, added, removed) = stat.iter().fold((0, 0, 0), |(n, a, r), f| {
                (n + 1, a + f.added.unwrap_or(0), r + f.removed.unwrap_or(0))
            });
            vec![
                view! { P "codingpath" [view! { Code [path.as_str()] }] },
                view! { Div "codingstat" [
                    para("codingtotal", format!("{files} file{}, +{added} −{removed}", if files == 1 { "" } else { "s" })),
                    view! { Ul "codingfiles" [stat.iter().map(|f| view! { Li [view! { Code [f.path.as_str()] }, " ", view! { Span "codinglines" [lines(f)] }] }).collect::<Vec<_>>()] },
                ] },
                view! { Pre "proposalbody codingreport" [report.as_str()] },
            ]
        }
        CodingSession::Merged { .. } | CodingSession::Dropped { .. } => Vec::new(),
    };
    let droppable = matches!(
        s,
        CodingSession::Fired { .. } | CodingSession::Reported { .. }
    );
    let idle = || {
        button(
            "act act-drop",
            "Dismiss",
            Ev::Open(Step::Saying {
                verb: "drop".to_owned(),
            }),
        )
    };
    let controls: Option<Vec<Node<Ev>>> = (writes && droppable).then(|| match shown {
        Shown::Pending | Shown::Taken => vec![crate::chrome::inline_wait()],
        Shown::Failed { e, lost } => vec![
            view! { Span "problem" [if *lost { format!("No answer ({e}); asking again never does it twice.") } else { e.clone() }] },
            idle(),
        ],
        Shown::Absent if ui.step == (Step::Saying { verb: "drop".to_owned() }) => vec![
            reason(key, ui, "Why not? (done by hand, wrong task…)"),
            button("act act-drop confirming", "Dismiss", Ev::Drop),
            button("act act-keep", "Keep", Ev::Close),
        ],
        Shown::Absent => vec![idle()],
    });
    view! { Section {Class => format!("codingsession codingsession-{state}"), Data("session") => id} [
        para("codinghead", head),
        body,
        controls.map(|c| view! { Div "proposalact-row" [c] }),
    ] }
}

fn reason(key: &Key, ui: &Ui, placeholder: &str) -> Node<Ev> {
    view! { Input "dismissreason" {Type => "text", Placeholder => placeholder, viewtree::Attr::Value => &ui.text, Data("field") => key.field("reason")} @input(Ev::Typed) }
}

fn yes_of<'a>(row: &'a ProposalRow, ui: &Ui) -> Option<(usize, &'a CardAnswer)> {
    let Step::Confirming { verb } = &ui.step else {
        return None;
    };
    row.card
        .answers
        .iter()
        .enumerate()
        .find(|(_, a)| &a.verb == verb && !matches!(a.asks, Asks::Now | Asks::Value { .. }))
}

fn control(n: usize, a: &CardAnswer, key: &Key, ui: &Ui) -> Node<Ev> {
    let class = format!("act act-{}", a.verb);
    match &a.asks {
        Asks::Value { input } => view! { Div "proposalsecretfield" [
            view! { Label "secretlabel" [
                view! { Span [input.as_str()] },
                view! { Input "secretvalue" {Type => "password", Autocomplete => "off", Placeholder => "Paste it here", Data("field") => key.field("secret")} @input(|w: String| Ev::Value(!w.is_empty())) },
            ] },
            view! { Button {Type => "button", Class => class, Disabled => !ui.typed} @click(Ev::Yes(n)) [a.label.as_str()] },
        ] },
        Asks::Now => {
            view! { Button {Type => "button", Class => class} @click(Ev::Yes(n)) [a.label.as_str()] }
        }
        Asks::Confirm { .. } | Asks::Always { .. } | Asks::Endorse { .. } => {
            view! { Button {Type => "button", Class => class} @click(Ev::Open(Step::Confirming { verb: a.verb.clone() })) [a.label.as_str()] }
        }
    }
}

fn readonly(d: Drawn<'_>) -> Node<Ev> {
    let readonly = match d.reach {
        Reach::Reads {
            key: Untrusted::Untrusted,
            ..
        } => Some(("This device is not trusted yet.", "Trust it →")),
        Reach::Reads {
            key: Untrusted::Revoked,
            ..
        } => Some((
            "This device's key was revoked.",
            "Put a new key in its place →",
        )),
        Reach::Reads {
            key: Untrusted::NotTheKey,
            ..
        }
        | Reach::Writes
        | Reach::Waits => None,
    };
    match readonly {
        Some((words, go)) => {
            view! { Div "proposaloutcome readonly" [words, " ", link("act act-trustit", Place::Settings, go)] }
        }
        None => view! { Div "proposaloutcome readonly" [d.row.card.readonly.as_str()] },
    }
}

fn idle(
    performing: &[CardAnswer],
    key: &Key,
    ui: &Ui,
    shown: &Shown,
    reply: Option<&Node<Ev>>,
) -> Vec<Node<Ev>> {
    let mut v: Vec<Node<Ev>> = asked_line(
        shown,
        ", and it was not done. Trying again never does it twice.",
    )
    .into_iter()
    .collect();
    v.extend(
        performing
            .iter()
            .enumerate()
            .map(|(n, a)| control(n, a, key, ui)),
    );
    v.push(button(
        "act act-dismiss",
        "Dismiss",
        Ev::Open(Step::Saying {
            verb: "dismiss".to_owned(),
        }),
    ));
    v.extend(reply.cloned());
    v
}

fn acts(
    d: Drawn<'_>,
    ui: &Ui,
    key: &Key,
    shown: &Shown,
    reply: Option<&Node<Ev>>,
) -> Option<Node<Ev>> {
    let row = d.row;
    let card = &row.card;
    let writes = matches!(d.reach, Reach::Writes);
    if !answerable(row.state) {
        let checking = (row.state == ProposalState::Unknown && writes).then(|| {
            let line = asked_line(
                shown,
                ", and it was not done. Trying again never does it twice.",
            );
            if shown.waits() {
                vec![line]
            } else {
                vec![
                    line,
                    Some(button("act act-acknowledge", &card.check, Ev::Acknowledge)),
                ]
            }
        });
        (checking.is_some() || reply.is_some())
            .then(|| view! { Div "proposalact-row" [checking, reply.cloned()] })
    } else if !writes {
        Some(readonly(d))
    } else if shown.waits() {
        Some(view! { Div "proposalact-row" [asked_line(shown, ""), reply.cloned()] })
    } else {
        let performing: &[CardAnswer] = if card.problem.is_empty() {
            &card.answers
        } else {
            &[]
        };
        let parts: Vec<Node<Ev>> = match (
            yes_of(row, ui).filter(|_| card.problem.is_empty()),
            &ui.step,
        ) {
            (Some((n, a)), _) => {
                let (question, yes) = match &a.asks {
                    Asks::Confirm { question, yes }
                    | Asks::Always { question, yes }
                    | Asks::Endorse { question, yes } => (question.as_str(), yes.as_str()),
                    Asks::Now | Asks::Value { .. } => ("", ""),
                };
                vec![
                    view! { Span "proposalconfirm" [question] },
                    button(&format!("act act-{} confirming", a.verb), yes, Ev::Yes(n)),
                    button("act act-cancelsend", "No", Ev::Close),
                    view! { Input "noteline" {Type => "text", Placeholder => "Why, or what next", viewtree::Attr::AriaLabel => "Why, or what next", viewtree::Attr::Value => &ui.text, Data("field") => key.field("note")} @input(Ev::Typed) },
                ]
            }
            (None, Step::Saying { verb }) if verb == "dismiss" => vec![
                reason(key, ui, "Why not? (wrong person, already sent, not yet…)"),
                button("act act-dismiss confirming", "Dismiss", Ev::Dismiss),
                button("act act-canceldismiss", "Keep", Ev::Close),
            ],
            (None, Step::Idle | Step::Confirming { .. } | Step::Saying { .. }) => {
                idle(performing, key, ui, shown, reply)
            }
        };
        Some(view! { Div "proposalact-row" [parts] })
    }
}

fn details(d: Drawn<'_>, ui: &Ui, carried: usize) -> Node<Ev> {
    let row = d.row;
    let card = &row.card;
    view! { Details "proposaldetails" {Open => ui.details} @toggle(Ev::Details) [
        view! { Summary ["Details"] },
        view! { Div "proposalsources" [
            view! { Span "proposallabel" ["Because of"] },
            row.sources.iter().map(|s| source(&s.to_string(), d.titles)).collect::<Vec<_>>(),
        ] },
        card.fields.iter().map(|f| view! { Div "proposalfield" [
            view! { Span "proposallabel" [f.label.as_str()] },
            view! { Span "proposalvalue" [f.value.as_str()] },
        ] }).collect::<Vec<_>>(),
        (!card.body.is_empty()).then(|| prose(crate::part::Tag::Pre, "proposalbody", &card.body, None, Chips::Conversation(d.conversation))),
        (!card.files.is_empty()).then(|| view! { Ul "proposalfiles" [card.files.iter().map(file_row).collect::<Vec<_>>()] }),
        (!card.rows.is_empty()).then(|| view! { Div "proposedchangeset" [card.rows.iter().map(|o| object_row(o, row.revises.is_some() && o.position > carried)).collect::<Vec<_>>()] }),
    ] }
}

fn view(d: Drawn<'_>, ui: &Ui, key: &Key) -> (Vec<(viewtree::Attr, String)>, Vec<Node<Ev>>) {
    let row = d.row;
    let card = &row.card;
    let signed = signature(row);
    let shown = d.intents.about(
        |a| matches!(a, Act::Answer { at, .. } if at.conversation == d.conversation && at.index == key.index),
        |a| matches!(a, Act::Answer { at, .. } if at.from != signed),
    );
    let writes = matches!(d.reach, Reach::Writes);
    let tag = match d.placed {
        Placed::Setup => None,
        Placed::Conversation | Placed::Inbox(_) | Placed::Settings => asker_tag(&card.by),
    };
    let context = match d.placed {
        Placed::Inbox(title) => Some(link(
            "proposalfrom",
            Place::conversation(d.conversation),
            title,
        )),
        Placed::Conversation | Placed::Setup | Placed::Settings => None,
    };
    let reply = match d.placed {
        Placed::Conversation | Placed::Inbox(_) | Placed::Setup => {
            Some(button("act act-replyto", "Reply", Ev::Reply))
        }
        Placed::Settings => None,
    };
    let carried = row.revises.map_or(0, |r| r.carried);
    let revises = (carried > 0 && carried < card.rows.len()).then(|| para("proposalrevises", format!("Replaces an earlier card: the first {carried} of these were asked before, the rows marked new are not.")));
    let overview = (!card.rows.is_empty()).then(|| {
        let (line, flags) = digest::lines(&card.rows);
        view! { Div "diffoverview" [
            para("diffline", line),
            flags.into_iter().map(|f| para("diffflag", f)).collect::<Vec<_>>(),
        ] }
    });
    let details = details(d, ui, carried);
    let outcome =
        (row.state != ProposalState::Standing).then(|| para("proposaloutcome", said(row)));
    let acts = acts(d, ui, key, &shown, reply.as_ref());
    let session = (!answerable(row.state))
        .then(|| {
            row.coding
                .as_ref()
                .map(|s| coding(s, &shown, ui, key, writes))
        })
        .flatten();
    let class = format!(
        "proposal proposal-{} proposal-act-{}",
        word(&row.state),
        tag_of(&row.act, "kind")
    );
    let class = if d.opened {
        format!("{class} opened")
    } else {
        class
    };
    let attrs = vec![
        (Class, class),
        (Data("field"), key.field("card")),
        (Data("proposal"), row.index.to_string()),
        (Data("conversation"), d.conversation.to_owned()),
        (Data("key"), key.print()),
    ];
    let parts: Vec<Option<Node<Ev>>> = vec![
        Some(view! { Div "proposalhead" [
            view! { Span "proposalact" [card.title.as_str()] },
            tag.map(|(text, tip)| view! { Span "proposaltag" {Title => tip} [text] }),
            context,
        ] }),
        Some(para("proposalwhy", row.why.as_str())),
        revises,
        row.announced
            .as_deref()
            .map(|a| prose(crate::part::Tag::P, "proposalannounced", a, Some(crate::part::Who::Box), Chips::None)),
        (!card.sentence.is_empty()).then(|| para("proposalbody", card.sentence.as_str())),
        overview,
        card.link.as_ref().and_then(|l| match l {
            CardLink::Page { label, href } => {
                Some(view! { A "act act-help proposallink" {Href => href} [label.as_str()] })
            }
            CardLink::Setup { label, .. } => writes.then(|| {
                view! { Button "act act-setup proposallink" {Type => "button"} @click(Ev::Setup) [label.as_str()] }
            }),
        }),
        (!card.problem.is_empty()).then(|| {
            para(
                "problem proposalproblem",
                format!("This changeset no longer applies: {}", card.problem),
            )
        }),
        Some(details),
        outcome,
        session,
        acts,
    ];
    (attrs, parts.into_iter().flatten().collect())
}

#[derive(Debug, Deserialize)]
struct Begun {
    id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(rename = "CardAsk")]
pub enum Asking {
    Act {
        key: Key,
        then: Then,
        signed: String,
    },
    /// `signed`: the row's signature at the tap.
    Endorse {
        key: Key,
        then: Then,
        note: String,
        signed: String,
    },
    Setup,
}

pub(crate) fn effects<H, A, S>(
    outs: Vec<Out>,
    hold: impl Fn(Key) -> H,
    ask: impl Fn(Asking) -> A,
) -> (Vec<crate::runtime::Effect<H, A, S>>, Vec<Key>) {
    use crate::runtime::Effect;
    let mut effects = Vec::new();
    let mut replies = Vec::new();
    for o in outs {
        match o {
            Out::Watch(key, local) => effects.push(Effect::Watch {
                ask: hold(key),
                local,
            }),
            Out::Keep(register, value) => effects.push(Effect::Keep { register, value }),
            Out::Act(key, act, secret, then) => {
                let signed = match &act {
                    Act::Answer { at, .. } => at.from.clone(),
                    Act::Question { .. }
                    | Act::Round { .. }
                    | Act::Rename { .. }
                    | Act::Forget { .. }
                    | Act::Erase { .. }
                    | Act::Topic { .. }
                    | Act::Seal { .. }
                    | Act::Lifecycle { .. } => String::new(),
                };
                effects.push(Effect::Act {
                    ask: ask(Asking::Act { key, then, signed }),
                    act,
                    secret,
                    told: crate::runtime::Told::Held,
                });
            }
            Out::Endorse(key, actor, device, then, note, signed) => effects.push(Effect::Endorse {
                ask: ask(Asking::Endorse {
                    key,
                    then,
                    note,
                    signed,
                }),
                actor,
                key: device,
            }),
            Out::Reply(key) => replies.push(key),
            Out::Focus(field) => effects.push(Effect::Focus { field }),
            Out::Toast(words) => effects.push(Effect::Toast { words }),
            Out::Setup(template) => effects.push(Effect::Post {
                ask: ask(Asking::Setup),
                path: format!(
                    "/api/secretary/setup/{}",
                    crate::runtime::component(&template)
                ),
                body: crate::runtime::Body::Empty {},
            }),
            Out::Go(href) => effects.push(Effect::Go { href }),
        }
    }
    (effects, replies)
}

impl Cards {
    /// A lost answer whose row already moved is said at once.
    pub(crate) fn asked<'a>(
        &mut self,
        asking: Asking,
        a: Answer,
        row: impl Fn(&Key) -> Option<&'a ProposalRow>,
    ) -> Vec<Out> {
        match asking {
            Asking::Act { key, then, signed } => {
                let mut out = self.answered(&key, &signed, &then, a);
                out.extend(self.settled(row));
                out
            }
            Asking::Endorse {
                key,
                then,
                note,
                signed,
            } => Cards::endorsed(&key, signed, note, then, a),
            Asking::Setup => vec![match answered::<Begun>(a) {
                Ok(Begun { id }) => Out::Go(Place::conversation(&id).href()),
                Err(words) => Out::Toast(words),
            }],
        }
    }

    /// Taps the link's card again with the link it was last tapped with.
    #[must_use]
    pub(crate) fn resend(link: &Key, row: &ProposalRow) -> Out {
        let done = row
            .card
            .answers
            .iter()
            .find(|a| matches!(a.asks, Asks::Value { .. }))
            .map_or_else(String::new, |a| a.done.clone());
        Out::Act(
            link.clone(),
            Act::Answer {
                at: Asked {
                    conversation: link.conversation.clone(),
                    index: link.index,
                    from: signature(row),
                },
                answer: tap(String::new(), false),
            },
            Secret::Kept { kept: link.print() },
            Then { done, yes: false },
        )
    }
}

#[must_use]
pub(crate) fn taken(a: &Answer) -> bool {
    matches!(
        answered::<Outcome>(a.clone()),
        Ok(Outcome::Answered {
            answer: Said::Taken,
            ..
        })
    )
}
