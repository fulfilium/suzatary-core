//! `suzatary prodrome carry`: the open todos of the upkeep chain that the carried list carries,
//! and its release todos, written as new todos on a store whose charter is public, each source
//! then cancelled naming its successor. The link runs one way: a public todo names nothing
//! private.
//!
//! The dry run and `--apply` both read [`Carry::of`], and door 1 judges the whole carry in one
//! batch, so the dry run's verdicts are what `--apply` does. Every carried event keeps the instant
//! of the source event it restates, so the event the dry run judges is the event `--apply`
//! writes. The destination is written first and the source second: a run after a crash finds
//! the carried todos held and writes the cancellations, and a run after a success finds every
//! carried todo cancelled naming its successor and writes nothing. "Held" is equality of events,
//! so a row whose todo the destination holds in other words than the row's (a list edited after a
//! crash) is refused rather than written beside them. A closed todo that a carried price names
//! (an open release's shipped or dropped item) is carried as it closed, so the release prices and
//! reads its items the same on the destination.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use prodrome_core::event::{Actor, Lifecycle, TodoId};
use prodrome_core::fpl::Outcome;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::{Policy as _, Untrusted};
use prodrome_core::term::{Term, TermF};
use suzatary_base::capability::{Clock as _, Typeset};
use suzatary_base::chains::ChainName;
use suzatary_base::reference::TodoRef;
use suzatary_core::release::{Counted, Plan};
use suzatary_record::record::{Authored, Node, Record, TodoEvent};
use suzatary_signatures::believed::Believed;
use suzatary_store::store::{Prodrome as _, Reads as _, Shut};
use suzatary_todo::render::Remembered;

use super::list::{CarriedList, Decision, Field, Restated};
use crate::commands::door::{Door, Lists, Opened, published};
use crate::instance::charter::Charter;
use crate::instance::{ChainDir, Instance};

/// One row for each open todo of the source and each completed release, judged by door 1.
pub(crate) struct Carry {
    rows: Vec<Row<Verdict>>,
    /// The version each release todo ships as, for the dry run's text.
    releases: BTreeMap<TodoId, Counted>,
}

/// Door 1's verdict on one event.
type Verdict = Result<(), Shut>;

/// One todo, and what the carry makes of it; `V` is what each new event carries, `()` before
/// door 1 judges it and its [`Verdict`] after.
struct Row<V> {
    id: TodoId,
    fate: Fate<V>,
}

enum Fate<V> {
    /// Left on the source.
    Stays { reason: String },
    /// Carried: what the destination gains, and what becomes of the source.
    Carried(Carried<V>),
    /// Carried by an earlier run: cancelled on the source, naming `to`.
    Done { to: TodoId },
    /// Cannot be carried as the source holds it.
    Unbuilt {
        to: TodoId,
        why: Why,
        refused: String,
    },
}

/// Why a todo is carried.
enum Why {
    /// The list says so, with its reason and what it restates.
    Listed(Restated),
    /// A closed todo carried as its history, with what the list restates of it.
    Closed(Closed, Restated),
}

/// Which closed todo is carried as its history: its record and its first trusted closing, so that
/// what reads its lot or its price reads the same on the destination.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Closed {
    /// A completed release no carried price names, with no price and so no rationale: its price
    /// names items that stay behind, and the version reads only that it completed.
    Shipped,
    /// A completed or cancelled todo that a carried todo's price names, a completed release
    /// among them: an open release's shipped or dropped item, with its own price renamed. A reference to it prices 1 from its
    /// closing on, as on the source, and the release's items read its lot. The core gives a todo
    /// whose first record has no price no function at all, so a reference to one carried with
    /// none would drop out of the release's price.
    Named,
}

struct Carried<V> {
    to: TodoId,
    why: Why,
    landing: Landing<V>,
    leaving: Leaving,
}

/// What the destination gains, by what it holds of the successor: "held" is equality of events.
enum Landing<V> {
    /// These events, in order: it holds none of the row's events, or a prefix of them and nothing
    /// else of the todo.
    New(Vec<(TodoEvent, V)>),
    /// Nothing: it holds every event of the row already, and nothing else of the todo.
    Held,
    /// Refused: it holds an event of the todo the row would not write, as after a run stopped
    /// between the two stores and a list edited before the next. A store never takes an event
    /// back, so a second record would stand beside the first.
    Differs,
}

