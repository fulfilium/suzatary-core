use std::collections::BTreeSet;

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::policy::Untrusted;
use prodrome_core::schema::Schema;
use prodrome_core::sign::{KeyAdded, PublicKey, Registrar, Secret, mk_key_revoked};
use prodrome_core::snapshot::mk_snapshot;

use super::{Custody, KeyStanding, Signers, Trust};
use crate::store::{Prodrome, Signer, StoreError, sign};

/// THE ROOT'S THREE WRITES, the only key objects this application writes, each joined into
/// `store` in every prodrome it holds and signed by `root`, so it counts.
///
/// `KeyAdded(genesis, actor, key)`: `key` now speaks for `actor`. Registering twice is one
/// object, so a tap answered twice registers once.
///
/// # Errors
/// The store unreadable, or refusing the objects.
pub fn trust<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &Secret,
    actor: &Actor,
    key: &PublicKey,
) -> Result<Vec<Hash>, StoreError> {
    let dag = dag_of(store)?;
    dag.geneses()
        .into_iter()
        .map(|genesis| {
            written(
                store,
                root,
                &Envelope::<E>::KeyAdded(KeyAdded {
                    genesis,
                    actor: actor.clone(),
                    key: key.clone(),
                }),
            )
        })
        .collect()
}

/// `KeyRevoked(genesis, deps, actor, key)`: `key` no longer speaks for `actor`, in each
/// prodrome where the root's registration of it stands, and speaks for every other actor as it
/// did. Its deps are every signature by `key` over `actor`'s writes the store holds there:
/// what was signed before stays proven, and nothing signed after, beside or unseen by this
/// revocation is. Where it is not trusted, nothing is written, so revoking twice is revoking
/// once.
///
/// # Errors
/// The store unreadable, or refusing the objects.
pub fn revoke<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &Secret,
    actor: &Actor,
    key: &PublicKey,
) -> Result<Vec<Hash>, StoreError> {
    let dag = dag_of(store)?;
    let reading = Trust::of(
        &dag,
        &Registrar::Roots([root.public()].into_iter().collect()),
    );
    dag.geneses()
        .into_iter()
        .filter(|genesis| reading.standing(&dag.key(genesis), actor, key) == KeyStanding::Trusted)
        .map(|genesis| {
            let kept: Vec<Hash> = dag
                .objects()
                .iter()
                .filter(|(name, object)| {
                    matches!(object, Envelope::Signed(signed)
                        if signed.key == *key
                            && dag.get(&signed.object).and_then(Envelope::event)
                                .is_some_and(|event| event.actor() == actor))
                        && dag.genesis_of(name).as_ref() == Some(&genesis)
                })
                .map(|(name, _)| name.clone())
                .collect();
            let revoked = mk_key_revoked(genesis, kept, actor.clone(), key.clone())
                .map_err(|e| StoreError::not_appended("chain", e))?;
            written(store, root, &Envelope::<E>::KeyRevoked(revoked))
        })
        .collect()
}

/// Every actor the root's registrations let `key` speak for in `store`: whom a stolen device's
/// key is revoked for.
///
/// # Errors
/// The store unreadable.
pub fn trusted_for<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &PublicKey,
    key: &PublicKey,
) -> Result<BTreeSet<Actor>, StoreError> {
    let reading = Trust::held(
        store,
        &Registrar::Roots([root.clone()].into_iter().collect()),
    )?;
    Ok(reading
        .keys()
        .filter(|(_, _, registered, standing)| *registered == key && standing.signs())
        .map(|(_, actor, _, _)| actor.clone())
        .collect())
}

/// Whether the root's history in `store` revoked `key` for `actor`: a revoked key is never
/// registered again, so a route that would trust it refuses first.
///
/// # Errors
/// The store unreadable.
pub fn revoked_for<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &PublicKey,
    actor: &Actor,
    key: &PublicKey,
) -> Result<bool, StoreError> {
    let reading = Trust::held(
        store,
        &Registrar::Roots([root.clone()].into_iter().collect()),
    )?;
    Ok(reading
        .keys()
        .any(|(_, registered_for, registered, standing)| {
            registered_for == actor && registered == key && standing == KeyStanding::Revoked
        }))
}

