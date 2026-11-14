use std::collections::BTreeSet;
use std::sync::Arc;

use prodrome_core::event::{Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::schema::Schema;
use prodrome_core::sign::Secret;

use super::{Prodrome, Reads, StoreError, WriteLock};

/// A store whose every append is SIGNED: the event's `Change`, then the key's `Signed` of it,
/// two objects joined into the store. So a writer holding a `Signing` cannot
/// write an unsigned object, and which key signs is the value the store was built with: the
/// box's key for the box's stores.
///
/// Signing is deterministic, so a twin's signature is the object already held and an append
/// replayed writes nothing new, signature included.
pub struct Signing<S> {
    store: S,
    signer: Signer,
}

/// What one store's appends are signed with: a sum, because a deployment whose keys did not
/// resolve must still READ, and its writes must say why they cannot be made rather than land
/// unsigned.
#[derive(Clone)]
pub enum Signer {
    /// This key unavailable (secretspec locked after a reboot, a seed that
    /// is not the deployment's): every append refuses with this sentence and appends nothing.
    Refusing(String),
    /// Every append signed by this key.
    Key(Arc<Secret>),
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signer::Refusing(why) => f.debug_tuple("Refusing").field(why).finish(),
            Signer::Key(key) => f.debug_tuple("Key").field(&key.public()).finish(),
        }
    }
}

impl<S> Signing<S> {
    /// `store`, every append signed as `signer` says.
    #[must_use]
    pub fn new(store: S, signer: Signer) -> Self {
        Self { store, signer }
    }

    /// The store underneath.
    #[must_use]
    pub fn inner(&self) -> &S {
        &self.store
    }
}

impl<S> std::fmt::Debug for Signing<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signing")
            .field("signer", &self.signer)
            .finish_non_exhaustive()
    }
}

/// `key`'s signature of the object `name` holds, joined into `store`: the one way this
/// application adds a `Signed`, so a box write and a tap's registration sign alike.
///
/// # Errors
/// The store's refusal to take it, as any accept's.
pub fn sign<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    key: &Secret,
    name: &Hash,
) -> Result<Hash, StoreError> {
    let signed = Envelope::<E>::Signed(key.sign(name));
    let named = seal_hash(&signed);
    store.accept(&[(named.clone(), canonical_envelope(&signed))])?;
    Ok(named)
}

impl<E: Schema, S: Reads<E>> Reads<E> for Signing<S> {
    fn objects(&self) -> Result<Vec<(Hash, Envelope<E>)>, StoreError> {
        self.store.objects()
    }

    fn graph(&self) -> Result<std::sync::Arc<super::Graph<E>>, StoreError> {
        self.store.graph()
    }

    fn literals(&self, names: &[Hash]) -> Result<Vec<Option<String>>, StoreError> {
        self.store.literals(names)
    }

    fn sizes(&self, names: &[Hash]) -> Result<Vec<Option<usize>>, StoreError> {
        self.store.sizes(names)
    }

    fn excluded(&self) -> Result<BTreeSet<Hash>, StoreError> {
        self.store.excluded()
    }

    fn tips(&self) -> Result<Vec<Hash>, StoreError> {
        self.store.tips()
    }

    fn verify(&self) -> Vec<String> {
        self.store.verify()
    }
}

impl<E: Schema, S: Prodrome<E>> Prodrome<E> for Signing<S> {
    /// No hold of its own: a signature is an element beside its object, so two writers'
    /// changes and signatures interleave harmlessly, and a crash between the two leaves an
    /// unsigned object that `verify` names and the same append, replayed, signs.
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        match &self.signer {
            Signer::Refusing(why) => Err(StoreError::not_appended("chain", why)),
            Signer::Key(key) => {
                let name = self.store.append(event)?;
                sign(&self.store, key, &name)?;
                Ok(name)
            }
        }
    }

    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        self.store.accept(objects)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.store.locked()
    }
}
