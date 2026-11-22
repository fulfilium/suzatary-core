//! The application's layers above the core, driven by law 68's scripts (`script_law.rs`): three
//! of the box's own stores on disk ([`Objects`]), each kept by the memo of its head set, read
//! through one cache of readings shared by the three ([`memoised`]), and a device's replica of
//! each ([`Held`]).
//!
//! After every step each actor's store on disk RECEIVES what its replica holds, and the check is:
//!
//! - each store's kept graph is what a handle that opens the store now reads;
//! - a store that took every object its replica holds reads as that replica;
//! - the reading the shared cache answers for each store is that store's fresh reading, though
//!   the three share one cache and a key names a head set, never a store;
//! - a device whose replica holds the objects of a store that another store's objects contain,
//!   told the other store's heads, reads as behind (the tab's `Fetching`) or as the other
//!   store whole, never as a whole that is wrong.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use prodrome_core::event::{Actor, Hash, canonical_envelope};
use prodrome_core::laws::script::{self, Script, Who, a_script};
use prodrome_core::laws::{Broken, Registers};
use prodrome_core::memo::{Cache, Lru};
use prodrome_core::policy::{Everything, Policy, Untrusted};
use prodrome_core::registers::fold;
use prodrome_core::store::{EventStore, Held as Replicated};
use proptest::prelude::*;
use proptest::test_runner::Config;

use suzatary_digest::secretary::digest_event::DigestEvent;
use suzatary_digest::secretary::digest_policy::Owners;
use suzatary_io::objects::Objects;
use suzatary_store::replica::frontier::Frontier;
use suzatary_store::replica::held::Held;
use suzatary_store::replica::objects::Object;
use suzatary_store::replica::word::Word;
use suzatary_store::store::{Prodrome, Reads as _, StoreError, memoised};
use suzatary_stores::stores::Store;

use crate::script_law::{
    STEPS, a_decision_write, a_digest_write, a_membership_write, a_memory_write, a_todo_write,
    an_alias_write, an_ask_write, an_inbox_write,
};

/// The scripts drawn for each store: each step writes and reads files.
const CASES: u32 = 8;

/// How many readings the shared cache keeps: few, so that it evicts.
const KEPT: usize = 4;

/// The law number the host's checks fail under: the script law's own.
const LAW: u32 = 68;

fn registers<E: Store>(store: &Objects<E>) -> Result<Registers<E>, StoreError> {
    Ok(Registers::of(&fold(&store.nodes()?)))
}

struct Layers<E: Store> {
    roots: [PathBuf; 3],
    stores: [Objects<E>; 3],
    open: fn(PathBuf) -> Objects<E>,
    cache: Mutex<Cache<Registers<E>, Lru>>,
}

fn broken(what: impl std::fmt::Display) -> Broken {
    Broken {
        law: LAW,
        what: what.to_string(),
    }
}

impl<E: Store<Vocabulary: Default>> Layers<E> {
    fn new(dir: &Path, open: fn(PathBuf) -> Objects<E>) -> Self {
        let roots = [0, 1, 2].map(|n| dir.join(n.to_string()));
        Layers {
            stores: roots.clone().map(open),
            roots,
            open,
            cache: Mutex::new(Cache::new(Lru::new(KEPT))),
        }
    }

    fn store(&self, who: Who) -> &Objects<E> {
        &self.stores[who.index()]
    }

    /// `who`'s store takes every object `replica` holds that it lacks, in one receipt. A door
    /// refuses a receipt whole, so then each object is offered alone, parents first, and a
    /// refused one is passed over, as the core's own law checks a refused receipt.
    fn mirror(&self, who: Who, replica: &Replicated<E>) -> Result<(), Broken> {
        let store = self.store(who);
        let held: BTreeSet<Hash> = store
            .objects()
            .map_err(broken)?
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let order = replica.dag.linearise().map_err(broken)?;
        let prints: Vec<(Hash, String)> = order
            .iter()
            .filter(|name| !held.contains(*name))
            .filter_map(|name| {
                let object = replica.dag.get(name)?;
                Some((name.clone(), canonical_envelope(object)))
            })
            .collect();
        if store.accept(&prints).is_err() {
            for print in &prints {
                let _refused = store.accept(std::slice::from_ref(print));
            }
        }
        Ok(())
    }

    fn cached(&self, who: Who) -> Result<Registers<E>, StoreError> {
        memoised(
            &self.cache,
            b"script_law_layers registers",
            self.store(who),
            registers,
        )
    }

