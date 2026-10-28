use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash};
use prodrome_core::memo::{self, Key};
use prodrome_core::registers::Genesis;
use prodrome_core::schema::Schema;
use prodrome_core::sign::{Checks, PublicKey, Registrar, Signed};

use super::{KeyStanding, Provenance, Unproven};
use crate::store::{Prodrome, StoreError};
use crate::table::Table;

/// How many readings a process keeps by their heads: a few chains, each under its registrar.
pub const KEPT_READINGS: usize = 8;

static READINGS: LazyLock<Table<Arc<Trust>>> = LazyLock::new(|| Table::new(KEPT_READINGS));

/// How many signature checks a process keeps: twenty times a chain of a few thousand objects'
/// signatures. A history holding more is read correctly and checks again what was dropped.
pub const KEPT_CHECKS: usize = 1 << 16;

/// This process's table of signature checks, the Prodrome's [`Checks`]. It is no input of any
/// reading, since it cannot change one, so no reader is handed it.
static CHECKS: LazyLock<Checks> = LazyLock::new(|| Checks::new(KEPT_CHECKS));

/// THE READING OF A HISTORY'S SIGNATURES, as a deployment reads them: what is proven, every
/// key's standing, the epochs and what lies beneath them, and why an object left out of the
/// proof is left out.
///
/// It MEANS the Prodrome's proof (its design §6.4):
///
/// ```text
/// proof(H) = { o ∈ H : ∃ s = Signed(o, k, σ) ∈ H. σ verifies,
///              KeyAdded(g(o), actor(o), k) counts in H,
///              ∀ r = KeyRevoked(g(o), _, actor(o), k) counting in H. s ∈ past(r) }
/// ```
///
/// and [`Trust::proves`] is `Proof::of` exactly (a law, tested over generated histories). It is
/// read here, in ONE fold over the objects, with each signature's check asked of a table of
/// [`Checks`], as `Proof::through` asks it: so a history read again costs the checks of the
/// signatures it gained.
///
/// A FUNCTION OF THE OBJECT SET (SPEC law 42): every answer is a question about which objects
/// exist and what each rests on, so two replicas holding the same objects read the same trust,
/// whatever order the objects arrived in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trust {
    /// The objects carrying an event that a key speaking for its actor signed.
    proven: BTreeSet<Hash>,
    /// Each `Snapshot` a counted key's signature covers: an epoch.
    epochs: BTreeSet<Hash>,
    /// Beneath an epoch: written before signatures.
    predating: BTreeSet<Hash>,
    /// Each signed object's signatures.
    signatures: BTreeMap<Hash, Vec<Signature>>,
    /// Each key registered by a key object that counts, and whether one that counts revokes it.
    keys: BTreeMap<(Genesis, Actor, PublicKey), KeyStanding>,
    /// The prodrome and actor of each object carrying an event.
    whose: BTreeMap<Hash, (Genesis, Actor)>,
    /// The prodrome of the history's least genesis.
    home: Genesis,
}

/// One `Signed` of an object: its own name, its key, and whether it verifies.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Signature {
    name: Hash,
    key: PublicKey,
    verifies: bool,
}

impl Trust {
    /// The trust `dag`'s objects give, its key objects counted as `registrar` says (the
    /// deployment's root, or anyone's, which is how `verify` reads them), each signature checked
    /// through this process's table of [`Checks`].
    #[must_use]
    pub fn of<E: Schema>(dag: &Dag<E>, registrar: &Registrar) -> Trust {
        Trust::through(dag, registrar, &CHECKS)
    }

    /// The trust a chain's store holds now, kept by its heads: a chain that has not moved is
    /// read once, and one that has moved is read again, checking only its new signatures. A
    /// revocation is an object, so it moves the heads and is read at once.
    ///
    /// # Errors
    /// The store unreadable.
    pub fn held<E: Schema>(
        store: &(impl Prodrome<E> + ?Sized),
        registrar: &Registrar,
    ) -> Result<Arc<Trust>, StoreError> {
        if let Some(kept) = READINGS.kept(&reading(registrar, &store.tips()?)) {
            return Ok(kept);
        }
        let dag: Dag<E> = store.objects()?.into_iter().collect();
        // Kept by the heads of what was read, never by the heads asked about first: a write
        // between the two is in this reading, so only its own heads may name it.
        let trust = Arc::new(Trust::of(&dag, registrar));
        let heads: Vec<Hash> = dag.tips().into_iter().collect();
        // A finding is kept as evidence and answers nothing, so the next read reads again.
        let _ = READINGS.keep(reading(registrar, &heads), Arc::clone(&trust));
        Ok(trust)
    }

