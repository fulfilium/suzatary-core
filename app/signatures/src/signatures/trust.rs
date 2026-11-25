use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};

use prodrome_dag::dag::Dag;
use prodrome_dag::registers::Genesis;
use prodrome_dag::sign::{Checks, Registrar};
use prodrome_data::event::{Actor, Envelope, Hash};
use prodrome_data::schema::Schema;
use prodrome_data::sign::{PublicKey, Signed};
use prodrome_memo::{self as memo, Key};

use super::{KeyStanding, Provenance, Unproven};
use suzatary_base::table::Table;
use suzatary_store::store::{Prodrome, StoreError};

/// How many readings a process keeps by their heads.
pub(crate) const KEPT_READINGS: usize = 8;

static READINGS: LazyLock<Table<Arc<Trust>>> = LazyLock::new(|| Table::new(KEPT_READINGS));

/// How many signature checks a process keeps; a larger history reads correctly and checks again.
pub const KEPT_CHECKS: usize = 1 << 16;

/// This process's table of signature checks; it cannot change a reading, so no reader is handed it.
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
/// and [`Trust::proves`] is `Proof::of` exactly (a law, tested over generated histories): a
/// signature proves only the bytes its name is the hash of, and of a husk it vouches. It is
/// read here, in ONE fold over the objects, with each signature's check asked of a table of
/// [`Checks`], as `Proof::through` asks it: so a history read again costs the checks of the
/// signatures it gained.
///
/// A FUNCTION OF THE OBJECT SET (SPEC law 42): every answer is a question about which objects
/// exist and what each rests on, so two replicas holding the same objects read the same trust,
/// whatever order the objects arrived in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trust {
    proven: BTreeSet<Hash>,
    /// Held as husks (SPEC §3): the signature vouches for each and proves none.
    vouched: BTreeSet<Hash>,
    /// Each `Snapshot` a counted key's signature covers: an epoch.
    epochs: BTreeSet<Hash>,
    predating: BTreeSet<Hash>,
    signatures: BTreeMap<Hash, Vec<Signature>>,
    /// Each key registered by a key object that counts.
    keys: BTreeMap<(Genesis, Actor, PublicKey), KeyStanding>,
    whose: BTreeMap<Hash, (Genesis, Actor)>,
    /// The prodrome of the history's least genesis.
    home: Genesis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Signature {
    name: Hash,
    key: PublicKey,
    verifies: bool,
}

impl Trust {
    /// The trust `dag`'s objects give, key objects counted as `registrar` says (`verify` reads
    /// under anyone's).
    #[must_use]
    pub fn of<E: Schema>(dag: &Dag<E>, registrar: &Registrar) -> Trust {
        Trust::through(dag, registrar, &CHECKS)
    }

    /// The trust a chain's store holds now, kept by its heads; a revocation moves the heads, so
    /// it is read at once.
    pub fn held<E: Schema>(
        store: &(impl Prodrome<E> + ?Sized),
        registrar: &Registrar,
    ) -> Result<Arc<Trust>, StoreError> {
        if let Some(kept) = READINGS.kept(&reading(registrar, &store.tips()?)) {
            return Ok(kept);
        }
        let graph = store.graph()?;
        let dag = graph.dag();
        // Keep it by the heads read, not the heads asked: a write between them is in this reading.
        let trust = Arc::new(Trust::of(dag, registrar));
        let heads: Vec<Hash> = dag.tips().into_iter().collect();
        // A finding is kept as evidence and answers nothing, so the next read reads again.
        let _ = READINGS.keep(reading(registrar, &heads), Arc::clone(&trust));
        Ok(trust)
    }

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
        let (vouched, proven) = whose
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
            .partition(|name| dag.husks().contains(name));
        Trust {
            proven,
            vouched,
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

    #[must_use]
    pub fn proves(&self, object: &Hash) -> bool {
        self.proven.contains(object)
    }

    /// The prodrome of the history's least genesis: where the door asks after a device's key.
    #[must_use]
    pub fn home(&self) -> &Genesis {
        &self.home
    }

    #[must_use]
    pub fn standing(&self, genesis: &Genesis, actor: &Actor, key: &PublicKey) -> KeyStanding {
        self.keys
            .get(&(genesis.clone(), actor.clone(), key.clone()))
            .copied()
            .unwrap_or(KeyStanding::Untrusted)
    }

    /// The one key check at the door, for posted objects and device requests: `signed` verifies
    /// and its key is trusted for `actor` in `genesis`.
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

    pub fn keys(&self) -> impl Iterator<Item = (&Genesis, &Actor, &PublicKey, KeyStanding)> {
        self.keys
            .iter()
            .map(|((genesis, actor, key), standing)| (genesis, actor, key, *standing))
    }

    /// Why `object` is not proven; `None` when it is proven, vouched for, or carries no event.
    #[must_use]
    pub fn why(&self, object: &Hash) -> Option<Unproven> {
        let (genesis, actor) = self.whose.get(object)?;
        if self.proves(object) || self.vouched.contains(object) {
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

    /// `None` for an object that carries no event.
    #[must_use]
    pub fn provenance(&self, object: &Hash) -> Option<Provenance> {
        let (genesis, actor) = self.whose.get(object)?;
        let signer = || {
            self.signatures
                .get(object)
                .into_iter()
                .flatten()
                .find(|one| {
                    one.verifies
                        && self.standing(genesis, actor, &one.key) != KeyStanding::Untrusted
                })
                .map(|one| one.key.clone())
        };
        if self.proves(object) {
            return Some(Provenance::Proven { key: signer()? });
        }
        if self.vouched.contains(object) {
            return Some(Provenance::Vouched { key: signer()? });
        }
        if self.predating.contains(object) {
            return Some(Provenance::Predates);
        }
        self.why(object).map(Provenance::Unproven)
    }

    /// Every write after the epoch with no signature, as a crash between append and sign leaves.
    pub fn unsigned(&self) -> impl Iterator<Item = (&Hash, &Actor)> {
        self.whose
            .iter()
            .filter(|(name, _)| {
                !self.predating.contains(*name) && !self.signatures.contains_key(*name)
            })
            .map(|(name, (_, actor))| (name, actor))
    }

    /// Each `Snapshot` a counted key signed: what the box held when it began signing.
    #[must_use]
    pub fn epochs(&self) -> &BTreeSet<Hash> {
        &self.epochs
    }

    /// Everything beneath an epoch, where an unsigned object is no finding; nothing moves out.
    #[must_use]
    pub fn predating(&self) -> &BTreeSet<Hash> {
        &self.predating
    }

    #[must_use]
    pub fn predates(&self, object: &Hash) -> bool {
        self.predating.contains(object)
    }
}

/// A signature's prodrome is what it signs'; a legacy object has none.
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

/// The heads name the whole history, since a store holds every ancestor of its heads.
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