/// What becomes of the source.
enum Leaving {
    /// This `Cancelled`, whose note names the successor.
    Cancelled(TodoEvent),
    /// Nothing: a completed release stays completed.
    AsItWas,
}

/// What [`Carry::of`] reads.
pub(crate) struct Reading<'a> {
    /// The source's history.
    pub(crate) nodes: &'a [Node],
    /// Whom the source does not believe.
    pub(crate) untrusted: &'a Untrusted,
    /// Each todo's events the destination holds.
    pub(crate) held: BTreeMap<TodoId, Vec<TodoEvent>>,
    /// The release plan, on the source.
    pub(crate) plan: &'a Plan,
    /// The name the deployment mounts the destination as, which a successor's reference names.
    pub(crate) named: &'a ChainName,
    /// Who restates, and who cancels the source.
    pub(crate) carrier: &'a Actor,
    /// Each source actor's public name, from the list.
    pub(crate) actors: &'a BTreeMap<Actor, Actor>,
    /// The source cancels at this instant.
    pub(crate) now: Datetime,
}

impl Carry {
    /// The carry of `list` over `reading`, judged by `door` in one batch.
    ///
    /// # Errors
    /// A list that does not name every open todo of the source, names one that is not open
    /// (and that no earlier run carried), lets a release stay or renames it, or carries two todos
    /// to one id.
    pub(crate) fn of(
        reading: &Reading<'_>,
        list: &CarriedList,
        door: &Door,
    ) -> anyhow::Result<Carry> {
        let rows = planned(reading, list)?;
        let events: Vec<&TodoEvent> = rows.iter().flat_map(Row::new_events).collect();
        let mut verdicts: Vec<Verdict> = events.iter().map(|_| Ok(())).collect();
        door.judge(&events, &mut verdicts);
        let mut verdicts = verdicts.into_iter();
        Ok(Carry {
            rows: rows
                .into_iter()
                .map(|row| row.judged(&mut verdicts))
                .collect(),
            releases: reading
                .plan
                .releases()
                .iter()
                .map(|release| (release.todo.clone(), release.version))
                .collect(),
        })
    }

    /// How many rows are carried, stay and are refused, which partition the rows.
    #[must_use]
    pub(crate) fn counts(&self) -> (usize, usize, usize) {
        self.rows
            .iter()
            .fold((0, 0, 0), |(carried, staying, refused), row| {
                match &row.fate {
                    Fate::Stays { .. } => (carried, staying + 1, refused),
                    Fate::Unbuilt { .. } => (carried, staying, refused + 1),
                    Fate::Carried(carried_row) if carried_row.refused() => {
                        (carried, staying, refused + 1)
                    }
                    Fate::Done { .. } | Fate::Carried(_) => (carried + 1, staying, refused),
                }
            })
    }

    /// The events the destination gains, in order, and the source's cancellations.
    #[must_use]
    pub(crate) fn writes(&self) -> (Vec<&TodoEvent>, Vec<&TodoEvent>) {
        let carried = self.rows.iter().filter_map(|row| match &row.fate {
            Fate::Carried(carried) => Some(carried),
            Fate::Stays { .. } | Fate::Done { .. } | Fate::Unbuilt { .. } => None,
        });
        let mut destination = Vec::new();
        let mut source = Vec::new();
        for carried in carried {
            if let Landing::New(events) = &carried.landing {
                destination.extend(events.iter().map(|(event, _)| event));
            }
            if let Leaving::Cancelled(event) = &carried.leaving {
                source.push(event);
            }
        }
        (destination, source)
    }
}

impl<V> Row<V> {
    fn new_events(&self) -> impl Iterator<Item = &TodoEvent> {
        let events = match &self.fate {
            Fate::Carried(Carried {
                landing: Landing::New(events),
                ..
            }) => events.as_slice(),
            Fate::Carried(_) | Fate::Stays { .. } | Fate::Done { .. } | Fate::Unbuilt { .. } => &[],
        };
        events.iter().map(|(event, _)| event)
    }
}