    /// [`Trust::of`], each signature checked through `checks`.
    #[must_use]
    pub fn through<E: Schema>(dag: &Dag<E>, registrar: &Registrar, checks: &Checks) -> Trust {
        let mut signatures: BTreeMap<Hash, Vec<Signature>> = BTreeMap::new();
        for (name, object) in dag.objects() {
            if let Envelope::Signed(signed) = object {
                signatures
                    .entry(signed.object.clone())
                    .or_default()
                    .push(Signature {
                        name: name.clone(),
                        key: signed.key.clone(),
                        verifies: checks.verifies(signed),
                    });
            }
        }
        let counted = |key: &PublicKey| match registrar {
            Registrar::Anyone => true,
            Registrar::Roots(roots) => roots.contains(key),
        };
        // Signed by a key `registrar` counts, with a signature that verifies.
        let attested = |name: &Hash| {
            signatures
                .get(name)
                .is_some_and(|by| by.iter().any(|one| one.verifies && counted(&one.key)))
        };
        let counts = |name: &Hash| matches!(registrar, Registrar::Anyone) || attested(name);
        let mut keys: BTreeMap<(Genesis, Actor, PublicKey), KeyStanding> = BTreeMap::new();
        let mut kept: BTreeMap<(Genesis, Actor, PublicKey), Vec<BTreeSet<Hash>>> = BTreeMap::new();
        let mut epochs = BTreeSet::new();
        let mut whose = BTreeMap::new();
        for (name, object) in dag.objects() {
            match object {
                Envelope::KeyAdded(added) if counts(name) => {
                    keys.insert(
                        (
                            prodrome(dag, name, object),
                            added.actor.clone(),
                            added.key.clone(),
                        ),
                        KeyStanding::Trusted,
                    );
                }
                Envelope::KeyRevoked(revocation) if counts(name) => kept
                    .entry((
                        prodrome(dag, name, object),
                        revocation.actor.clone(),
                        revocation.key.clone(),
                    ))
                    .or_default()
                    .push(dag.closure(revocation.deps.iter().cloned())),
                Envelope::Snapshot(_) if attested(name) => {
                    epochs.insert(name.clone());
                }
                _ => {}
            }
            if let Some(event) = object.event() {
                whose.insert(
                    name.clone(),
                    (prodrome(dag, name, object), event.actor().clone()),
                );
            }
        }
        for revoked in kept.keys() {
            if let Some(standing) = keys.get_mut(revoked) {
                *standing = KeyStanding::Revoked;
            }
        }
        // A signature proves where its key is registered and every revocation of it keeps it.
        let proven = whose
            .iter()
            .filter(|(name, (genesis, actor))| {
                signatures.get(*name).is_some_and(|by| {
                    by.iter().any(|one| {
                        let whose = (genesis.clone(), actor.clone(), one.key.clone());
                        one.verifies
                            && keys.contains_key(&whose)
                            && kept.get(&whose).is_none_or(|pasts| {
                                pasts.iter().all(|past| past.contains(&one.name))
                            })
                    })
                })
            })
            .map(|(name, _)| name.clone())
            .collect();
        Trust {
            proven,
            predating: dag.closure(epochs.iter().cloned()),
            epochs,
            signatures,
            keys,
            whose,
            home: dag
                .geneses()
                .into_iter()
                .next()
                .and_then(|genesis| dag.key(&genesis)),
        }
    }

    /// Is `object` signed by a key that speaks for its actor?
    #[must_use]
    pub fn proves(&self, object: &Hash) -> bool {
        self.proven.contains(object)
    }

    /// The prodrome of the history's least genesis: where the door asks after a device's key.
    #[must_use]
    pub fn home(&self) -> &Genesis {
        &self.home
    }

    /// What the history says of `key` for `actor` in the prodrome `genesis`.
    #[must_use]
    pub fn standing(&self, genesis: &Genesis, actor: &Actor, key: &PublicKey) -> KeyStanding {
        self.keys
            .get(&(genesis.clone(), actor.clone(), key.clone()))
            .copied()
            .unwrap_or(KeyStanding::Untrusted)
    }

    /// THE ONE CHECK OF A KEY AT THE DOOR, for a posted object's signature and a device's request
    /// alike: `signed` verifies, and the history trusts its key for `actor` in `genesis`.
    ///
    /// # Errors
    /// Why not, in the words a refusal says.
    pub fn admits(
        &self,
        genesis: &Genesis,
        actor: &Actor,
        signed: &Signed,
    ) -> Result<(), Unproven> {
        if !signed.verifies() {
            return Err(Unproven::Forged);
        }
        match self.standing(genesis, actor, &signed.key) {
            KeyStanding::Trusted => Ok(()),
            KeyStanding::Revoked => Err(Unproven::Revoked {
                key: signed.key.clone(),
            }),
            KeyStanding::Untrusted => Err(Unproven::Untrusted {
                key: signed.key.clone(),
            }),
        }
    }