/// A revocation is ALL CHAINS OR NONE: every key a chain holds revoked is revoked on each other
/// chain that still trusts it, so a revoke interrupted between chains is completed by the next
/// writer to adopt. Answers what it wrote.
///
/// # Errors
/// A store unreadable, or refusing the objects.
pub fn complete_revocations<E: Schema>(
    stores: &[&dyn Prodrome<E>],
    root: &Secret,
) -> Result<Vec<Hash>, StoreError> {
    let roots = Registrar::Roots([root.public()].into_iter().collect());
    let readings = stores
        .iter()
        .map(|store| Ok(Trust::of(&dag_of(*store)?, &roots)))
        .collect::<Result<Vec<Trust>, StoreError>>()?;
    let revoked: BTreeSet<(Actor, PublicKey)> = readings
        .iter()
        .flat_map(Trust::keys)
        .filter(|(_, _, _, standing)| *standing == KeyStanding::Revoked)
        .map(|(_, actor, key, _)| (actor.clone(), key.clone()))
        .collect();
    let mut written = Vec::new();
    for (store, reading) in stores.iter().zip(&readings) {
        for (actor, key) in &revoked {
            let trusted_here = reading.keys().any(|(_, held, registered, standing)| {
                held == actor && registered == key && standing == KeyStanding::Trusted
            });
            if trusted_here {
                written.extend(revoke(*store, root, actor, key)?);
            }
        }
    }
    Ok(written)
}

/// The EPOCH: a `Snapshot` of each prodrome's heads, signed by the root, which says "this is
/// what the box held when it began signing". Everything beneath it reads as written before
/// signatures (`Provenance::Predates`); nothing written after can be placed beneath it.
///
/// Taken ONCE per prodrome. Asked again, it is the epoch already taken, unless a write since
/// then carries no signature: a second epoch would quietly absorb it into history, so that is
/// refused, naming the repair.
///
/// # Errors
/// [`AdoptError`].
pub fn epoch<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &Secret,
) -> Result<Vec<Epoch>, AdoptError> {
    let dag = dag_of(store)?;
    let roots = Registrar::Roots([root.public()].into_iter().collect());
    let trust = Trust::of(&dag, &roots);
    dag.geneses()
        .into_iter()
        .map(|genesis| {
            let within = |name: &Hash| dag.genesis_of(name).as_ref() == Some(&genesis);
            let Some(epoch) = trust.epochs().iter().find(|epoch| within(epoch)) else {
                let tips = dag.tips_in(&dag.key(&genesis)).into_iter().collect();
                let snapshot = mk_snapshot(genesis, tips, None)
                    .map_err(|e| StoreError::not_appended("chain", e))?;
                return Ok(Epoch::Taken(written(
                    store,
                    root,
                    &Envelope::<E>::Snapshot(snapshot),
                )?));
            };
            let unsigned: Vec<&Hash> = trust
                .unsigned()
                .map(|(name, _)| name)
                .filter(|name| within(name))
                .collect();
            match unsigned.first() {
                None => Ok(Epoch::Standing(epoch.clone())),
                Some(first) => Err(AdoptError::UnsignedSinceEpoch {
                    count: unsigned.len(),
                    first: (*first).clone(),
                }),
            }
        })
        .collect()
}

/// What asking for a prodrome's epoch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Epoch {
    /// Took it now: the prodrome began signing with this ask.
    Taken(Hash),
    /// It was taken before, and nothing since is unsigned.
    Standing(Hash),
}

impl Epoch {
    /// The snapshot's name.
    #[must_use]
    pub fn name(&self) -> &Hash {
        match self {
            Epoch::Taken(name) | Epoch::Standing(name) => name,
        }
    }
}

/// Why a store did not adopt signatures.
#[derive(Debug, thiserror::Error)]
pub enum AdoptError {
    /// The store would not read or take the objects.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Writes since the epoch that no key signed.
    #[error(
        "{count} write(s) since the epoch carry no signature (the first is {first}); `suzatary \
         device sign-unsigned` signs each with the key its actor owns, and no new epoch will \
         absorb them"
    )]
    UnsignedSinceEpoch {
        /// How many.
        count: usize,
        /// The first, by name.
        first: Hash,
    },
}

/// One write since an epoch that carries no signature: what [`sign_unsigned`] would sign, shown
/// to the operator before it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsignedWrite {
    /// The object.
    pub name: Hash,
    /// Whose event it is, and so whose key would sign it.
    pub actor: Actor,
    /// The instant the event claims.
    pub at: prodrome_core::literal::Datetime,
}

/// Every write since an epoch that carries no signature, in the order held: what a crash between
/// an append and its signature leaves. Only where an epoch was taken, since before one nothing is
/// history yet and signing would put the root's name on everything ever written.
///
/// # Errors
/// The store unreadable.
pub fn unsigned_writes<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &PublicKey,
) -> Result<Vec<UnsignedWrite>, StoreError> {
    let dag = dag_of(store)?;
    let roots = Registrar::Roots([root.clone()].into_iter().collect());
    let trust = Trust::of(&dag, &roots);
    let begun: BTreeSet<Hash> = trust
        .epochs()
        .iter()
        .filter_map(|epoch| dag.genesis_of(epoch))
        .collect();
    Ok(trust
        .unsigned()
        .filter(|(name, _)| dag.genesis_of(name).is_some_and(|g| begun.contains(&g)))
        .filter_map(|(name, actor)| {
            Some(UnsignedWrite {
                name: name.clone(),
                actor: actor.clone(),
                at: dag.get(name)?.event()?.at(),
            })
        })
        .collect())
}