impl Row<()> {
    /// The row with each new event's verdict, taken in order from `verdicts`.
    fn judged(self, verdicts: &mut impl Iterator<Item = Verdict>) -> Row<Verdict> {
        let fate = match self.fate {
            Fate::Stays { reason } => Fate::Stays { reason },
            Fate::Done { to } => Fate::Done { to },
            Fate::Unbuilt { to, why, refused } => Fate::Unbuilt { to, why, refused },
            Fate::Carried(carried) => Fate::Carried(Carried {
                landing: match carried.landing {
                    Landing::New(events) => Landing::New(
                        events
                            .into_iter()
                            .map(|(event, ())| event)
                            .zip(verdicts)
                            .collect(),
                    ),
                    Landing::Held => Landing::Held,
                    Landing::Differs => Landing::Differs,
                },
                to: carried.to,
                why: carried.why,
                leaving: carried.leaving,
            }),
        };
        Row { id: self.id, fate }
    }
}

impl Carried<Verdict> {
    fn refused(&self) -> bool {
        match &self.landing {
            Landing::New(events) => events.iter().any(|(_, verdict)| verdict.is_err()),
            Landing::Held => false,
            Landing::Differs => true,
        }
    }
}

/// The dry run's text: each row, then the counts on the last line.
impl std::fmt::Display for Carry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for row in &self.rows {
            let id = row.id.as_str();
            let release = self
                .releases
                .get(&row.id)
                .map_or_else(String::new, |version| format!(" (release {version})"));
            match &row.fate {
                Fate::Stays { reason } => writeln!(f, "{id}{release}  stay: {reason}")?,
                Fate::Done { to } => writeln!(
                    f,
                    "{id} → {}{release}  done: an earlier run carried it",
                    to.as_str()
                )?,
                Fate::Unbuilt { to, why, refused } => {
                    writeln!(f, "{id} → {}{release}  {why}", to.as_str())?;
                    writeln!(f, "  refused: {refused}")?;
                }
                Fate::Carried(carried) => {
                    writeln!(
                        f,
                        "{id} → {}{release}  {}",
                        carried.to.as_str(),
                        carried.why
                    )?;
                    match &carried.landing {
                        Landing::Held => writeln!(f, "  the destination holds it already")?,
                        Landing::Differs => writeln!(
                            f,
                            "  refused: the destination holds a different record of it"
                        )?,
                        Landing::New(events) => {
                            for (event, verdict) in events {
                                write!(
                                    f,
                                    "  {} at {}, door 1: ",
                                    event.kind_name(),
                                    event.at().isoformat()
                                )?;
                                match verdict {
                                    Ok(()) => writeln!(f, "taken")?,
                                    Err(Shut::Refused(why) | Shut::Unjudged(why)) => {
                                        writeln!(f, "refused: {why}")?;
                                    }
                                }
                                for (field, text) in published(event) {
                                    writeln!(f, "    {field} = {text:?}")?;
                                }
                            }
                        }
                    }
                }
            }
        }
        let (carried, staying, refused) = self.counts();
        writeln!(f, "carried {carried}, staying {staying}, refused {refused}")
    }
}

impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Why::Listed(restated) => write!(f, "carry: {}", restated.reason),
            Why::Closed(Closed::Shipped, _) => {
                write!(f, "completed: carried as completed; its items stay behind")
            }
            Why::Closed(Closed::Named, _) => write!(
                f,
                "closed: a carried price names it, so it is carried as it closed"
            ),
        }
    }
}