    /// Every invariant of the module's head, for `who` and for every pair of stores once the
    /// last actor's has mirrored.
    fn check(&self, who: Who, replica: &Replicated<E>) -> Result<(), Broken> {
        self.mirror(who, replica)?;
        let kept = self.store(who);
        let fresh = (self.open)(self.roots[who.index()].clone());
        let (kept_graph, fresh_graph) = (
            kept.graph().map_err(broken)?,
            fresh.graph().map_err(broken)?,
        );
        let names = |graph: &suzatary_store::store::Graph<E>| -> BTreeSet<Hash> {
            graph.dag().objects().keys().cloned().collect()
        };
        if names(&kept_graph) != names(&fresh_graph) || kept_graph.heads() != fresh_graph.heads() {
            return Err(broken(format!(
                "{}'s kept graph is not its files'",
                who.name()
            )));
        }
        let now = registers(&fresh).map_err(broken)?;
        let mirrored: BTreeSet<Hash> = replica.dag.objects().keys().cloned().collect();
        if names(&fresh_graph) == mirrored
            && let Some(difference) = now.differs(&Registers::of(&replica.folded))
        {
            return Err(broken(format!(
                "{}'s store reads otherwise than its replica: {difference}",
                who.name()
            )));
        }
        if self.cached(who).map_err(broken)? != now {
            return Err(broken(format!(
                "the shared cache answers {} otherwise than a fresh fold",
                who.name()
            )));
        }
        match who {
            Who::Cai => self.devices(),
            Who::Ada | Who::Ben => Ok(()),
        }
    }

    /// A device's replica of one store, holding another store's objects, reads as behind or as
    /// the store whole.
    fn devices(&self) -> Result<(), Broken> {
        let mut wholes = Vec::new();
        for who in Who::ALL {
            let fresh = (self.open)(self.roots[who.index()].clone());
            let graph = fresh.graph().map_err(broken)?;
            let prints: Vec<String> = fresh
                .objects()
                .map_err(broken)?
                .iter()
                .map(|(_, object)| canonical_envelope(object))
                .collect();
            let names: BTreeSet<Hash> = graph.dag().objects().keys().cloned().collect();
            wholes.push((who, names, graph.heads().clone(), prints));
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
                    return Err(broken(format!(
                        "{}'s replica of {}'s store is not behind and reads {} of its {} objects",
                        device.name(),
                        box_.name(),
                        reads.len(),
                        whole.len()
                    )));
                }
            }
        }
        Ok(())
    }
}

fn holds<E: Store<Vocabulary: Default>, P: Policy<E> + Clone>(
    open: fn(PathBuf) -> Objects<E>,
    policy: &P,
    script: &Script<E>,
) -> Result<(), TestCaseError> {
    let dir = tempfile::tempdir().map_err(|e| TestCaseError::fail(e.to_string()))?;
    let layers = Layers::new(dir.path(), open);
    script::run_with(&E::Vocabulary::default(), script, policy, |who, replica| {
        layers.check(who, replica)
    })
    .map(drop)
    .map_err(|broken| TestCaseError::fail(broken.to_string()))
}

fn triage() -> Untrusted {
    Untrusted::of([Actor::new("triage").expect("an actor")])
}

fn chain(root: PathBuf) -> Objects<suzatary_record::record::TodoEvent> {
    Objects::chain(EventStore::new(root, triage()))
}

fn owners() -> BTreeSet<Actor> {
    [Who::Ada.actor()].into()
}

fn digests(root: PathBuf) -> Objects<DigestEvent> {
    Objects::digests(root, owners())
}

proptest! {
    #![proptest_config(Config::with_cases(CASES))]

    #[test]
    fn the_chain_s_layers_read_as_its_objects(script in a_script(a_todo_write, STEPS)) {
        holds(chain, &triage(), &script)?;
    }

    #[test]
    fn the_decisions_layers_read_as_their_objects(script in a_script(a_decision_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }

    #[test]
    fn the_inbox_s_layers_read_as_its_objects(script in a_script(an_inbox_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }

    #[test]
    fn the_memory_s_layers_read_as_its_objects(script in a_script(a_memory_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }

    #[test]
    fn the_topics_layers_read_as_their_objects(script in a_script(a_membership_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }

    #[test]
    fn the_aliases_layers_read_as_their_objects(script in a_script(an_alias_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }

    #[test]
    fn the_digests_layers_read_as_their_objects(script in a_script(a_digest_write, STEPS)) {
        let owners = owners();
        holds(digests, &Owners(&owners), &script)?;
    }

    #[test]
    fn the_asks_layers_read_as_their_objects(script in a_script(an_ask_write, STEPS)) {
        holds(Objects::open, &Everything, &script)?;
    }
}