/// THE REPAIR for a crash between an append and its signature: exactly `writes`, each signed by
/// the key its actor owns. Only the box's own writers leave such a write, since the door refuses
/// one posted unsigned; the operator sees the list ([`unsigned_writes`]) before asking for this.
///
/// # Errors
/// The store refusing the signatures.
pub fn sign_unsigned<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    signers: &Signers,
    untrusted: &Untrusted,
    writes: &[UnsignedWrite],
) -> Result<Vec<Hash>, StoreError> {
    signed_as_owned(
        store,
        signers,
        untrusted,
        writes
            .iter()
            .map(|write| (write.name.clone(), write.actor.clone()))
            .collect(),
    )
}

fn signed_as_owned<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    signers: &Signers,
    untrusted: &Untrusted,
    writes: Vec<(Hash, Actor)>,
) -> Result<Vec<Hash>, StoreError> {
    writes
        .into_iter()
        .filter_map(|(name, actor)| match signers.for_actor(&actor, untrusted) {
            Signer::Key(key) => Some(sign(store, &key, &name)),
            Signer::Refusing(_) => None,
        })
        .collect()
}

/// `object` joined into `store`, then signed by `root`: two objects, the second proving the
/// first is the root's.
fn written<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &Secret,
    object: &Envelope<E>,
) -> Result<Hash, StoreError> {
    let name = seal_hash(object);
    store.accept(&[(name.clone(), canonical_envelope(object))])?;
    sign(store, root, &name)?;
    Ok(name)
}

fn dag_of<E: Schema>(store: &(impl Prodrome<E> + ?Sized)) -> Result<Dag<E>, StoreError> {
    Ok(store.objects()?.into_iter().collect())
}

/// A chain ADOPTS signatures: each actor of its roster registered with the key its writes are
/// signed by ([`Custody`]), then the epoch. Every write from here on is held to a key;
/// everything before reads as written before signatures. Idempotent: adopted again, it writes
/// nothing, or refuses as [`epoch`] does; it answers what it registered and each prodrome's
/// epoch, taken now or before.
///
/// # Errors
/// [`AdoptError`].
pub fn adopt<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    keys: &super::BoxKeys,
    chain: &crate::chains::ChainPolicy,
) -> Result<Adoption, AdoptError> {
    let roots = Registrar::Roots([keys.root().public()].into_iter().collect());
    let reading = Trust::held(store, &roots)?;
    let held: BTreeSet<(&Actor, &PublicKey)> = reading
        .keys()
        .map(|(_, actor, key, _)| (actor, key))
        .collect();
    let mut registered = Vec::new();
    for actor in &chain.roster {
        let key = keys
            .of_custody(Custody::of(actor, &chain.untrusted))
            .public();
        trust(store, keys.root(), actor, &key)?;
        if !held.contains(&(actor, &key)) {
            registered.push((actor.clone(), key));
        }
    }
    Ok(Adoption {
        registered,
        epochs: epoch(store, keys.root())?,
    })
}

/// What adopting a chain did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoption {
    /// Each actor registered now, with the key that signs its writes.
    pub registered: Vec<(Actor, PublicKey)>,
    /// Each prodrome's epoch, taken now or before.
    pub epochs: Vec<Epoch>,
}

/// THE BOX'S TWO KEYS SPEAK ONLY FOR THE ACTORS WHOSE WRITES THEY SIGN: each revoked, where the
/// root's registration of it stands, for every actor of the roster whose [`Custody`] is the
/// other key's. What an adoption before custody left, the box's key registered for the agent's
/// actor; what it signed as that actor until now stays proven, nothing after. Answers each
/// (actor, key) it revoked, and asked again, nothing.
///
/// # Errors
/// The store unreadable, or refusing the objects.
pub fn revoke_misplaced<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    keys: &super::BoxKeys,
    chain: &crate::chains::ChainPolicy,
) -> Result<Vec<(Actor, PublicKey)>, StoreError> {
    let mut revoked = Vec::new();
    for actor in &chain.roster {
        let key = keys
            .of_custody(Custody::of(actor, &chain.untrusted).other())
            .public();
        if !revoke(store, keys.root(), actor, &key)?.is_empty() {
            revoked.push((actor.clone(), key));
        }
    }
    Ok(revoked)
}