/// Every row before door 1 judges it.
fn planned(reading: &Reading<'_>, list: &CarriedList) -> anyhow::Result<Vec<Row<()>>> {
    let believed = Believed::of(reading.nodes, reading.now, reading.untrusted)
        .context("folding the source")?;
    let releases: Vec<&TodoId> = reading
        .plan
        .releases()
        .iter()
        .map(|release| &release.todo)
        .collect();
    let open: BTreeSet<&TodoId> = believed
        .content
        .iter()
        .filter(|(todo, records)| {
            believed.is_open(todo) && records.iter().all(|record| !record.payload.is_decision())
        })
        .map(|(todo, _)| todo)
        .collect();
    let closed = closed(reading, &believed, list, &open, &releases);
    let (to, done) = successors(reading, list, &open, &releases, &closed)?;
    let mut rows = Vec::new();
    for (todo, decision) in list
        .todos
        .iter()
        .filter(|(todo, _)| !closed.contains_key(todo))
    {
        let fate = match (decision, done.get(todo)) {
            (Decision::Stay { reason }, _) => Fate::Stays {
                reason: reason.clone(),
            },
            (Decision::Carry(_), Some(successor)) => Fate::Done {
                to: successor.clone(),
            },
            (Decision::Carry(restated), None) => {
                carried(reading, &believed, &to, todo, Why::Listed(restated.clone()))
            }
        };
        rows.push(Row {
            id: todo.clone(),
            fate,
        });
    }
    for (todo, how) in closed {
        let restated = match list.todos.get(todo) {
            Some(Decision::Carry(restated)) => restated.clone(),
            Some(Decision::Stay { .. }) | None => Restated::kept(),
        };
        rows.push(Row {
            id: todo.clone(),
            fate: carried(reading, &believed, &to, todo, Why::Closed(how, restated)),
        });
    }
    Ok(rows)
}

/// The closed todos carried as their history: each completed release, and each completed or
/// cancelled todo that a carried price names, whether the list carries that price's todo now or
/// an earlier run did, closed over the prices of the closed todos it adds.
fn closed<'a>(
    reading: &Reading<'_>,
    believed: &'a Believed,
    list: &CarriedList,
    open: &BTreeSet<&TodoId>,
    releases: &[&'a TodoId],
) -> BTreeMap<&'a TodoId, Closed> {
    let completed = |todo: &TodoId| {
        believed
            .outcome(todo)
            .iter()
            .all(|outcome| matches!(outcome, Some(Outcome::Completed(_))))
    };
    let mut closed: BTreeMap<&TodoId, Closed> = releases
        .iter()
        .copied()
        .filter(|todo| believed.content.contains_key(*todo) && completed(todo))
        .map(|todo| (todo, Closed::Shipped))
        .collect();
    let earlier = |todo: &TodoId| match list.todos.get(todo) {
        Some(Decision::Carry(restated)) => {
            cancelled_to(reading, todo, restated.to.as_ref().unwrap_or(todo))
        }
        Some(Decision::Stay { .. }) | None => false,
    };
    let by_name: BTreeMap<&str, &TodoId> = believed
        .content
        .keys()
        .map(|todo| (todo.as_str(), todo))
        .collect();
    let mut priced: Vec<&TodoId> = list
        .todos
        .iter()
        .filter(|(todo, decision)| {
            matches!(decision, Decision::Carry(_)) && (open.contains(todo) || earlier(todo))
        })
        .filter_map(|(todo, _)| by_name.get(todo.as_str()).copied())
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(todo) = priced.pop() {
        for name in names(believed, todo) {
            let Some(&item) = by_name.get(name.as_str()) else {
                continue;
            };
            if seen.insert(item)
                && !believed.is_open(item)
                && !earlier(item)
                && closed.get(item) != Some(&Closed::Named)
                && believed
                    .content
                    .get(item)
                    .into_iter()
                    .flatten()
                    .all(|record| !record.payload.is_decision())
            {
                closed.insert(item, Closed::Named);
                priced.push(item);
            }
        }
    }
    closed
}

/// The todos of this store that `todo`'s price names: the price [`built`] carries, its specs'
/// candidates, else its record's.
fn names(believed: &Believed, todo: &TodoId) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let specs: Vec<&Term> = match believed.specs.get(todo).map(Vec::as_slice) {
        Some(specs @ [_, ..]) => specs.iter().collect(),
        Some([]) | None => believed
            .content
            .get(todo)
            .into_iter()
            .flatten()
            .map(|record| &record.payload.spec)
            .collect(),
    };
    for spec in specs {
        spec.cata(|layer: TermF<()>| {
            if let TermF::Ref {
                store: None,
                entity,
            } = layer
            {
                names.insert(entity);
            }
        });
    }
    names
}