    /// Every registered key, with its prodrome, actor and standing.
    pub fn keys(&self) -> impl Iterator<Item = (&Genesis, &Actor, &PublicKey, KeyStanding)> {
        self.keys
            .iter()
            .map(|((genesis, actor, key), standing)| (genesis, actor, key, *standing))
    }

    /// Why `object` is not proven: `None` where it is, or where it carries no event (a
    /// signature, a key, a genesis or a snapshot asserts no actor's fact).
    #[must_use]
    pub fn why(&self, object: &Hash) -> Option<Unproven> {
        let (genesis, actor) = self.whose.get(object)?;
        if self.proves(object) {
            return None;
        }
        let verifying: Vec<&PublicKey> = self
            .signatures
            .get(object)
            .into_iter()
            .flatten()
            .filter(|one| one.verifies)
            .map(|one| &one.key)
            .collect();
        let Some(first) = verifying.first() else {
            return Some(if self.signatures.contains_key(object) {
                Unproven::Forged
            } else {
                Unproven::Unsigned
            });
        };
        Some(
            match verifying
                .iter()
                .find(|key| self.standing(genesis, actor, key) == KeyStanding::Revoked)
            {
                Some(key) => Unproven::Revoked {
                    key: (*key).clone(),
                },
                None => Unproven::Untrusted {
                    key: (*first).clone(),
                },
            },
        )
    }

    /// Who wrote `object`, as the reading says it: proven by a key, written before
    /// signatures, or unproven and why. `None` for an object that carries no event.
    #[must_use]
    pub fn provenance(&self, object: &Hash) -> Option<Provenance> {
        let (genesis, actor) = self.whose.get(object)?;
        if self.proves(object) {
            let key = self
                .signatures
                .get(object)
                .into_iter()
                .flatten()
                .find(|one| {
                    one.verifies
                        && self.standing(genesis, actor, &one.key) != KeyStanding::Untrusted
                })
                .map(|one| one.key.clone())?;
            return Some(Provenance::Proven { key });
        }
        if self.predating.contains(object) {
            return Some(Provenance::Predates);
        }
        self.why(object).map(Provenance::Unproven)
    }

    /// Every write after the epoch that carries no signature at all, with its actor: what a
    /// crash between an append and its signature leaves, or a writer that did not sign.
    pub fn unsigned(&self) -> impl Iterator<Item = (&Hash, &Actor)> {
        self.whose
            .iter()
            .filter(|(name, _)| {
                !self.predating.contains(*name) && !self.signatures.contains_key(*name)
            })
            .map(|(name, (_, actor))| (name, actor))
    }

    /// The EPOCHS: each `Snapshot` a key the registrar counts has signed (any key, read
    /// structurally; a root, read as the deployment). A snapshot attests a state of its
    /// prodrome, so the root's signature on one says "this is what the box held when it began
    /// signing", and nothing a writer does later can move it.
    #[must_use]
    pub fn epochs(&self) -> &BTreeSet<Hash> {
        &self.epochs
    }

    /// Everything beneath an epoch: the history written before signatures, which an unsigned
    /// object may belong to without being a finding. An epoch arriving moves objects into it,
    /// and nothing moves out.
    #[must_use]
    pub fn predating(&self) -> &BTreeSet<Hash> {
        &self.predating
    }

    /// Is `object` beneath an epoch?
    #[must_use]
    pub fn predates(&self, object: &Hash) -> bool {
        self.predating.contains(object)
    }
}

/// The prodrome `object` is in, as the Prodrome's `Dag` places it: a signature's is what it
/// signs', a legacy object's none.
fn prodrome<E: Schema>(dag: &Dag<E>, name: &Hash, object: &Envelope<E>) -> Genesis {
    match object {
        Envelope::Sealed { .. } | Envelope::Woven { .. } => None,
        Envelope::Genesis(_) => dag.key(name),
        Envelope::Signed(signed) => dag
            .objects()
            .get_key_value(&signed.object)
            .and_then(|(name, object)| prodrome(dag, name, object)),
        other => other.genesis().and_then(|genesis| dag.key(genesis)),
    }
}

/// The key a reading is kept under: the trust under `registrar` of the history `heads` name,
/// which is the whole of it, since a store holds every ancestor of its heads.
fn reading(registrar: &Registrar, heads: &[Hash]) -> Key {
    let counted = match registrar {
        Registrar::Anyone => "anyone".to_owned(),
        Registrar::Roots(roots) => roots
            .iter()
            .map(PublicKey::as_str)
            .collect::<Vec<_>>()
            .join(" "),
    };
    let mut heads = heads.to_vec();
    heads.sort();
    Key {
        function: memo::name(
            format!("suzatary signatures::Trust under {counted}").as_bytes(),
            [],
        ),
        argument: memo::name(b"heads", &heads),
    }
}
