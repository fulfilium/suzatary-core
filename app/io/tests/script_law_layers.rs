//! The application's layers above the core, driven by law 68's scripts (`script_law.rs`): three
//! of the box's own stores on disk ([`Objects`]), each kept by the memo of its head set, read
//! through one cache of readings shared by the three ([`memoised`]), and a device's replica of
//! each ([`Held`]).
//!
//! The steps that reach these layers are taken: an append, a partial sync in any order with
//! duplicates, a sync, a sync of all, and a read through the cache. The rest (signatures, keys,
//! refused receipts, snapshots, the core's own cache) are the core's, which `script_law.rs`
//! checks. After every step:
//!
//! - each store's kept graph is what a handle that opens the store now reads;
//! - the reading the shared cache answers for each store is that store's fresh reading, though
//!   the three share one cache and a key names a head set, never a store;
//! - a device whose replica holds the objects of a store that another store's objects contain,
//!   told the other store's heads, reads as behind (the tab's `Fetching`) or as the other
//!   store whole, never as a whole that is wrong.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use prodrome_core::event::{Actor, Hash, canonical_envelope};
use prodrome_core::fold::{Product as _, read};
use prodrome_core::laws::script::{Script, Step, Who, a_script};
use prodrome_core::memo::{Cache, Lru};
use prodrome_core::policy::{Everything, Untrusted};
use prodrome_core::registers::fold;
use prodrome_core::store::EventStore;
use proptest::prelude::*;
use proptest::test_runner::Config;

use suzatary_io::objects::Objects;
use suzatary_secretary::stores::Store;
use suzatary_store::replica::frontier::Frontier;
use suzatary_store::replica::held::Held;
use suzatary_store::replica::objects::Object;
use suzatary_store::replica::word::Word;
use suzatary_store::store::{Prodrome, Reads as _, StoreError, memoised};

use crate::script_law::{
    STEPS, a_decision_write, a_digest_write, a_membership_write, a_memory_write, a_todo_write,
    an_alias_write, an_ask_write, an_inbox_write,
};

/// The scripts drawn for each store: each step writes and reads files.
const CASES: u32 = 8;

/// How many readings the shared cache keeps: few, so that it evicts.
const KEPT: usize = 4;

/// Every entity's registers read whole, each as the names of the writes it reads: what a reading
/// through the cache is compared by.
type Registers = BTreeMap<String, Vec<BTreeSet<Hash>>>;

fn registers<E: Store>(store: &Objects<E>) -> Result<Registers, StoreError>
where
    E::Key: std::fmt::Debug,
{
    let nodes = store.nodes()?;
    Ok(fold(&nodes)
        .entities()
        .map(|(key, stream)| {
            let read = read(stream, None, &Everything);
            let each = read
                .registers()
                .into_iter()
                .map(|register| {
                    read.reading(register)
                        .iter()
                        .map(|stamp| stamp.name.clone())
                        .collect()
                })
                .collect();
            (format!("{key:?}"), each)
        })
        .collect())
}

/// Three stores of one prodrome on disk, opened by `open`, and the cache they share.
struct Layers<E: Store> {
    roots: [PathBuf; 3],
    stores: [Objects<E>; 3],
    open: fn(PathBuf) -> Objects<E>,
    cache: Mutex<Cache<Registers, Lru>>,
}

