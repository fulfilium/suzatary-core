//! What the server decides when a replica posts objects, with no I/O: what it takes in, what it
//! already holds, and what it refuses and why.
//!
//! Pure, so the sync law is a proptest over generated DAGs; the binary performs the one effect
//! the answer names (the Prodrome's `store::accept` of what it takes) and decides nothing. Nothing is clamped: `at` is the
//! writer's claim, and a future-dated object is stored and reported, never rewritten.
//!
//! The refusals:
//!
//! 1. **Bytes that are not one canonical object.** The name is the hash of the reprint, never a
//!    name the client sent, so two replicas agree on what an object is called.
//! 2. **A parent nobody holds** after this batch, which would leave a store whose `verify` fails;
//!    a `Change` rests on its genesis as well as its deps, so an unheld genesis is one too.
//! 3. **An event naming another actor** than the one the gate resolved
//!    ([`crate::gate::actor_of`]); an eventless `Woven` asserts no fact and is allowed from anyone.
//!    A replica's `Change` carries its event, so it is held to the rule like a `Sealed`.
//! 4. **An object resting on a refused one**, (2) applied transitively: one bad object stops
//!    everything under it, so a refusal is shown to a person rather than retried.
//! 5. **A sealed lifecycle event on a `decision` record**, which settles only through its tap.
//! 6. **Under a root, an object its signatures do not prove** (SPEC law 42): a write no
//!    signature by a key the root registered to its actor proves, whatever kind it is, and a
//!    signature that does not verify or whose key is not trusted for the poster. A key object or
//!    an epoch is the box's to write and is refused from a replica. This is IN ADDITION to (3):
//!    the gate says who is posting, the signature says who wrote the object, to anyone holding it.
//!
//! Idempotence needs no nonce: the same object posted twice is the same hash, answered `already`.
//! What passes is taken in through the Prodrome's own verify-on-receive (`store::accept`), which
//! rehashes and reparses it again; this module decides, it does not replace that check.

use std::collections::{BTreeMap, BTreeSet};

use crate::record::{Envelope, TodoEvent};
use crate::signatures::{Trust, Unproven};
use prodrome_core::dag::Dag;
use prodrome_core::event::{
    Actor, Hash, TodoId, canonical_envelope, parents_of, parse_envelope, seal_hash,
};
use prodrome_core::sign::{PublicKey, Registrar};

/// What the server holds: its objects by name, as `Prodrome::objects` answers them.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    by_name: BTreeMap<Hash, Envelope>,
}

impl Objects {
    /// The store's read, as this module reads it.
    #[must_use]
    pub fn of(objects: &[(Hash, Envelope)]) -> Objects {
        Objects {
            by_name: objects
                .iter()
                .map(|(name, envelope)| (name.clone(), envelope.clone()))
                .collect(),
        }
    }

    /// Does the set hold this name?
    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.by_name.contains_key(name)
    }

    /// How many.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Is it empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Every name, in hash order.
    #[must_use]
    pub fn names(&self) -> Vec<Hash> {
        self.by_name.keys().cloned().collect()
    }

    /// One object.
    #[must_use]
    pub fn get(&self, name: &Hash) -> Option<&Envelope> {
        self.by_name.get(name)
    }

    /// Everything `name` rests on, strictly (parents, and a change's genesis); a name this set
    /// does not hold ends that branch.
    #[must_use]
    pub fn ancestors(&self, name: &Hash) -> BTreeSet<Hash> {
        let mut seen: BTreeSet<Hash> = BTreeSet::new();
        let mut pending: Vec<Hash> = self.by_name.get(name).map(rests_on).unwrap_or_default();
        while let Some(next) = pending.pop() {
            if !seen.insert(next.clone()) {
                continue;
            }
            if let Some(envelope) = self.by_name.get(&next) {
                pending.extend(rests_on(envelope));
            }
        }
        seen
    }

    /// Take one object in, as the law tests apply an [`Outcome`] without a filesystem.
    pub fn insert(&mut self, name: Hash, envelope: Envelope) {
        self.by_name.insert(name, envelope);
    }

    /// Is `todo` a `decision` record? Its kind never changes once authored, so any one
    /// `Authored` answers for its whole life and no fold is needed.
    #[must_use]
    pub fn is_decision(&self, todo: &TodoId) -> bool {
        self.by_name.values().any(|envelope| {
            matches!(
                envelope.event(),
                Some(TodoEvent::Authored(record))
                    if record.todo == *todo && record.payload.is_decision()
            )
        })
    }
}

/// Why one posted literal was not taken: a sum because the client branches on it (a missing
/// parent can be completed, a foreign actor never); [`Reason::said`] is the sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The literal is not one canonical object.
    NotAnObject(String),
    /// It rests on something neither the server nor this post holds.
    MissingParent(Hash),
    /// Its event names an actor who is not the poster.
    ForeignActor {
        /// The actor the event names.
        named: String,
        /// The actor the gate resolved for the login that posted it.
        poster: String,
    },
    /// A lifecycle event on a `decision` record: the box appends its own after performing it,
    /// so one sealed offline would make something look done that was never performed.
    SealedDecision,
    /// Under a root, an object its signatures do not prove, or a signature that proves nothing
    /// for the poster.
    Unproven(Unproven),
    /// A genesis, a key object or an epoch, which only the box writes: a prodrome a replica
    /// began would be one no key is registered in, so its first write could never be proven.
    BoxOnly,
    /// An object of this kind in a store that takes only changes.
    NotAChange(&'static str),
}

impl Reason {
    /// Whether the same write can be taken later: an unsigned or untrusted one waits for its
    /// device to be trusted (a tab caught at the switch, a phone before its card is tapped),
    /// and every other refusal is final.
    #[must_use]
    pub fn kind(&self) -> crate::wire::RefusalKind {
        match self {
            Reason::Unproven(Unproven::Unsigned | Unproven::Untrusted { .. }) => {
                crate::wire::RefusalKind::Trust
            }
            Reason::NotAnObject(_)
            | Reason::MissingParent(_)
            | Reason::ForeignActor { .. }
            | Reason::SealedDecision
            | Reason::Unproven(Unproven::Forged | Unproven::Revoked { .. })
            | Reason::BoxOnly
            | Reason::NotAChange(_) => crate::wire::RefusalKind::Final,
        }
    }