/// Each carried todo's successor by its id, an earlier run's beside the others, and the todos an
/// earlier run carried; or every way the list does not fit the source.
fn successors(
    reading: &Reading<'_>,
    list: &CarriedList,
    open: &BTreeSet<&TodoId>,
    releases: &[&TodoId],
    closed: &BTreeMap<&TodoId, Closed>,
) -> anyhow::Result<(BTreeMap<String, TodoId>, BTreeMap<TodoId, TodoId>)> {
    let mut wrong = Vec::new();
    for todo in open {
        if !list.todos.contains_key(*todo) {
            wrong.push(format!(
                "names no decision for {}, which is open",
                todo.as_str()
            ));
        }
    }
    let mut to: BTreeMap<String, TodoId> = BTreeMap::new();
    let mut done: BTreeMap<TodoId, TodoId> = BTreeMap::new();
    for (todo, decision) in &list.todos {
        let release = releases.contains(&todo);
        match decision {
            Decision::Stay { .. } if release => wrong.push(format!(
                "lets the release {} stay; the version reads its releases on the destination",
                todo.as_str()
            )),
            Decision::Carry(Restated { to: Some(_), .. }) if release => wrong.push(format!(
                "gives the release {} a new id; releases.toml names it by its own",
                todo.as_str()
            )),
            Decision::Carry(Restated {
                rationale: Field::Restated(_),
                ..
            }) if closed.get(todo) == Some(&Closed::Shipped) => wrong.push(format!(
                "restates the rationale of the completed release {}, which is carried with no \
                 price for one to explain",
                todo.as_str()
            )),
            Decision::Stay { .. } if closed.contains_key(todo) => wrong.push(format!(
                "lets {} stay, which a carried price names; it is carried as it closed",
                todo.as_str()
            )),
            Decision::Stay { .. } | Decision::Carry(_) => {}
        }
        if let Decision::Carry(restated) = decision {
            let successor = restated.to.clone().unwrap_or_else(|| todo.clone());
            if open.contains(todo) || closed.contains_key(todo) {
                to.insert(todo.as_str().to_owned(), successor);
            } else if cancelled_to(reading, todo, &successor) {
                done.insert(todo.clone(), successor);
            } else {
                wrong.push(format!(
                    "carries {}, which is not open and no carried price names",
                    todo.as_str()
                ));
            }
        } else if !open.contains(todo) && !closed.contains_key(todo) {
            wrong.push(format!("lets {} stay, which is not open", todo.as_str()));
        }
    }
    for todo in closed.keys() {
        to.entry(todo.as_str().to_owned())
            .or_insert_with(|| (*todo).clone());
    }
    for (from, successor) in &done {
        to.insert(from.as_str().to_owned(), successor.clone());
    }
    let mut landed: BTreeMap<&TodoId, &str> = BTreeMap::new();
    for (from, successor) in &to {
        if let Some(other) = landed.insert(successor, from) {
            wrong.push(format!(
                "carries {other} and {from} to one id, {}",
                successor.as_str()
            ));
        }
    }
    if !wrong.is_empty() {
        bail!("the carried list {}", wrong.join("; it "));
    }
    Ok((to, done))
}

/// Has the source cancelled `todo`, naming `successor`?
fn cancelled_to(reading: &Reading<'_>, todo: &TodoId, successor: &TodoId) -> bool {
    let note = successor_note(reading.named, successor);
    reading.nodes.iter().any(|node| {
        matches!(
            node.event(),
            Some(TodoEvent::Cancelled(cancelled)) if cancelled.todo == *todo && cancelled.note == note
        )
    })
}

/// The note of a carried todo's `Cancelled`: its successor's reference.
fn successor_note(named: &ChainName, successor: &TodoId) -> String {
    let reference = TodoRef {
        chain: named.clone(),
        id: successor.clone(),
    };
    format!("carried to {reference}")
}

