use std::collections::BTreeSet;

use prodrome_dag::dag::Dag;
use prodrome_dag::sign::{Registrar, Secret};
use prodrome_data::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_data::policy::Untrusted;
use prodrome_data::schema::Schema;
use prodrome_data::sign::{KeyAdded, PublicKey, mk_key_revoked};
use prodrome_data::snapshot::mk_snapshot;

use super::{Custody, KeyStanding, Signers, Trust};
use suzatary_store::store::{Prodrome, Signer, StoreError, sign};

/// `KeyAdded(genesis, actor, key)` in every prodrome of `store`, signed by `root`; registering
/// twice writes one object.
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

/// `KeyRevoked(genesis, deps, actor, key)` wherever the root's registration stands; its deps are
/// every signature by `key` over `actor`'s writes, so those stay proven.
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

/// Every actor the root's registrations let `key` speak for in `store`.
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

/// Whether the root revoked `key` for `actor`; a revoked key is never registered again.
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

/// A revocation covers all keyed chains or none: this completes a revoke interrupted between
/// chains.
pub fn complete_revocations<E: Schema>(
    stores: &super::Keyed<&dyn Prodrome<E>>,
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

/// The root-signed `Snapshot` of each prodrome's heads, taken once per prodrome.
///
/// # Errors
/// An unsigned write since the epoch is refused: a second epoch would absorb it into history.
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
    Taken(Hash),
    /// Taken before, and nothing since is unsigned.
    Standing(Hash),
}

impl Epoch {
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
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(
        "{count} write(s) since the epoch carry no signature (the first is {first}); `suzatary \
         device sign-unsigned` signs each with the key its actor owns, and no new epoch will \
         absorb them"
    )]
    UnsignedSinceEpoch {
        count: usize,
        /// The first, by name.
        first: Hash,
    },
}

/// One write since an epoch with no signature, shown to the operator before [`sign_unsigned`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsignedWrite {
    pub name: Hash,
    /// Whose key would sign it.
    pub actor: Actor,
    pub at: prodrome_data::literal::Datetime,
}

/// Every unsigned write since an epoch; before an epoch, signing would put the root's name on
/// everything ever written.
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

/// Signs exactly `writes`, each by the key its actor owns: the repair for a crash between append
/// and sign.
/// Only the box's own writers leave an unsigned write, since the door refuses one posted unsigned.
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
    let dag = dag_of(store)?;
    writes
        .into_iter()
        .filter_map(|(name, actor)| match signers.for_actor(&actor, untrusted) {
            Signer::Key(key) => Some(
                key.sign_whole(&dag, &name)
                    .map_err(|e| StoreError::not_appended("chain", e))
                    .and_then(|signed| sign(store, signed)),
            ),
            Signer::Refusing(_) | Signer::Unsigned => None,
        })
        .collect()
}

fn written<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    root: &Secret,
    object: &Envelope<E>,
) -> Result<Hash, StoreError> {
    let name = seal_hash(object);
    store.accept(&[(name.clone(), canonical_envelope(object))])?;
    sign(store, root.sign(&name))?;
    Ok(name)
}

/// The store's own history, which knows which names it holds as husks.
fn dag_of<E: Schema>(store: &(impl Prodrome<E> + ?Sized)) -> Result<Dag<E>, StoreError> {
    Ok(store.graph()?.dag().clone())
}

/// Registers each roster actor's [`Custody`] key, then takes the epoch; idempotent, refusing as
/// [`epoch`] does.
pub fn adopt<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    keys: &super::BoxKeys,
    chain: &suzatary_base::chains::ChainPolicy,
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
    /// Each actor registered now.
    pub registered: Vec<(Actor, PublicKey)>,
    pub epochs: Vec<Epoch>,
}

/// Revokes each box key for every roster actor whose [`Custody`] is the other key's.
pub fn revoke_misplaced<E: Schema>(
    store: &(impl Prodrome<E> + ?Sized),
    keys: &super::BoxKeys,
    chain: &suzatary_base::chains::ChainPolicy,
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