    /// The server's own sentence, shown to the reader verbatim.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Reason::NotAnObject(why) => format!("not one canonical object: {why}"),
            Reason::MissingParent(parent) => {
                let short: String = parent.as_str().chars().take(12).collect();
                format!("rests on {short}…, which this box does not hold")
            }
            Reason::ForeignActor { named, poster } => {
                format!("its event is {named}'s and you are posting as {poster}")
            }
            Reason::SealedDecision => {
                "a decision is settled through its tap, not by a replica".to_owned()
            }
            Reason::Unproven(why) => format!("its signatures do not prove it: {}", why.said()),
            Reason::BoxOnly => {
                "prodromes, keys and epochs are the box's to begin and write".to_owned()
            }
            Reason::NotAChange(kind) => format!("this store takes only changes, not a {kind}"),
        }
    }
}

/// One literal the server would not take, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The name it would have had, when the bytes were an object at all.
    pub name: Option<Hash>,
    /// Which refusal.
    pub reason: Reason,
}

/// What a post means, decided from the objects alone: take in `taken`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The objects to adopt with their canonical prints, parents first, so applying them in
    /// order never holds a child before its parent.
    pub taken: Vec<(Hash, String)>,
    /// The names the server already had: what makes a re-post free, read as settled.
    pub already: Vec<Hash>,
    /// What was refused, in the order the literals arrived.
    pub refused: Vec<Refused>,
}

/// What a post does: the whole decision, with no I/O. What the objects would place
/// (`placed`), held to their signatures under `root`, the chain's root key (SPEC law 42), so
/// a write no key the history trusts for `writer` proves is refused with why.
#[must_use]
pub fn receive(held: &Objects, posted: &[String], writer: &Actor, root: &PublicKey) -> Outcome {
    let placed = placed(held, posted, writer);
    let (taken, unproven) = proven(held, placed.taken, &placed.taking, writer, root);
    Outcome {
        taken,
        already: placed.already,
        refused: placed.refused.into_iter().chain(unproven).collect(),
    }
}

/// What a post's objects would place, read from them alone: each parsed and named by its
/// reprint, an event as another actor and a sealed act on a decision refused, a genesis, a key
/// or a snapshot refused as the box's alone, and the rest taken by fixpoint over "every parent
/// is held or taken", so a child posted before its parent is not refused. `writer` is the
/// gate's actor for this login. Private: nothing it places is proven yet, so only [`receive`]
/// may answer with it.
#[must_use]
fn placed(held: &Objects, posted: &[String], writer: &Actor) -> Placed {
    // Parse first, and by content: the name is the hash of the reprint, so a
    // client cannot name an object anything but what it is.
    let mut candidates: Vec<(Hash, String, Envelope)> = Vec::new();
    let mut refused: Vec<Refused> = Vec::new();
    let mut already: Vec<Hash> = Vec::new();
    for literal in posted {
        match named(literal) {
            Err(reason) => refused.push(Refused { name: None, reason }),
            Ok((name, canonical, envelope)) => {
                // Already-held is answered before the actor rule: the rule stops a client
                // writing as somebody else, and an object the box holds is no write. A replica
                // routinely offers back the box's own `triage` events, and refusing them would
                // leave a permanently stuck branch.
                if held.holds(&name) {
                    already.push(name);
                    continue;
                }
                if let Some(event) = envelope.event()
                    && event.actor() != writer
                {
                    refused.push(Refused {
                        name: Some(name),
                        reason: Reason::ForeignActor {
                            named: event.actor().as_str().to_owned(),
                            poster: writer.as_str().to_owned(),
                        },
                    });
                    continue;
                }
                // A sealed lifecycle act on a `decision` is refused before the fixpoint: a
                // decision settles through the tap that performed it, never through adoption.
                if let Some(
                    event @ (TodoEvent::Completed(_)
                    | TodoEvent::Cancelled(_)
                    | TodoEvent::Reopened(_)
                    | TodoEvent::Tended(_)),
                ) = envelope.event()
                    && held.is_decision(event.todo())
                {
                    refused.push(Refused {
                        name: Some(name),
                        reason: Reason::SealedDecision,
                    });
                    continue;
                }
                if matches!(
                    envelope,
                    Envelope::Genesis(_)
                        | Envelope::KeyAdded(_)
                        | Envelope::KeyRevoked(_)
                        | Envelope::Snapshot(_)
                ) {
                    refused.push(Refused {
                        name: Some(name),
                        reason: Reason::BoxOnly,
                    });
                    continue;
                }
                candidates.push((name, canonical, envelope));
            }
        }
    }

    let Settled {
        taken,
        taking,
        refused: unplaced,
    } = settled(candidates, &|name| held.holds(name));
    refused.extend(unplaced);
    Placed {
        taken,
        taking,
        already,
        refused,
    }
}

/// What a post's objects would place ([`placed`]), before their signatures are read.
#[derive(Debug, Clone, PartialEq, Default)]
struct Placed {
    /// The objects placed, with their canonical prints, parents first.
    taken: Vec<(Hash, String)>,
    /// Each placed object, by name.
    taking: BTreeMap<Hash, Envelope>,
    /// The names the server already had.
    already: Vec<Hash>,
    /// What was refused, in the order the literals arrived.
    refused: Vec<Refused>,
}

/// `candidates` placed by fixpoint over "every parent is held or taken", so a child posted
/// before its parent is not refused: what is taken, parents first, each taken object by name,
/// and a refusal naming the first missing parent of each object left over.
fn settled<E>(
    candidates: Vec<(Hash, String, prodrome_core::event::Envelope<E>)>,
    holds: &dyn Fn(&Hash) -> bool,
) -> Settled<E> {
    let mut taken: Vec<(Hash, String)> = Vec::new();
    let mut taking: BTreeMap<Hash, prodrome_core::event::Envelope<E>> = BTreeMap::new();
    let mut left = candidates;
    loop {
        let mut waiting = Vec::new();
        let mut moved = false;
        for (name, canonical, envelope) in left {
            let ready = rests_on(&envelope)
                .iter()
                .all(|parent| holds(parent) || taking.contains_key(parent));
            if ready {
                // A duplicate inside one post is the same hash twice: the
                // second is nothing, exactly as a re-post is.
                if taking.insert(name.clone(), envelope).is_none() {
                    taken.push((name, canonical));
                }
                moved = true;
            } else {
                waiting.push((name, canonical, envelope));
            }
        }
        left = waiting;
        if !moved || left.is_empty() {
            break;
        }
    }
    let refused = left
        .into_iter()
        .map(|(name, _, envelope)| {
            let missing = rests_on(&envelope)
                .into_iter()
                .find(|parent| !holds(parent) && !taking.contains_key(parent))
                .unwrap_or_else(|| name.clone());
            Refused {
                name: Some(name),
                reason: Reason::MissingParent(missing),
            }
        })
        .collect();
    Settled {
        taken,
        taking,
        refused,
    }
}