/// The fate of one todo the carry takes: an open one with what the list restates, or a closed
/// one as it closed.
fn carried(
    reading: &Reading<'_>,
    believed: &Believed,
    to: &BTreeMap<String, TodoId>,
    todo: &TodoId,
    why: Why,
) -> Fate<()> {
    let successor = match &why {
        Why::Listed(Restated { to: Some(to), .. })
        | Why::Closed(Closed::Named, Restated { to: Some(to), .. }) => to.clone(),
        Why::Listed(Restated { to: None, .. })
        | Why::Closed(Closed::Named, Restated { to: None, .. })
        | Why::Closed(Closed::Shipped, _) => todo.clone(),
    };
    match built(reading, believed, to, todo, &successor, &why) {
        Ok((landing, leaving)) => Fate::Carried(Carried {
            to: successor,
            why,
            landing,
            leaving,
        }),
        Err(refused) => Fate::Unbuilt {
            to: successor,
            why,
            refused,
        },
    }
}

type Built = (Landing<()>, Leaving);

fn built(
    reading: &Reading<'_>,
    believed: &Believed,
    to: &BTreeMap<String, TodoId>,
    todo: &TodoId,
    successor: &TodoId,
    why: &Why,
) -> Result<Built, String> {
    let records = believed.content.get(todo).map_or(&[][..], Vec::as_slice);
    let specs = believed.specs.get(todo).map_or(&[][..], Vec::as_slice);
    let [source] = records else {
        return Err(conflict("content", records.len()));
    };
    let spec = match specs {
        [] => source.payload.spec.clone(),
        [spec] => spec.clone(),
        many => return Err(conflict("price", many.len())),
    };
    let outcomes = believed.outcome(todo);
    if outcomes.len() > 1 {
        return Err(conflict("lifecycle", outcomes.len()));
    }
    let at = reading
        .nodes
        .iter()
        .filter_map(Node::event)
        .filter(|event| event.todo() == todo && event.at() <= reading.now)
        .filter(|event| matches!(event, TodoEvent::Authored(_) | TodoEvent::SpecRevised(_)))
        .map(TodoEvent::at)
        .fold(source.at, std::cmp::max);
    if reading.untrusted.actors().contains(&source.actor) {
        return Err(format!(
            "its record is {}'s, which the source does not trust; confirm it there first",
            source.actor.as_str()
        ));
    }
    let (spec, restated) = match why {
        Why::Listed(restated) | Why::Closed(Closed::Named, restated) => {
            (renamed(&spec, to)?, restated.clone())
        }
        Why::Closed(Closed::Shipped, restated) => {
            (prodrome_core::fpl::mk_absent(), restated.clone())
        }
    };
    // A record with no price holds no rationale: there is nothing for one to explain.
    let rationale = match why {
        Why::Closed(Closed::Shipped, _) => Vec::new(),
        Why::Listed(_) | Why::Closed(Closed::Named, _) => restated
            .rationale
            .clone()
            .or(source.payload.rationale.clone()),
    };
    // Restated words are the carrier's own, so the carrier authors them; words carried as they
    // stand keep their author. Either is published under the name the list gives it.
    let actor = public(
        reading,
        if restated.restates() {
            reading.carrier
        } else {
            &source.actor
        },
    )?;
    let kept = source.payload.clone();
    let payload = Record {
        body: restated.body.or(kept.body),
        detail: restated.detail.or(kept.detail),
        rationale,
        waiting_on: restated.waiting_on.or(kept.waiting_on),
        category: restated.category.or(kept.category),
        subtodos: restated.subtodos.or(kept.subtodos),
        notes: restated.notes.or(kept.notes),
        spec,
        source: None,
        note: String::new(),
        ..kept
    };
    let mut events = vec![TodoEvent::Authored(Box::new(Authored {
        todo: successor.clone(),
        at,
        actor,
        payload,
    }))];
    let leaving = match why {
        Why::Listed(_) => Leaving::Cancelled(TodoEvent::Cancelled(Lifecycle {
            todo: todo.clone(),
            at: reading.now,
            actor: reading.carrier.clone(),
            note: successor_note(reading.named, successor),
        })),
        Why::Closed(..) => {
            events.push(closing(reading, todo, successor)?);
            Leaving::AsItWas
        }
    };
    let held = reading.held.get(successor).map_or(&[][..], Vec::as_slice);
    Ok((landing(events, held), leaving))
}

