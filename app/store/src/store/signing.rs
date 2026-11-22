use std::collections::BTreeSet;
use std::sync::Arc;

use prodrome_core::event::{Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::schema::Schema;
use prodrome_core::sign::{Secret, Signed};

use super::{Prodrome, Reads, StoreError, WriteLock};

/// A store whose every append is signed: the event's `Change`, then the key's `Signed` of it.
/// Signing is deterministic, so a replayed append writes nothing new.
pub struct Signing<S> {
    store: S,
    signer: Signer,
}

/// What one store's appends are signed with; a store whose key did not resolve still reads, and
/// its writes say why they cannot be made rather than land unsigned.
#[derive(Clone)]
pub enum Signer {
    /// The key is unavailable: every append refuses with this sentence.
    Refusing(String),
    Key(Arc<Secret>),
    /// For a public store, which never holds a deployment's keys.
    Unsigned,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Signer::Refusing(why) => f.debug_tuple("Refusing").field(why).finish(),
            Signer::Key(key) => f.debug_tuple("Key").field(&key.public()).finish(),
            Signer::Unsigned => f.write_str("Unsigned"),
        }
    }
}

impl<S> Signing<S> {
    #[must_use]
    pub fn new(store: S, signer: Signer) -> Self {
        Self { store, signer }
    }

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

/// `signed` joined into `store`: the one way this application adds a `Signed`.
/// Use `Secret::sign` only over a name just sealed; over a held name use `Secret::sign_whole`,
/// since a husk's name would prove a whole the signer never hashed.
pub fn sign<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    signed: Signed,
) -> Result<Hash, StoreError> {
    let signed = Envelope::<E>::Signed(signed);
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
    /// No hold of its own: a signature is an element beside its object, so two writers' changes
    /// and signatures interleave harmlessly.
    /// A crash between the two objects leaves an unsigned object that `verify` names and a
    /// replayed append signs.
    fn append(&self, event: E) -> Result<Hash, StoreError> {
        match &self.signer {
            Signer::Refusing(why) => Err(StoreError::not_appended("chain", why)),
            Signer::Key(key) => {
                let name = self.store.append(event)?;
                sign(&self.store, key.sign(&name))?;
                Ok(name)
            }
            Signer::Unsigned => self.store.append(event),
        }
    }

    fn accept(&self, objects: &[(Hash, String)]) -> Result<Vec<Hash>, StoreError> {
        self.store.accept(objects)
    }

    fn locked(&self) -> Result<Box<dyn WriteLock + '_>, StoreError> {
        self.store.locked()
    }
}