/// What [`settled`] placed.
struct Settled<E> {
    taken: Vec<(Hash, String)>,
    taking: BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    refused: Vec<Refused>,
}

/// What a post to a store of schema `E` that the box began and nobody signs means: every
/// object a `Change` whose event is `writer`'s, resting on what is held or posted. The gate
/// proved the request's device for `writer`, and the request's signature covers its body, so
/// no object needs a signature of its own.
#[must_use]
pub fn receive_changes<E: prodrome_core::schema::Schema>(
    held: &BTreeSet<Hash>,
    posted: &[String],
    writer: &Actor,
) -> Outcome
where
    E::Vocabulary: Default,
{
    let vocabulary = E::Vocabulary::default();
    let mut candidates = Vec::new();
    let mut refused: Vec<Refused> = Vec::new();
    let mut already: Vec<Hash> = Vec::new();
    for literal in posted {
        match named_at::<E>(&vocabulary, literal) {
            Err(reason) => refused.push(Refused { name: None, reason }),
            Ok((name, _, _)) if held.contains(&name) => already.push(name),
            Ok((name, canonical, envelope)) => {
                let reason = match &envelope {
                    prodrome_core::event::Envelope::Change(change)
                        if change.event.actor() == writer =>
                    {
                        None
                    }
                    prodrome_core::event::Envelope::Change(change) => Some(Reason::ForeignActor {
                        named: change.event.actor().as_str().to_owned(),
                        poster: writer.as_str().to_owned(),
                    }),
                    prodrome_core::event::Envelope::Genesis(_)
                    | prodrome_core::event::Envelope::KeyAdded(_)
                    | prodrome_core::event::Envelope::KeyRevoked(_)
                    | prodrome_core::event::Envelope::Snapshot(_) => Some(Reason::BoxOnly),
                    other @ (prodrome_core::event::Envelope::Sealed { .. }
                    | prodrome_core::event::Envelope::Woven { .. }
                    | prodrome_core::event::Envelope::Signed(_)) => {
                        Some(Reason::NotAChange(other.name()))
                    }
                };
                match reason {
                    Some(reason) => refused.push(Refused {
                        name: Some(name),
                        reason,
                    }),
                    None => candidates.push((name, canonical, envelope)),
                }
            }
        }
    }
    let Settled {
        taken,
        refused: unplaced,
        ..
    } = settled(candidates, &|name| held.contains(name));
    refused.extend(unplaced);
    Outcome {
        taken,
        already,
        refused,
    }
}

/// `taken` held to its signatures, read with what the box holds (SPEC law 42): a write the
/// history does not prove is refused with why, and so is a signature that proves nothing for
/// `writer`, and then everything resting on something refused. What stays is taken.
fn proven(
    held: &Objects,
    taken: Vec<(Hash, String)>,
    taking: &BTreeMap<Hash, Envelope>,
    writer: &Actor,
    root: &PublicKey,
) -> (Vec<(Hash, String)>, Vec<Refused>) {
    let union: Dag<TodoEvent> = held
        .by_name
        .iter()
        .chain(taking)
        .map(|(name, object)| (name.clone(), object.clone()))
        .collect();
    let trust = Trust::of(
        &union,
        &Registrar::Roots([root.clone()].into_iter().collect()),
    );
    let mut refused: Vec<Refused> = Vec::new();
    let mut kept: Vec<(Hash, String)> = Vec::new();
    for (name, print) in taken {
        let object = &taking[&name];
        let why = match object {
            Envelope::Signed(signed) => {
                let genesis = union
                    .genesis_of(&signed.object)
                    .and_then(|genesis| union.key(&genesis));
                trust.admits(&genesis, writer, signed).err()
            }
            // An object that asserts no actor's fact (a bare merge) is still a write: it grows
            // the chain, so it needs a trusted signature of its own like any other.
            _ if object.event().is_none() => unasserted(&union, &trust, &name, writer),
            _ => trust.why(&name),
        };
        let reason = why.map(Reason::Unproven).or_else(|| {
            rests_on(object)
                .into_iter()
                .find(|parent| refused.iter().any(|one| one.name.as_ref() == Some(parent)))
                .map(Reason::MissingParent)
        });
        match reason {
            Some(reason) => refused.push(Refused {
                name: Some(name),
                reason,
            }),
            None => kept.push((name, print)),
        }
    }
    (kept, refused)
}

/// Why an object carrying no event is not admitted: no signature over it, or none by a key the
/// history trusts for `writer`.
fn unasserted(
    union: &Dag<TodoEvent>,
    trust: &Trust,
    name: &Hash,
    writer: &Actor,
) -> Option<Unproven> {
    let genesis = union
        .genesis_of(name)
        .and_then(|genesis| union.key(&genesis));
    let mut why = Unproven::Unsigned;
    for object in union.objects().values() {
        if let Envelope::Signed(signed) = object
            && signed.object == *name
        {
            match trust.admits(&genesis, writer, signed) {
                Ok(()) => return None,
                Err(refused) => why = refused,
            }
        }
    }
    Some(why)
}

/// What an object cannot be held without: its parents, and the genesis a change names, which a
/// store's verify-on-receive walks to as it walks to a parent.
fn rests_on<E>(envelope: &prodrome_core::event::Envelope<E>) -> Vec<Hash> {
    let mut names = parents_of(envelope);
    names.extend(envelope.genesis().cloned());
    names
}

/// One literal parsed, reprinted and named by the hash of the print, never the client's name.
fn named(literal: &str) -> Result<(Hash, String, Envelope), Reason> {
    named_at(&crate::record::TodoVocabulary::default(), literal)
}

/// [`named`], at the schema `E`.
fn named_at<E: prodrome_core::schema::Schema>(
    vocabulary: &E::Vocabulary,
    literal: &str,
) -> Result<(Hash, String, prodrome_core::event::Envelope<E>), Reason> {
    let envelope =
        parse_envelope::<E>(vocabulary, literal).map_err(|e| Reason::NotAnObject(e.to_string()))?;
    let canonical = canonical_envelope(&envelope);
    if canonical != literal {
        return Err(Reason::NotAnObject(
            "the bytes are not this value's canonical print".to_owned(),
        ));
    }
    Ok((seal_hash(&envelope), canonical, envelope))
}

#[cfg(test)]
mod tests {
    use crate::record::TodoEvent;
    use prodrome_core::event::{
        Hash, mk_cancelled, mk_completed, mk_created, mk_reopened, mk_sealed, mk_woven,
    };
    use prodrome_core::literal::Datetime;
    use prodrome_core::store::{MemoryStore, Replica};