/// What the destination gains of `events`, given the events of the todo it `held`.
fn landing(mut events: Vec<TodoEvent>, held: &[TodoEvent]) -> Landing<()> {
    let prefix = events
        .iter()
        .take_while(|event| held.contains(event))
        .count();
    if held.iter().any(|event| !events[..prefix].contains(event)) {
        return Landing::Differs;
    }
    match events.split_off(prefix) {
        new if new.is_empty() => Landing::Held,
        new => Landing::New(new.into_iter().map(|event| (event, ())).collect()),
    }
}

/// A closed todo's first trusted closing, as its successor's, under its public actor: its first
/// completion where it has one, else its first cancellation, as a release reads an item's lot
/// (`suzatary_core::release::Lot`). The note is the source's own record of why (a release's is
/// the train's record of its land), so none is published.
fn closing(reading: &Reading<'_>, todo: &TodoId, successor: &TodoId) -> Result<TodoEvent, String> {
    let trusted: Vec<&TodoEvent> = reading
        .nodes
        .iter()
        .filter(|node| node.event().is_some_and(|e| e.todo() == todo))
        .filter(|node| {
            node.event()
                .is_some_and(|event| reading.untrusted.standing(node.name(), event).binds())
        })
        .filter_map(Node::event)
        .collect();
    let first = |of: fn(&TodoEvent) -> Option<&Lifecycle>| {
        trusted
            .iter()
            .filter_map(|event| of(event))
            .min_by_key(|closed| closed.at)
    };
    let published = |closed: &Lifecycle| -> Result<Lifecycle, String> {
        Ok(Lifecycle {
            todo: successor.clone(),
            at: closed.at,
            actor: public(reading, &closed.actor)?,
            note: String::new(),
        })
    };
    let completion = first(|event| match event {
        TodoEvent::Completed(closed) => Some(closed),
        _ => None,
    });
    let cancellation = first(|event| match event {
        TodoEvent::Cancelled(closed) => Some(closed),
        _ => None,
    });
    match (completion, cancellation) {
        (Some(closed), _) => published(closed).map(TodoEvent::Completed),
        (None, Some(closed)) => published(closed).map(TodoEvent::Cancelled),
        (None, None) => Err("its closing is not a trusted event".to_owned()),
    }
}

/// The name the list publishes `actor` under; an actor it does not name refuses the row.
fn public(reading: &Reading<'_>, actor: &Actor) -> Result<Actor, String> {
    reading.actors.get(actor).cloned().ok_or_else(|| {
        format!(
            "its actor {} has no public name in the list's [actors]",
            actor.as_str()
        )
    })
}

fn conflict(register: &str, candidates: usize) -> String {
    format!("its {register} holds a conflict of {candidates} writes; settle it on the source first")
}

/// `spec` with each reference renamed to its successor; a reference to a todo the carry leaves
/// behind, or to another store's, is refused.
fn renamed(spec: &Term, to: &BTreeMap<String, TodoId>) -> Result<Term, String> {
    spec.try_cata(|layer| match layer {
        TermF::Ref {
            store: None,
            entity,
        } => to
            .get(&entity)
            .map(|successor| {
                Term::new(TermF::Ref {
                    store: None,
                    entity: successor.as_str().to_owned(),
                })
            })
            .ok_or_else(|| format!("its price names {entity}, which the carry leaves behind")),
        TermF::Ref {
            store: Some(store),
            entity,
        } => Err(format!(
            "its price names {entity} of the chain {store}, and a public store names no other \
             store's todo"
        )),
        other => Ok(Term::new(other)),
    })
}

/// The texts one carry keeps the closed world's answer to: more than a carried list's bodies and
/// markup fields.
const TEXTS_KEPT: usize = 4096;