impl<E: Store<Vocabulary: Default>> Layers<E>
where
    E::Key: std::fmt::Debug,
{
    fn new(dir: &Path, open: fn(PathBuf) -> Objects<E>) -> Result<Self, String> {
        let roots = [0, 1, 2].map(|n| dir.join(n.to_string()));
        let stores = roots.clone().map(open);
        stores[0].begin().map_err(|e| e.to_string())?;
        let layers = Layers {
            roots,
            stores,
            open,
            cache: Mutex::new(Cache::new(Lru::new(KEPT))),
        };
        for to in [Who::Ben, Who::Cai] {
            layers.give(Who::Ada, to, &[None])?;
        }
        Ok(layers)
    }

    fn store(&self, who: Who) -> &Objects<E> {
        &self.stores[who.index()]
    }

    /// `to` takes, for each of `chosen` in order, the object at that position among `from`'s
    /// with what it rests on, or everything `from` holds for `None`. A receipt the store's door
    /// refuses takes nothing, as the core's own law checks; it is passed over here.
    fn give(&self, from: Who, to: Who, chosen: &[Option<usize>]) -> Result<(), String> {
        let graph = self.store(from).graph().map_err(|e| e.to_string())?;
        let names: Vec<&Hash> = graph.dag().objects().keys().collect();
        for at in chosen {
            let wanted: BTreeSet<Hash> = match at {
                Some(at) => graph.dag().closure([names[at % names.len()].clone()]),
                None => names.iter().map(|name| (*name).clone()).collect(),
            };
            let prints: Vec<(Hash, String)> = self
                .store(from)
                .objects()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|(name, _)| wanted.contains(name))
                .map(|(name, object)| (name, canonical_envelope(&object)))
                .collect();
            let _refused = self.store(to).accept(&prints);
        }
        Ok(())
    }

    fn step(&self, step: &Step<E>) -> Result<(), String> {
        match step {
            Step::Append { by, event, .. } => {
                // An append the schema or the door refuses writes nothing (the core's law 20).
                let _refused = self.store(*by).append(event.clone());
                Ok(())
            }
            Step::Pick { from, to, chosen } => {
                let chosen: Vec<Option<usize>> = chosen.iter().copied().map(Some).collect();
                self.give(*from, *to, &chosen)
            }
            Step::Sync { from, to } => self.give(*from, *to, &[None]),
            Step::SyncAll => {
                for (from, to) in [
                    (Who::Ben, Who::Ada),
                    (Who::Cai, Who::Ada),
                    (Who::Ada, Who::Ben),
                    (Who::Ada, Who::Cai),
                ] {
                    self.give(from, to, &[None])?;
                }
                Ok(())
            }
            Step::Read { by, .. } => self.cached(*by).map(drop),
            Step::Refuse { .. }
            | Step::Rotate { .. }
            | Step::Revoke { .. }
            | Step::Record { .. }
            | Step::Snapshot { .. }
            | Step::Forget { .. }
            | Step::Flush { .. } => Ok(()),
        }
    }

    /// `who`'s reading through the shared cache.
    fn cached(&self, who: Who) -> Result<Registers, String> {
        memoised(
            &self.cache,
            b"script_law_layers registers",
            self.store(who),
            registers,
        )
        .map_err(|e| e.to_string())
    }

    /// Every invariant of the module's head, after a step.
    fn check(&self) -> Result<(), String> {
        let mut wholes = Vec::new();
        for who in Who::ALL {
            let kept = self.store(who);
            let fresh = (self.open)(self.roots[who.index()].clone());
            let (kept_graph, fresh_graph) = (
                kept.graph().map_err(|e| e.to_string())?,
                fresh.graph().map_err(|e| e.to_string())?,
            );
            let names = |graph: &suzatary_store::store::Graph<E>| -> BTreeSet<Hash> {
                graph.dag().objects().keys().cloned().collect()
            };
            if names(&kept_graph) != names(&fresh_graph)
                || kept_graph.heads() != fresh_graph.heads()
            {
                return Err(format!("{}'s kept graph is not its files'", who.name()));
            }
            let now = registers(&fresh).map_err(|e| e.to_string())?;
            if self.cached(who)? != now {
                return Err(format!(
                    "the shared cache answers {} otherwise than a fresh fold",
                    who.name()
                ));
            }
            let prints: Vec<String> = fresh
                .objects()
                .map_err(|e| e.to_string())?
                .iter()
                .map(|(_, object)| canonical_envelope(object))
                .collect();
            wholes.push((
                who,
                names(&fresh_graph),
                fresh_graph.heads().clone(),
                prints,
            ));
        }
        for (box_, whole, heads, _) in &wholes {
            for (device, held, _, prints) in &wholes {
                if box_ == device || !held.is_subset(whole) {
                    continue;
                }
                let replica = Held::of(
                    0,
                    prints
                        .iter()
                        .map(|print| Object::read::<E>(&E::Vocabulary::default(), print.clone())),
                    vec![Word {
                        epoch: 1,
                        heads: heads.clone(),
                        geneses: BTreeSet::new(),
                        fetched_at: 0,
                    }],
                    Frontier::Nothing,
                );
                let read = replica.read();
                let reads: BTreeSet<Hash> = read
                    .nodes()
                    .map(|node| suzatary_store::replica::held::Node::name(node).clone())
                    .collect();
                if !read.behind(heads) && reads != *whole {
                    return Err(format!(
                        "{}'s replica of {}'s store is not behind and reads {} of its {} objects",
                        device.name(),
                        box_.name(),
                        reads.len(),
                        whole.len()
                    ));
                }
            }
        }
        Ok(())
    }
}

/// `script` over three stores `open` opens, every invariant checked after every step.
fn holds<E: Store<Vocabulary: Default>>(
    open: fn(PathBuf) -> Objects<E>,
    script: &Script<E>,
) -> Result<(), TestCaseError>
where
    E::Key: std::fmt::Debug,
{
    let dir = tempfile::tempdir().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let layers = Layers::new(dir.path(), open).map_err(TestCaseError::fail)?;
    for (n, step) in script.steps.iter().enumerate() {
        layers
            .step(step)
            .and_then(|()| layers.check())
            .map_err(|what| TestCaseError::fail(format!("at step {n}: {what}")))?;
    }
    Ok(())
}

fn chain(root: PathBuf) -> Objects<suzatary_record::record::TodoEvent> {
    Objects::chain(EventStore::new(
        root,
        Untrusted::of([Actor::new("triage").expect("an actor")]),
    ))
}

fn digests(root: PathBuf) -> Objects<suzatary_digest::secretary::digest_event::DigestEvent> {
    Objects::digests(root, [Who::Ada.actor()].into())
}

proptest! {
    #![proptest_config(Config::with_cases(CASES))]

    #[test]
    fn the_chain_s_layers_read_as_its_objects(script in a_script(a_todo_write, STEPS)) {
        holds(chain, &script)?;
    }

    #[test]
    fn the_decisions_layers_read_as_their_objects(script in a_script(a_decision_write, STEPS)) {
        holds(Objects::open, &script)?;
    }

    #[test]
    fn the_inbox_s_layers_read_as_its_objects(script in a_script(an_inbox_write, STEPS)) {
        holds(Objects::open, &script)?;
    }

    #[test]
    fn the_memory_s_layers_read_as_its_objects(script in a_script(a_memory_write, STEPS)) {
        holds(Objects::open, &script)?;
    }

    #[test]
    fn the_topics_layers_read_as_their_objects(script in a_script(a_membership_write, STEPS)) {
        holds(Objects::open, &script)?;
    }

    #[test]
    fn the_aliases_layers_read_as_their_objects(script in a_script(an_alias_write, STEPS)) {
        holds(Objects::open, &script)?;
    }

    #[test]
    fn the_digests_layers_read_as_their_objects(script in a_script(a_digest_write, STEPS)) {
        holds(digests, &script)?;
    }

    #[test]
    fn the_asks_layers_read_as_their_objects(script in a_script(an_ask_write, STEPS)) {
        holds(Objects::open, &script)?;
    }
}
