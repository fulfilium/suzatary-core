use prodrome_core::event::{Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::schema::Schema;
use prodrome_core::store::{MemoryStore, Replica};

use super::{InMemoryHold, Prodrome, Reads, Store, StoreError, WriteLock};

/// A prodrome held in memory, for tests: the Prodrome's own `MemoryStore`, so an append writes
/// byte for byte what a store on disk holding the same objects writes, its heads are derived, and
/// an empty one is begun by a `Genesis` as a store on disk is.
#[derive(Debug)]
pub struct InMemory<E: Schema> {
    store: MemoryStore<E>,
    hold: InMemoryHold,
}

impl<E: Store<Vocabulary: Default>> Default for InMemory<E> {
    fn default() -> Self {
        Self {
            store: MemoryStore::default(),
            hold: InMemoryHold::of(E::HOLD),
        }
    }
}

impl<E: Store> InMemory<E> {
    /// A store in memory holding nothing, opened at the schema `schema`: a declared schema is a
    /// value, so its store has no default.
    #[must_use]
    pub fn at(schema: E::Vocabulary) -> Self {
        Self {
            store: MemoryStore::at(schema),
            hold: InMemoryHold::of(E::HOLD),
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
            .receive([name].into(), &|_| Some(print.clone()))
            .map(drop)
            .map_err(Self::refused)
    }
}

impl<E: Store> Reads<E> for InMemory<E> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        let unreadable =
            |why: prodrome_core::literal::ProdromeError| StoreError::unreadable("memory", why);
        let dag = self.store.held().map_err(unreadable)?.dag;
        Ok(dag
            .linearise()
            .map_err(unreadable)?
            .into_iter()
            .map(|name| {
                let object = dag.objects()[&name].clone();
                (name, object)
            })
            .collect())
    }

    fn excluded(&self) -> Result<std::collections::BTreeSet<Hash>, StoreError> {
        let held = self
            .store
            .held()
            .map_err(|why| StoreError::unreadable("memory", why))?;
        Ok(super::left_out(&held.dag))
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.store
            .tips()
            .map(|tips| tips.into_iter().collect())
            .map_err(|why| StoreError::unreadable("memory", why))
    }

    fn verify(&self) -> Vec<String> {
        Vec::new()
    }
}

impl<E: Store> Prodrome<E> for InMemory<E> {
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        if self.tips()?.is_empty() {
            self.begin()?;
        }
        self.store.append(event).map_err(Self::refused)
    }

    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        let prints: std::collections::BTreeMap<&Hash, &String> =
            objects.iter().map(|(name, print)| (name, print)).collect();
        self.store
            .receive(
                prints.keys().map(|name| (*name).clone()).collect(),
                &|name| prints.get(name).map(|print| print.as_bytes().to_vec()),
            )
            .map_err(Self::refused)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.hold.hold("memory")
    }
}