/// `suzatary prodrome carry`.
pub(crate) fn run(path: &Path, args: &super::Carrying) -> anyhow::Result<()> {
    let instance = Instance::open(path)?;
    let (source, plan) = upkeep(&instance)?;
    if Charter::read(&args.to)? != Charter::Public {
        bail!(
            "{} is not chartered public; `suzatary prodrome init --public` begins one",
            args.to.display()
        );
    }
    let named = ChainName::new(args.named.clone())?;
    let list_path = args
        .list
        .clone()
        .unwrap_or_else(|| instance.local.carried_list());
    // The list's bodies, the dry run's door and the destination's are one reading of each text.
    let typeset = Arc::new(Remembered::new(
        suzatary_io::typeset::Closed::this_binary(),
        TEXTS_KEPT,
    ));
    let list = CarriedList::parse(
        &std::fs::read_to_string(&list_path)
            .with_context(|| format!("reading the carried list {}", list_path.display()))?,
        &*typeset,
    )?;
    let actor = Actor::new(
        args.actor
            .clone()
            .unwrap_or_else(|| source.owner.as_str().to_owned()),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let lists = Lists::of(&instance.local, instance.code(), &instance.path);
    let door = || {
        Door::named(
            lists.clone(),
            list.public_actors(),
            Opened::AtTerminal,
            Arc::clone(&typeset) as Arc<dyn Typeset + Send>,
        )
    };
    let destination =
        crate::commands::chain_at(args.to.clone(), Untrusted::none()).public(door().rule());
    let writer = Instance::writer_of(source, &instance.signers(), &actor, Opened::AtTerminal);
    let now = suzatary_io::clock::SystemClock.now();
    let carry = || -> anyhow::Result<Carry> {
        let mut held: BTreeMap<TodoId, Vec<TodoEvent>> = BTreeMap::new();
        for event in destination
            .nodes()
            .context("reading the destination")?
            .iter()
            .filter_map(Node::event)
        {
            held.entry(event.todo().clone())
                .or_default()
                .push(event.clone());
        }
        let reading = Reading {
            nodes: &writer.nodes().context("reading the source")?,
            untrusted: &source.policy.untrusted,
            held,
            plan: &plan,
            named: &named,
            carrier: &actor,
            actors: &list.actors,
            now,
        };
        let carry = Carry::of(&reading, &list, &door())?;
        print!("{carry}");
        Ok(carry)
    };
    if !args.apply {
        return refusals(&carry()?, "the dry run wrote nothing");
    }
    let _source_held = writer.locked().context("taking the source's write lock")?;
    let _destination_held = destination
        .locked()
        .context("taking the destination's write lock")?;
    let carry = carry()?;
    refusals(&carry, "nothing was written to either store")?;
    let (into, cancelled) = carry.writes();
    let mut next = into.iter();
    destination
        .append_each(&mut || Ok(next.next().map(|event| (*event).clone())))
        .context("writing the destination")?;
    for event in &cancelled {
        writer
            .append((*event).clone())
            .context("cancelling on the source")?;
    }
    println!(
        "wrote {} events to the destination and cancelled {} todos on the source",
        into.len(),
        cancelled.len()
    );
    Ok(())
}

/// The upkeep chain and the release plan, whose releases are todos of that chain.
fn upkeep(instance: &Instance) -> anyhow::Result<(&ChainDir, Plan)> {
    let Some(upkeep) = &instance.deployment.upkeep else {
        bail!(
            "{}: no [upkeep] chain to carry from",
            instance.path.display()
        );
    };
    let plan = Plan::declared().context("the release plan this build carries")?;
    if plan.chain() != upkeep.as_str() {
        bail!(
            "the release plan's chain is {}, and the carry reads the upkeep chain {}",
            plan.chain(),
            upkeep.as_str()
        );
    }
    Ok((instance.chain(Some(upkeep.as_str()))?, plan))
}

fn refusals(carry: &Carry, then: &str) -> anyhow::Result<()> {
    match carry.counts() {
        (_, _, 0) => Ok(()),
        (_, _, refused) => bail!("{refused} carried todos are refused, and {then}"),
    }
}
