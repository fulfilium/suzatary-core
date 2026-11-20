use std::marker::PhantomData;

use prodrome_core::event::{Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::policy::Untrusted;
use prodrome_core::schema::Schema;
use prodrome_core::store::{MemoryStore, Replica, Sender};

use crate::stores::Store;
use suzatary_store::store::{InMemoryHold, Prodrome, Reads, StoreError, WriteLock};

/// A prodrome held in memory, for tests: the Prodrome's own `MemoryStore`, so an append writes
/// byte for byte what a store on disk holding the same objects writes, its heads are derived, and
/// an empty one is begun by a `Genesis` as a store on disk is.
///
/// Its `verify` is the store on disk's check of the same objects, under the policy every store
/// but a chain opens with (the law `the_store_in_memory_finds_what_the_store_on_disk_finds`), and
/// it is asked when the store is dropped: `H` says what the test expects of the answer.
/// `MemoryStore` verifies what it receives, so it never holds a missing parent or a print that
/// does not hash to its name.
#[derive(Debug)]
pub struct InMemory<E: Schema, H: Health = Healthy> {
    store: MemoryStore<E>,
    hold: InMemoryHold,
    /// The graph at the heads it was read at, as a store on disk keeps it.
    graph: std::sync::Mutex<Option<std::sync::Arc<suzatary_store::store::Graph<E>>>>,
    health: PhantomData<fn() -> H>,
}

/// What a test expects of an in-memory store's findings when the store is dropped.
pub trait Health {
    /// Told the findings when the store is dropped, unless its thread is already panicking.
    fn ended(findings: &[String]);
}

/// No finding: one fails the test that held the store.
#[derive(Debug)]
pub enum Healthy {}

/// A store a test writes a faulty object into on purpose: the test reads the findings itself.
#[derive(Debug)]
pub enum Faulty {}

impl Health for Healthy {
    fn ended(findings: &[String]) {
        assert!(
            findings.is_empty(),
            "an in-memory store ended holding what the store on disk would report (a test that \
             writes a faulty object on purpose builds its store as `InMemory<_, Faulty>`): \
             {findings:#?}"
        );
    }
}

impl Health for Faulty {
    fn ended(_: &[String]) {}
}

impl<E: Store<Vocabulary: Default>> Default for InMemory<E> {
    fn default() -> Self {
        Self::new(E::Vocabulary::default())
    }
}

impl<E: Store> InMemory<E> {
    /// A store in memory holding nothing, opened at the schema `schema`: a declared schema is a
    /// value, so its store has no default.
    #[must_use]
    pub fn at(schema: E::Vocabulary) -> Self {
        Self::new(schema)
    }
}

impl<E: Store<Vocabulary: Default>> InMemory<E, Faulty> {
    /// A store in memory holding nothing, which its test writes a faulty object into.
    #[must_use]
    pub fn faulty() -> Self {
        Self::new(E::Vocabulary::default())
    }
}

impl<E: Store, H: Health> InMemory<E, H> {
    fn new(schema: E::Vocabulary) -> Self {
        Self {
            store: MemoryStore::at(schema),
            hold: InMemoryHold::of(E::HOLD),
            graph: std::sync::Mutex::default(),
            health: PhantomData,
        }
    }

    fn refused(why: impl std::fmt::Display) -> StoreError {
        StoreError::not_appended("memory", why)
    }

    /// Begin the prodrome, with a nonce of zeros so a test's names are its own.
    fn begin(&self) -> Result<(), StoreError> {
        let genesis: Envelope<E> =
            Envelope::Genesis(mk_genesis("memory", &"0".repeat(32)).map_err(Self::refused)?);
        let name = seal_hash(&genesis);
        let print = canonical_envelope(&genesis).into_bytes();
        self.store
            .receive([name].into(), &|_| Some(print.clone()), Sender::Stranger)
            .map(drop)
            .map_err(Self::refused)
    }
}

impl<E: Schema, H: Health> InMemory<E, H> {
    /// What the store on disk reports of the same objects; a store that cannot fold them reports
    /// why.
    fn findings(&self) -> Vec<String> {
        match self.store.held() {
            Ok(held) => suzatary_signatures::signatures::findings_of(&held.dag, &Untrusted::none())
                .iter()
                .map(ToString::to_string)
                .collect(),
            Err(why) => vec![why.to_string()],
        }
    }
}

impl<E: Schema, H: Health> Drop for InMemory<E, H> {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            H::ended(&self.findings());
        }
    }
}

impl<E: Store, H: Health> Reads<E> for InMemory<E, H> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        let unreadable =
            |why: prodrome_core::literal::ProdromeError| StoreError::unreadable("memory", why);
        let dag = self.store.held().map_err(unreadable)?.dag;
        Ok(dag
            .linearise()
            .map_err(unreadable)?
            .iter()
            .map(|name| {
                let object = dag.objects()[name].clone();
                (name.clone(), object)
            })
            .collect())
    }

    fn graph(&self) -> Result<std::sync::Arc<suzatary_store::store::Graph<E>>, StoreError> {
        let heads: std::collections::BTreeSet<Hash> = self.tips()?.into_iter().collect();
        let mut kept = self
            .graph
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(graph) = kept.as_ref().filter(|graph| graph.heads() == &heads) {
            return Ok(std::sync::Arc::clone(graph));
        }
        let graph = std::sync::Arc::new(suzatary_store::store::Graph::of(self.objects()?));
        *kept = Some(std::sync::Arc::clone(&graph));
        Ok(graph)
    }

    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        let held = self
            .store
            .held()
            .map_err(|why| StoreError::unreadable("memory", why))?;
        Ok(suzatary_store::store::left_out(&held.dag))
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.store
            .tips()
            .map(|tips| tips.into_iter().collect())
            .map_err(|why| StoreError::unreadable("memory", why))
    }

    fn verify(&self) -> Vec<String> {
        self.findings()
    }
}

impl<E: Store, H: Health> Prodrome<E> for InMemory<E, H> {
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        if self.tips()?.is_empty() {
            self.begin()?;
        }
        self.store.append(event).map_err(Self::refused)
    }

    /// A stranger's, as the store on disk takes them (`suzatary_io::objects::Objects::accept`).
    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        let prints: std::collections::BTreeMap<&Hash, &String> =
            objects.iter().map(|(name, print)| (name, print)).collect();
        self.store
            .receive(
                prints.keys().map(|name| (*name).clone()).collect(),
                &|name| prints.get(name).map(|print| print.as_bytes().to_vec()),
                Sender::Stranger,
            )
            .map_err(Self::refused)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.hold.hold("memory")
    }
}
