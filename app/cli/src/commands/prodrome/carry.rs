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
//! carried todo cancelled naming its successor and writes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context as _, bail};
use prodrome_core::event::{Actor, Lifecycle, TodoId};
use prodrome_core::fpl::Outcome;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::{Policy as _, Untrusted};
use prodrome_core::term::{Term, TermF};
use suzatary_base::capability::Clock as _;
use suzatary_base::chains::ChainName;
use suzatary_base::reference::TodoRef;
use suzatary_core::release::{Counted, Plan};
use suzatary_record::record::{Authored, Node, Record, TodoEvent};
use suzatary_signatures::believed::Believed;
use suzatary_store::store::{Prodrome as _, Reads as _, Shut};

use super::list::{CarriedList, Decision, Restated};
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
    /// A completed release, carried so the version reads the same on the destination, with what
    /// the list restates of it.
    Shipped(Restated),
}

struct Carried<V> {
    to: TodoId,
    why: Why,
    landing: Landing<V>,
    leaving: Leaving,
}

/// What the destination gains.
enum Landing<V> {
    /// These events, in order: those of the todo's that the destination does not hold yet.
    New(Vec<(TodoEvent, V)>),
    /// Nothing: it holds every event of the todo already.
    Held,
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
            Why::Shipped(_) => write!(f, "completed: carried as completed; its items stay behind"),
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
    let shipped: Vec<&TodoId> = releases
        .iter()
        .copied()
        .filter(|todo| believed.content.contains_key(*todo))
        .filter(|todo| {
            believed
                .outcome(todo)
                .iter()
                .all(|outcome| matches!(outcome, Some(Outcome::Completed(_))))
        })
        .collect();
    let (to, done) = successors(reading, list, &open, &releases, &shipped)?;
    let mut rows = Vec::new();
    for (todo, decision) in list
        .todos
        .iter()
        .filter(|(todo, _)| !shipped.contains(todo))
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
    for todo in shipped {
        rows.push(Row {
            id: todo.clone(),
            fate: carried(
                reading,
                &believed,
                &to,
                todo,
                Why::Shipped(match list.todos.get(todo) {
                    Some(Decision::Carry(restated)) => restated.clone(),
                    Some(Decision::Stay { .. }) | None => Restated::kept(),
                }),
            ),
        });
    }
    Ok(rows)
}

/// Each carried todo's successor by its id, an earlier run's beside the others, and the todos an
/// earlier run carried; or every way the list does not fit the source.
fn successors(
    reading: &Reading<'_>,
    list: &CarriedList,
    open: &BTreeSet<&TodoId>,
    releases: &[&TodoId],
    shipped: &[&TodoId],
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
            Decision::Stay { .. } | Decision::Carry(_) => {}
        }
        if let (Decision::Carry(_), true) = (decision, shipped.contains(&todo)) {
            continue;
        }
        if let Decision::Carry(restated) = decision {
            let successor = restated.to.clone().unwrap_or_else(|| todo.clone());
            if open.contains(todo) {
                to.insert(todo.as_str().to_owned(), successor);
            } else if cancelled_to(reading, todo, &successor) {
                done.insert(todo.clone(), successor);
            } else {
                wrong.push(format!("carries {}, which is not open", todo.as_str()));
            }
        } else if !open.contains(todo) {
            wrong.push(format!("lets {} stay, which is not open", todo.as_str()));
        }
    }
    for todo in shipped {
        to.insert(todo.as_str().to_owned(), (*todo).clone());
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

/// The fate of one todo the carry takes: an open one with what the list restates, or a
/// completed release as completed, with no items.
fn carried(
    reading: &Reading<'_>,
    believed: &Believed,
    to: &BTreeMap<String, TodoId>,
    todo: &TodoId,
    why: Why,
) -> Fate<()> {
    let successor = match &why {
        Why::Listed(Restated { to: Some(to), .. }) => to.clone(),
        Why::Listed(Restated { to: None, .. }) | Why::Shipped(_) => todo.clone(),
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
        Why::Listed(restated) => (renamed(&spec, to)?, restated.clone()),
        Why::Shipped(restated) => (prodrome_core::fpl::mk_absent(), restated.clone()),
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
        rationale: restated.rationale.or(kept.rationale),
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
        Why::Shipped(_) => {
            events.push(completed(reading, todo, successor)?);
            Leaving::AsItWas
        }
    };
    let held = reading.held.get(successor).map_or(&[][..], Vec::as_slice);
    let new: Vec<(TodoEvent, ())> = events
        .into_iter()
        .filter(|event| !held.contains(event))
        .map(|event| (event, ()))
        .collect();
    let landing = if new.is_empty() {
        Landing::Held
    } else {
        Landing::New(new)
    };
    Ok((landing, leaving))
}

/// A completed release's first trusted completion, as its successor's, under its public actor.
/// The completion's note is the train's private record of the land, so none is published.
fn completed(
    reading: &Reading<'_>,
    todo: &TodoId,
    successor: &TodoId,
) -> Result<TodoEvent, String> {
    let completed = reading
        .nodes
        .iter()
        .filter(|node| node.event().is_some_and(|e| e.todo() == todo))
        .filter_map(|node| match node.event() {
            Some(TodoEvent::Completed(done))
                if reading
                    .untrusted
                    .standing(node.name(), node.event()?)
                    .binds() =>
            {
                Some(done)
            }
            _ => None,
        })
        .min_by_key(|done| done.at)
        .ok_or_else(|| "its completion is not a trusted event".to_owned())?;
    Ok(TodoEvent::Completed(Lifecycle {
        todo: successor.clone(),
        at: completed.at,
        actor: public(reading, &completed.actor)?,
        note: String::new(),
    }))
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
    let list = CarriedList::parse(
        &std::fs::read_to_string(&list_path)
            .with_context(|| format!("reading the carried list {}", list_path.display()))?,
        &suzatary_io::typeset::Closed::this_binary(),
    )?;
    let actor = Actor::new(
        args.actor
            .clone()
            .unwrap_or_else(|| source.owner.as_str().to_owned()),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let lists = Lists::of(&instance.local, instance.code(), &instance.path);
    let door = || Door::named(lists.clone(), list.public_actors(), Opened::AtTerminal);
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