    use super::*;

    fn actor(name: &str) -> Actor {
        Actor::new(name).expect("a roster name")
    }

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day.clamp(1, 28), 12, 0, 0, 0).expect("a real instant")
    }

    fn sealed(prev: Option<&Hash>, event: TodoEvent) -> Envelope {
        mk_sealed(prev.cloned(), event)
    }

    /// A store's read, as `Objects` and as the nodes a fold takes.
    /// The heads of a set, as the Prodrome derives them.
    fn heads(set: &Objects) -> BTreeSet<Hash> {
        prodrome_core::dag::Dag::from_iter(objects_of(set)).tips()
    }

    fn objects_of(set: &Objects) -> Vec<(Hash, Envelope)> {
        let by_name: BTreeMap<Hash, Envelope> = set
            .names()
            .into_iter()
            .map(|name| {
                let envelope = set.get(&name).expect("named").clone();
                (name, envelope)
            })
            .collect();
        prodrome_core::dag::Dag::from_iter(by_name.clone())
            .linearise()
            .expect("a closed set linearises")
            .into_iter()
            .map(|name| {
                let envelope = by_name[&name].clone();
                (name, envelope)
            })
            .collect()
    }

    /// Apply an outcome the way the binary does: take in exactly what it names.
    fn apply(held: &mut Objects, taken: &[(Hash, String)]) {
        for (name, literal) in taken {
            let envelope = parse_envelope(&crate::record::TodoVocabulary::default(), literal)
                .expect("we printed it");
            assert_eq!(&seal_hash(&envelope), name);
            held.insert(name.clone(), envelope);
        }
    }

    /// The rows over everything the set holds.
    fn folded(set: &Objects) -> Vec<crate::record::Entry> {
        let nodes = prodrome_core::dag::Dag::from_iter(objects_of(set))
            .nodes()
            .expect("a closed set reads");
        prodrome_core::view::entries(&nodes, at(28), &prodrome_core::policy::Untrusted::none())
            .expect("a closed set folds")
    }

    /// A little chain of `count` objects for `todo`, genesis first.
    fn branch(from: Option<&Hash>, todo: &str, count: u32) -> Vec<Envelope> {
        let mut out: Vec<Envelope> = Vec::new();
        let mut prev = from.cloned();
        for n in 0..count {
            let event = if n == 0 {
                mk_created(todo, at(n + 1), "ada", "", "").expect("valid")
            } else {
                mk_completed(todo, at(n + 1), "ada", "").expect("valid")
            };
            let object = mk_sealed(prev.clone(), event);
            prev = Some(seal_hash(&object));
            out.push(object);
        }
        out
    }

    #[test]
    fn an_empty_post_takes_nothing_and_leaves_the_heads_alone() {
        let held = Objects::default();
        let outcome = placed(&held, &[], &actor("ada"));
        assert_eq!(outcome, Placed::default());
    }

    /// The name is the print's hash, never the client's claim: bytes that are not one canonical
    /// object are refused rather than filed under whatever they hash to.
    #[test]
    fn bytes_that_are_not_one_canonical_object_are_refused() {
        let held = Objects::default();
        let posted = vec![
            "Sealed(prev='', event=Nonsense())".to_owned(),
            "not a literal at all".to_owned(),
            String::new(),
        ];
        let outcome = placed(&held, &posted, &actor("ada"));
        assert!(outcome.taken.is_empty());
        assert_eq!(outcome.refused.len(), 3);
        for refused in &outcome.refused {
            assert_eq!(refused.name, None);
            assert!(matches!(refused.reason, Reason::NotAnObject(_)));
        }
    }

    /// A non-canonical spelling is a second name for one value, so it is refused.
    #[test]
    fn a_non_canonical_spelling_of_a_real_object_is_refused() {
        let object = sealed(
            None,
            mk_created("alpha", at(1), "ada", "", "").expect("valid"),
        );
        let canonical = canonical_envelope(&object);
        let spaced = format!("{canonical} ");
        let outcome = placed(&Objects::default(), &[spaced], &actor("ada"));
        assert!(outcome.taken.is_empty(), "{outcome:?}");
        assert_eq!(outcome.refused.len(), 1);
    }

    /// The actor rule applies only to a write: re-offering an object the box holds is `already`
    /// whoever's event it carries, since a replica routinely offers back the agent's events.
    #[test]
    fn re_offering_an_object_the_box_already_holds_is_never_a_foreign_actor() {
        let theirs = sealed(
            None,
            mk_created("beta", at(1), "triage", "", "").expect("valid"),
        );
        let mut held = Objects::default();
        held.insert(seal_hash(&theirs), theirs.clone());

        let outcome = placed(&held, &[canonical_envelope(&theirs)], &actor("ada"));
        assert_eq!(outcome.already, vec![seal_hash(&theirs)]);
        assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
        assert!(outcome.taken.is_empty());
    }

    /// The gate says who is posting; an event naming somebody else is that poster writing as them.
    #[test]
    fn an_event_naming_another_actor_is_refused_and_a_merge_is_not() {
        let mine = sealed(
            None,
            mk_created("alpha", at(1), "ada", "", "").expect("valid"),
        );
        let theirs = sealed(
            None,
            mk_created("beta", at(1), "triage", "", "").expect("valid"),
        );
        let posted = vec![canonical_envelope(&mine), canonical_envelope(&theirs)];
        let outcome = placed(&Objects::default(), &posted, &actor("ada"));
        assert_eq!(outcome.taken.len(), 1);
        assert_eq!(outcome.taken[0].0, seal_hash(&mine));
        assert_eq!(
            outcome.refused,
            vec![Refused {
                name: Some(seal_hash(&theirs)),
                reason: Reason::ForeignActor {
                    named: "triage".to_owned(),
                    poster: "ada".to_owned(),
                },
            }]
        );

        // …and a merge, which carries no event, is allowed from any actor:
        // it asserts no fact about a todo, so there is nobody it could name.
        let mut held = Objects::default();
        apply(&mut held, &outcome.taken);
        let other = sealed(
            Some(&seal_hash(&mine)),
            mk_completed("alpha", at(2), "ada", "").expect("valid"),
        );
        let second = sealed(
            Some(&seal_hash(&mine)),
            mk_created("gamma", at(2), "ada", "", "").expect("valid"),
        );
        held.insert(seal_hash(&other), other.clone());
        held.insert(seal_hash(&second), second.clone());
        let woven: Envelope =
            mk_woven(vec![seal_hash(&other), seal_hash(&second)], None).expect("two");
        let outcome = placed(&held, &[canonical_envelope(&woven)], &actor("someone-else"));
        assert_eq!(outcome.taken.len(), 1, "{outcome:?}");
        assert!(outcome.refused.is_empty());
    }

    /// A sealed lifecycle event on a `decision` record is refused by name, while the same event
    /// on an ordinary todo is accepted.
    #[test]
    fn a_sealed_completed_on_a_decision_is_refused_while_an_ordinary_todos_is_accepted() {
        let decision = sealed(
            None,
            crate::record::mk_authored(
                "settlethis",
                at(1),
                "assistant",
                "decision",
                at(1),
                crate::record::MarkupSource::escaped("Send the reply"),
                prodrome_core::fpl::mk_absent(),
                vec!["because he asked".to_owned()],
                "",
                "",
                crate::record::MarkupSource::escaped(
                    "proposal:default/0\n\nthe body a person reads",
                ),
                None,
                Vec::new(),
                Vec::new(),
                "",
            )
            .expect("a valid decision record"),
        );
        let ordinary = sealed(
            None,
            mk_created("alpha", at(1), "ada", "", "").expect("valid"),
        );
        let mut held = Objects::default();
        held.insert(seal_hash(&decision), decision.clone());
        held.insert(seal_hash(&ordinary), ordinary.clone());

        let sealed_decision_completed = sealed(
            Some(&seal_hash(&decision)),
            mk_completed("settlethis", at(2), "ada", "looked done").expect("valid"),
        );
        let sealed_todo_completed = sealed(
            Some(&seal_hash(&ordinary)),
            mk_completed("alpha", at(2), "ada", "").expect("valid"),
        );
        let posted = vec![
            canonical_envelope(&sealed_decision_completed),
            canonical_envelope(&sealed_todo_completed),
        ];
        let outcome = placed(&held, &posted, &actor("ada"));

        assert_eq!(outcome.taken.len(), 1, "{outcome:?}");
        assert_eq!(outcome.taken[0].0, seal_hash(&sealed_todo_completed));
        assert_eq!(
            outcome.refused,
            vec![Refused {
                name: Some(seal_hash(&sealed_decision_completed)),
                reason: Reason::SealedDecision,
            }]
        );
        assert_eq!(
            outcome.refused[0].reason.said(),
            "a decision is settled through its tap, not by a replica"
        );
    }

    /// A parent nobody holds stops that object and everything under it.
    #[test]
    fn an_object_resting_on_nothing_anyone_holds_is_refused_with_its_descendants() {
        let orphaned = branch(None, "alpha", 3);
        // The genesis is withheld, so the second and third rest on a name
        // nobody has.
        let posted: Vec<String> = orphaned[1..].iter().map(canonical_envelope).collect();
        let outcome = placed(&Objects::default(), &posted, &actor("ada"));
        assert!(outcome.taken.is_empty());
        assert_eq!(outcome.refused.len(), 2);
        assert!(
            outcome
                .refused
                .iter()
                .all(|r| matches!(r.reason, Reason::MissingParent(_))),
            "{:?}",
            outcome.refused
        );
    }

    /// A child before its parent is not a refusal: the fixpoint makes the client's order irrelevant.
    #[test]
    fn a_post_in_any_order_takes_the_whole_closure_parents_first() {
        let chain = branch(None, "alpha", 4);
        let mut posted: Vec<String> = chain.iter().map(canonical_envelope).collect();
        posted.reverse();
        let outcome = placed(&Objects::default(), &posted, &actor("ada"));
        assert_eq!(outcome.taken.len(), 4);
        // Parents first, whatever order they arrived in.
        let order: Vec<Hash> = outcome.taken.iter().map(|(name, _)| name.clone()).collect();
        let expected: Vec<Hash> = chain.iter().map(seal_hash).collect();
        assert_eq!(order, expected);
    }

    /// Posting the same object twice is the same hash twice; the hash is the identity, so no nonce.
    #[test]
    fn a_re_post_is_already_held_and_appends_nothing() {
        let chain = branch(None, "alpha", 2);
        let posted: Vec<String> = chain.iter().map(canonical_envelope).collect();
        let mut held = Objects::default();
        let first = placed(&held, &posted, &actor("ada"));
        apply(&mut held, &first.taken);
        let again = placed(&held, &posted, &actor("ada"));
        assert!(again.taken.is_empty());
        assert_eq!(again.already.len(), 2);
        assert_eq!(held.len(), 2);
    }

    /// Two diverged replicas leave two heads, and nothing joins them: the box takes the post in
    /// and writes nothing of its own.
    #[test]
    fn divergence_leaves_both_heads_and_writes_nothing() {
        let shared = branch(None, "alpha", 1);
        let root = seal_hash(&shared[0]);
        let mine = sealed(
            Some(&root),
            mk_completed("alpha", at(2), "ada", "").expect("valid"),
        );
        let theirs = sealed(
            Some(&root),
            mk_created("beta", at(2), "ada", "", "").expect("valid"),
        );
        let mut held = Objects::default();
        let first = placed(
            &held,
            &[canonical_envelope(&shared[0]), canonical_envelope(&mine)],
            &actor("ada"),
        );
        apply(&mut held, &first.taken);
        let outcome = placed(&held, &[canonical_envelope(&theirs)], &actor("ada"));
        assert_eq!(outcome.taken.len(), 1);
        apply(&mut held, &outcome.taken);
        assert_eq!(held.len(), 3, "the post, and nothing the box wrote");
        assert_eq!(
            heads(&held),
            [seal_hash(&mine), seal_hash(&theirs)]
                .into_iter()
                .collect::<BTreeSet<Hash>>()
        );
    }

    /// A tab: the Prodrome's replica in memory, holding `objects`, as the browser's module
    /// holds what it was handed.
    fn replica_of(objects: &[Envelope]) -> MemoryStore<TodoEvent> {
        let mut set = Objects::default();
        for object in objects {
            set.insert(seal_hash(object), object.clone());
        }
        let prints: BTreeMap<Hash, String> = objects
            .iter()
            .map(|object| (seal_hash(object), canonical_envelope(object)))
            .collect();
        let tab = MemoryStore::default();
        tab.receive(heads(&set), &|name| {
            prints.get(name).map(|print| print.clone().into_bytes())
        })
        .expect("a closed set is received");
        tab
    }

    /// What a tab posts for one act: the print of the object its append answers.
    fn wrote(tab: &MemoryStore<TodoEvent>, event: TodoEvent) -> String {
        let name = tab.append(event).expect("the replica appends");
        String::from_utf8(tab.print(&name).expect("an append holds its object")).expect("utf8")
    }

    /// A replica's act is a `Change` over its todo's heads, adopted like any write: held to
    /// the actor rule, and refused when nothing holds the genesis it names.
    #[test]
    fn a_change_a_replica_wrote_is_adopted_and_held_to_the_same_rules() {
        let base = branch(None, "alpha", 2);
        let tab = replica_of(&base);
        let mine = wrote(
            &tab,
            mk_completed("alpha", at(5), "ada", "").expect("valid"),
        );
        let theirs = wrote(
            &tab,
            mk_completed("alpha", at(6), "triage", "").expect("valid"),
        );
        let fresh = wrote(
            &tab,
            mk_created("gamma", at(7), "ada", "", "").expect("valid"),
        );
        assert!(mine.starts_with("Change(genesis="), "{mine}");

        let mut held = Objects::default();
        for object in &base {
            held.insert(seal_hash(object), object.clone());
        }
        let outcome = placed(
            &held,
            &[theirs.clone(), fresh.clone(), mine.clone()],
            &actor("ada"),
        );
        let taken: Vec<&String> = outcome.taken.iter().map(|(_, print)| print).collect();
        assert_eq!(taken, vec![&fresh, &mine], "{outcome:?}");
        assert!(matches!(
            outcome.refused.as_slice(),
            [Refused {
                reason: Reason::ForeignActor { .. },
                ..
            }]
        ));

        // `gamma` had no writes, so its change has no deps: only its genesis says where it lives.
        let outcome = placed(&Objects::default(), &[fresh], &actor("ada"));
        assert!(outcome.taken.is_empty());
        assert_eq!(
            outcome.refused[0].reason,
            Reason::MissingParent(seal_hash(&base[0]))
        );
    }

    // Signatures (SPEC law 42): under a root, a post is held to who wrote each object.

    fn seed(byte: char) -> prodrome_core::sign::Secret {
        prodrome_core::sign::Secret::new(&byte.to_string().repeat(64)).expect("a seed")
    }

    fn print(object: &Envelope) -> (Hash, String) {
        (seal_hash(object), canonical_envelope(object))
    }

    /// `object` and the root's signature of it, as the box writes a key object.
    fn rooted(held: &mut Objects, root: &prodrome_core::sign::Secret, object: Envelope) -> Hash {
        let name = seal_hash(&object);
        held.insert(name.clone(), object);
        let signed = Envelope::Signed(root.sign(&name));
        held.insert(seal_hash(&signed), signed);
        name
    }

    /// A box holding a little history and `phone` registered to ada by the root, and a tab
    /// holding all of it.
    fn signed_box(
        root: &prodrome_core::sign::Secret,
        phone: &prodrome_core::sign::Secret,
    ) -> (Objects, Vec<Envelope>, Hash) {
        let base = branch(None, "alpha", 2);
        let mut held = Objects::default();
        for object in &base {
            held.insert(seal_hash(object), object.clone());
        }
        rooted(
            &mut held,
            root,
            Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
                genesis: seal_hash(&base[0]),
                actor: actor("ada"),
                key: phone.public(),
            }),
        );
        let all = objects_of(&held)
            .into_iter()
            .map(|(_, object)| object)
            .collect();
        (held, all, seal_hash(&base[0]))
    }

    /// The tab's act and its signature by `key`, as the tab posts them.
    fn act(
        tab: &MemoryStore<TodoEvent>,
        key: &prodrome_core::sign::Secret,
        todo: &str,
    ) -> (String, String) {
        let change = wrote(tab, mk_completed(todo, at(9), "ada", "").expect("valid"));
        let name = name_of(&change);
        (
            change,
            canonical_envelope(&Envelope::Signed(key.sign(&name))),
        )
    }

    fn name_of(print: &str) -> Hash {
        let object: Envelope =
            parse_envelope(&crate::record::TodoVocabulary::default(), print).expect("a print");
        seal_hash(&object)
    }

    fn reasons(outcome: &Outcome) -> Vec<Reason> {
        outcome
            .refused
            .iter()
            .map(|one| one.reason.clone())
            .collect()
    }

    #[test]
    fn under_a_root_a_change_its_trusted_key_signed_is_taken_with_its_signature() {
        let (root, phone) = (seed('9'), seed('1'));
        let (held, all, _) = signed_box(&root, &phone);
        let tab = replica_of(&all);
        let (change, signature) = act(&tab, &phone, "alpha");
        let outcome = receive(&held, &[change, signature], &actor("ada"), &root.public());
        assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
        assert_eq!(outcome.taken.len(), 2);
    }

    #[test]
    fn under_a_root_an_unsigned_or_forged_change_is_refused() {
        let (root, phone) = (seed('9'), seed('1'));
        let (held, all, _) = signed_box(&root, &phone);
        let tab = replica_of(&all);
        let (change, _) = act(&tab, &phone, "alpha");
        let outcome = receive(
            &held,
            std::slice::from_ref(&change),
            &actor("ada"),
            &root.public(),
        );
        assert_eq!(
            reasons(&outcome),
            vec![Reason::Unproven(Unproven::Unsigned)]
        );
        // An unsigned write waits for its device, where a forged one never goes.
        assert_eq!(
            Reason::Unproven(Unproven::Unsigned).kind(),
            crate::wire::RefusalKind::Trust
        );
        assert_eq!(
            Reason::Unproven(Unproven::Forged).kind(),
            crate::wire::RefusalKind::Final
        );
        // Before signatures the same post is taken: the law begins with the root.
        assert!(
            placed(&held, std::slice::from_ref(&change), &actor("ada"))
                .refused
                .is_empty()
        );

        let (other, _) = act(&tab, &phone, "beta");
        let forged = prodrome_core::sign::Signed {
            object: name_of(&change),
            ..phone.sign(&name_of(&other))
        };
        let outcome = receive(
            &held,
            &[change, canonical_envelope(&Envelope::Signed(forged))],
            &actor("ada"),
            &root.public(),
        );
        assert!(outcome.taken.is_empty(), "{outcome:?}");
        assert_eq!(
            reasons(&outcome),
            vec![
                Reason::Unproven(Unproven::Forged),
                Reason::Unproven(Unproven::Forged)
            ]
        );
    }

    #[test]
    fn an_untrusted_key_is_refused_until_the_root_registers_it() {
        let (root, phone, stranger) = (seed('9'), seed('1'), seed('2'));
        let (mut held, all, genesis) = signed_box(&root, &phone);
        let tab = replica_of(&all);
        let (change, signature) = act(&tab, &stranger, "alpha");
        let posted = [change, signature];
        let outcome = receive(&held, &posted, &actor("ada"), &root.public());
        assert!(outcome.taken.is_empty());
        assert!(matches!(
            reasons(&outcome).as_slice(),
            [Reason::Unproven(Unproven::Untrusted { key }), ..] if *key == stranger.public()
        ));
        // A key registered by a key that is not the root still does not count.
        rooted(
            &mut held,
            &phone,
            Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
                genesis: genesis.clone(),
                actor: actor("ada"),
                key: stranger.public(),
            }),
        );
        assert!(
            receive(&held, &posted, &actor("ada"), &root.public())
                .taken
                .is_empty()
        );
        // The tap: the root registers it, and the same post is taken.
        rooted(
            &mut held,
            &root,
            Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
                genesis: genesis.clone(),
                actor: actor("ada"),
                key: stranger.public(),
            }),
        );
        let outcome = receive(&held, &posted, &actor("ada"), &root.public());
        assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
        assert_eq!(outcome.taken.len(), 2);
    }

    #[test]
    fn a_revoked_key_is_refused_and_what_it_signed_before_still_reads() {
        let (root, phone) = (seed('9'), seed('1'));
        let (mut held, all, genesis) = signed_box(&root, &phone);
        let tab = replica_of(&all);
        let (before, signature) = act(&tab, &phone, "alpha");
        let first = receive(
            &held,
            &[before.clone(), signature.clone()],
            &actor("ada"),
            &root.public(),
        );
        apply(&mut held, &first.taken);
        let kept = name_of(&signature);
        rooted(
            &mut held,
            &root,
            Envelope::KeyRevoked(
                prodrome_core::sign::mk_key_revoked(
                    genesis.clone(),
                    vec![kept],
                    actor("ada"),
                    phone.public(),
                )
                .expect("a revocation"),
            ),
        );
        let (after, late) = act(&tab, &phone, "beta");
        let outcome = receive(&held, &[after, late], &actor("ada"), &root.public());
        assert!(outcome.taken.is_empty());
        assert!(matches!(
            reasons(&outcome).as_slice(),
            [Reason::Unproven(Unproven::Revoked { .. }), ..]
        ));
        let union: Dag<TodoEvent> = objects_of(&held).into_iter().collect();
        let trust = Trust::of(
            &union,
            &Registrar::Roots([root.public()].into_iter().collect()),
        );
        assert!(
            trust.proves(&name_of(&before)),
            "its earlier objects still read"
        );
    }

    #[test]
    fn under_a_root_a_bare_merge_needs_a_trusted_signature_too() {
        let (root, phone) = (seed('9'), seed('1'));
        let (held, all, _) = signed_box(&root, &phone);
        let heads: Vec<Hash> = all
            .iter()
            .filter(|object| object.event().is_some())
            .take(2)
            .map(seal_hash)
            .collect();
        let merge = mk_woven(heads, None).expect("two parents");
        let (_, text) = print(&merge);
        let outcome = receive(&held, &[text], &actor("ada"), &root.public());
        assert_eq!(
            reasons(&outcome),
            vec![Reason::Unproven(Unproven::Unsigned)],
            "a revoked or new phone cannot grow the chain with merges"
        );
    }

    #[test]
    fn a_replica_may_not_post_a_genesis_a_key_or_an_epoch() {
        let (root, phone) = (seed('9'), seed('1'));
        let (held, _, genesis) = signed_box(&root, &phone);
        let added = Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
            genesis: genesis.clone(),
            actor: actor("ada"),
            key: seed('3').public(),
        });
        let (_, text) = print(&added);
        let outcome = receive(&held, &[text], &actor("ada"), &root.public());
        assert_eq!(reasons(&outcome), vec![Reason::BoxOnly]);
        // Nor a prodrome of its own, which would wedge every enrolment that reads the first.
        let (_, begun) = print(&Envelope::Genesis(
            prodrome_core::genesis::mk_genesis("chain", &"7".repeat(32)).expect("a genesis"),
        ));
        let outcome = receive(&held, &[begun], &actor("ada"), &root.public());
        assert_eq!(reasons(&outcome), vec![Reason::BoxOnly]);
    }

    /// Whatever the post carries, a genesis, a key or an epoch is the box's alone: refused by
    /// the objects alone, before any signature is read, so no unsigned path takes one.
    #[test]
    fn a_genesis_a_key_or_an_epoch_is_refused_from_the_objects_alone() {
        let genesis = prodrome_core::genesis::mk_genesis("chain", &"7".repeat(32)).expect("one");
        let begun = Envelope::Genesis(genesis);
        let named = seal_hash(&begun);
        let mut held = Objects::default();
        held.insert(named.clone(), begun);
        let posted = [
            Envelope::Genesis(
                prodrome_core::genesis::mk_genesis("chain", &"8".repeat(32)).expect("one"),
            ),
            Envelope::KeyAdded(prodrome_core::sign::KeyAdded {
                genesis: named.clone(),
                actor: actor("ada"),
                key: seed('3').public(),
            }),
            Envelope::Snapshot(
                prodrome_core::snapshot::mk_snapshot(named.clone(), vec![named], None)
                    .expect("an epoch"),
            ),
        ];
        let printed: Vec<String> = posted.iter().map(canonical_envelope).collect();
        let placed = placed(&held, &printed, &actor("ada"));
        assert!(placed.taken.is_empty(), "{placed:?}");
        assert_eq!(
            placed
                .refused
                .iter()
                .map(|refused| refused.reason.clone())
                .collect::<Vec<_>>(),
            vec![Reason::BoxOnly; 3]
        );
    }

    // The sync law.

    use proptest::prelude::*;

    /// A generated DAG: each object seals onto one earlier object or onto nothing, a forest of
    /// chains that occasionally branch, which is the divergence sync is about.
    fn a_dag() -> impl Strategy<Value = Vec<Envelope>> {
        (1usize..9, proptest::collection::vec(0usize..9, 0..9)).prop_map(|(count, choices)| {
            let mut built: Vec<Envelope> = Vec::new();
            let mut names: Vec<Hash> = Vec::new();
            for n in 0..count {
                let prev = if n == 0 {
                    None
                } else {
                    choices.get(n).map(|pick| names[pick % names.len()].clone())
                };
                let todo = format!("todo{}", n % 3);
                let event = if n % 2 == 0 {
                    mk_created(&todo, at(u32::try_from(n).unwrap_or(1) + 1), "ada", "", "")
                        .expect("valid")
                } else {
                    mk_completed(&todo, at(u32::try_from(n).unwrap_or(1) + 1), "ada", "")
                        .expect("valid")
                };
                let object = mk_sealed(prev, event);
                names.push(seal_hash(&object));
                built.push(object);
            }
            built
        })
    }

    /// How the same objects are split into batches and shuffled.
    fn a_delivery(size: usize) -> impl Strategy<Value = (Vec<usize>, Vec<usize>)> {
        (
            proptest::collection::vec(0..size.max(1), size),
            proptest::collection::vec(1usize..4, 1..6),
        )
    }

    /// Receive `objects` in the order and batching `delivery` describes, as many
    /// times as it says, and answer with the set that results.
    fn deliver(objects: &[Envelope], order: &[usize], batches: &[usize]) -> Objects {
        // A permutation-ish order: every object exactly once, visited through
        // the generated indices and then swept for whatever was missed.
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut sequence: Vec<usize> = Vec::new();
        for pick in order.iter().copied().chain(0..objects.len()) {
            let index = pick % objects.len().max(1);
            if index < objects.len() && seen.insert(index) {
                sequence.push(index);
            }
        }
        let mut held = Objects::default();
        let writer = actor("ada");
        let mut cut = 0;
        let mut sizes = batches.iter().cycle();
        while cut < sequence.len() {
            let size = (*sizes.next().unwrap_or(&1)).min(sequence.len() - cut);
            let posted: Vec<String> = sequence[cut..cut + size]
                .iter()
                .map(|index| canonical_envelope(&objects[*index]))
                .collect();
            // Twice, every time: a replay must be free.
            let outcome = placed(&held, &posted, &writer);
            apply(&mut held, &outcome.taken);
            let replay = placed(&held, &posted, &writer);
            apply(&mut held, &replay.taken);
            cut += size;
        }
        // The exchange repeats until it takes nothing, as a replica does: convergence is the
        // fixpoint, not the first round trip.
        let all: Vec<String> = objects.iter().map(canonical_envelope).collect();
        loop {
            let outcome = placed(&held, &all, &writer);
            if outcome.taken.is_empty() {
                break;
            }
            apply(&mut held, &outcome.taken);
        }
        held
    }

    /// A tab's acts: which todo, which lifecycle move, which day.
    fn acts() -> impl Strategy<Value = Vec<(usize, usize, u32)>> {
        proptest::collection::vec((0usize..3, 0usize..3, 10u32..20), 1..5)
    }

    fn act_of((todo, kind, day): (usize, usize, u32)) -> TodoEvent {
        let todo = ["alpha", "beta", "gamma"][todo];
        match kind {
            0 => mk_completed(todo, at(day), "ada", ""),
            1 => mk_cancelled(todo, at(day), "ada", ""),
            _ => mk_reopened(todo, at(day), "ada", ""),
        }
        .expect("valid")
    }

    /// The box holding `base`, after each post in turn, joined as it joins.
    fn boxed(base: &[Envelope], posts: &[&[String]]) -> Objects {
        let mut held = Objects::default();
        for object in base {
            held.insert(seal_hash(object), object.clone());
        }
        for posted in posts {
            let outcome = placed(&held, posted, &actor("ada"));
            assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
            apply(&mut held, &outcome.taken);
        }
        held
    }

    proptest! {
        /// Sync is set union: the same objects in any order, batching or repetition yield the same
        /// received set and the same fold.
        ///
        /// The box writes nothing of its own, so both sides hold exactly the objects posted.
        #[test]
        fn receiving_in_any_order_or_batching_yields_one_store(
            objects in a_dag(),
            (order, batches) in a_delivery(9),
            (other_order, other_batches) in a_delivery(9),
        ) {
            let one = deliver(&objects, &order, &batches);
            let two = deliver(&objects, &other_order, &other_batches);
            // Every object posted is held: every parent is in the same set.
            for object in &objects {
                prop_assert!(one.holds(&seal_hash(object)));
                prop_assert!(two.holds(&seal_hash(object)));
            }
            // …and nothing else is, either side.
            prop_assert_eq!(one.names(), two.names());
            prop_assert_eq!(one.len(), objects.len());
            prop_assert_eq!(folded(&one), folded(&two));
        }

        /// Two tabs that wrote changes apart, each over its own fold, reach the box in either
        /// order and leave it holding the same objects, folding the same rows, at the same heads.
        #[test]
        fn two_tabs_writing_changes_apart_converge_whichever_reaches_the_box_first(
            shared in 1u32..4,
            left in acts(),
            right in acts(),
        ) {
            let base = branch(None, "alpha", shared);
            let (one_tab, other_tab) = (replica_of(&base), replica_of(&base));
            let mine: Vec<String> = left.into_iter().map(|act| wrote(&one_tab, act_of(act))).collect();
            let theirs: Vec<String> =
                right.into_iter().map(|act| wrote(&other_tab, act_of(act))).collect();

            let one = boxed(&base, &[&mine, &theirs]);
            let two = boxed(&base, &[&theirs, &mine]);
            prop_assert_eq!(one.names(), two.names());
            prop_assert_eq!(folded(&one), folded(&two));
            prop_assert_eq!(heads(&one), heads(&two));
        }

        /// A box that took two diverged logs in IS their union, object for object, so it folds
        /// as the union, every field of every row, conflicts included.
        #[test]
        fn two_diverged_logs_received_are_their_union(
            shared in a_dag(),
            mine in 1u32..4,
            theirs in 1u32..4,
        ) {
            let base: Objects = {
                let mut set = Objects::default();
                for object in &shared { set.insert(seal_hash(object), object.clone()); }
                set
            };
            let tip = heads(&base).iter().next().cloned();
            // Two branches off one head, each a little chain of its own.
            let left = branch(tip.as_ref(), "left", mine);
            let right = branch(tip.as_ref(), "right", theirs);

            // The box: the shared prefix, then one branch, then the other.
            let mut box_side = base.clone();
            for object in &left { box_side.insert(seal_hash(object), object.clone()); }
            let posted: Vec<String> = right.iter().map(canonical_envelope).collect();
            let outcome = placed(&box_side, &posted, &actor("ada"));
            prop_assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
            apply(&mut box_side, &outcome.taken);

            let mut union = base;
            for object in left.iter().chain(right.iter()) {
                union.insert(seal_hash(object), object.clone());
            }
            prop_assert_eq!(box_side.names(), union.names());
            prop_assert_eq!(folded(&box_side), folded(&union));
        }
    }
}
