//! What the server decides when a replica posts objects, with no I/O: what it takes, already
//! holds and refuses, and why. `at` is the writer's claim and is never clamped. A name is the
//! hash of the reprint, so the same object posted twice is answered `already`. The gate says who
//! posts; under a root, the signature says who wrote it (SPEC law 42).

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::dag::Dag;
use prodrome_core::event::{
    Actor, Hash, TodoId, canonical_envelope, parents_of, parse_envelope, seal_hash,
};
use prodrome_core::policy::Untrusted;
use prodrome_core::schema::Schema as _;
use prodrome_core::sign::{PublicKey, Registrar};
use suzatary_lines::parts::part;
use suzatary_record::record::{Envelope, TodoEvent};
use suzatary_signatures::signatures::{Trust, Trusting, Unproven};
use suzatary_store::admission::Admission;

/// What the server holds: its objects by name, as `Prodrome::objects` answers them.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    by_name: BTreeMap<Hash, Envelope>,
}

impl Objects {
    /// The store's read.
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

    /// Take one object in.
    pub fn insert(&mut self, name: Hash, envelope: Envelope) {
        self.by_name.insert(name, envelope);
    }

    /// Is `todo` a `decision` record? Its kind never changes, so any `Authored` answers.
    #[must_use]
    pub fn is_decision(&self, todo: &TodoId) -> bool {
        self.by_name
            .values()
            .any(|envelope| decision(envelope) == Some(todo))
    }
}

/// The todo a `decision` record is of; none for any other object.
fn decision(envelope: &Envelope) -> Option<&TodoId> {
    match envelope.event()? {
        TodoEvent::Authored(record) => record.payload.is_decision().then_some(&record.todo),
        TodoEvent::Created(_)
        | TodoEvent::Completed(_)
        | TodoEvent::Cancelled(_)
        | TodoEvent::Reopened(_)
        | TodoEvent::Tended(_)
        | TodoEvent::SpecRevised(_) => None,
    }
}

/// Why one posted literal was not taken: a sum because the client branches on it.
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
        /// The gate's actor for the poster.
        poster: String,
    },
    /// A lifecycle event on a `decision` record, which settles only through its tap.
    SealedDecision,
    /// Under a root, an object its signatures do not prove.
    Unproven(Unproven),
    /// A genesis, a key object or an epoch, which only the box writes.
    BoxOnly,
    /// An object of this kind in a store that takes only changes.
    NotAChange(&'static str),
    /// A signature posted without the change it signs.
    Unaccompanied,
    /// It rests on an object neither held nor posted: taken once a later post brings it.
    NotYetHeld(Hash),
    /// Its store's own rule refuses it ([`suzatary_store::admission::Admission`]), on the rule's
    /// grounds and in its words.
    Inadmissible(suzatary_store::admission::Inadmissible),
    /// The store's own check ([`Dag::verify`]) would report it once held, each finding in the
    /// check's sentence: an object is never taken back, so the box takes none.
    Reported(Vec<String>),
    /// It rests on an object refused in this post, and is refused as that object is.
    RestsOn {
        /// The refused object.
        parent: Hash,
        /// Its refusal's kind.
        kind: suzatary_signatures::wire::signatures::RefusalKind,
    },
}

impl Reason {
    /// Whether the write may be taken later, so it is posted again: one unsigned, untrusted or
    /// resting on an unheld object waits, one resting on a refused object is as it is, the rest
    /// are final.
    #[must_use]
    pub fn kind(&self) -> suzatary_signatures::wire::signatures::RefusalKind {
        match self {
            Reason::Unproven(Unproven::Unsigned | Unproven::Untrusted { .. }) => {
                suzatary_signatures::wire::signatures::RefusalKind::Trust
            }
            Reason::NotYetHeld(_) => suzatary_signatures::wire::signatures::RefusalKind::Later,
            Reason::RestsOn { kind, .. } => *kind,
            Reason::NotAnObject(_)
            | Reason::MissingParent(_)
            | Reason::ForeignActor { .. }
            | Reason::SealedDecision
            | Reason::Unproven(Unproven::Forged | Unproven::Revoked { .. })
            | Reason::BoxOnly
            | Reason::NotAChange(_)
            | Reason::Unaccompanied
            | Reason::Reported(_) => suzatary_signatures::wire::signatures::RefusalKind::Final,
            Reason::Inadmissible(why) => match why.grounds {
                suzatary_store::admission::Ruling::Final => {
                    suzatary_signatures::wire::signatures::RefusalKind::Final
                }
                suzatary_store::admission::Ruling::Outdated => {
                    suzatary_signatures::wire::signatures::RefusalKind::Outdated
                }
                // Posted again later, and said in the rule's words; never recorded.
                suzatary_store::admission::Ruling::Later => {
                    suzatary_signatures::wire::signatures::RefusalKind::Later
                }
            },
        }
    }

    /// The server's own sentence, shown to the reader verbatim.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Reason::NotAnObject(why) => format!("not one canonical object: {why}"),
            Reason::MissingParent(parent) => {
                format!("rests on {}…, which this box does not hold", short(parent))
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
            Reason::Unaccompanied => {
                "a signature is taken only with the change it signs".to_owned()
            }
            Reason::Inadmissible(why) => why.reason.clone(),
            Reason::Reported(findings) => format!(
                "the store's check would report it for ever once held: {}",
                findings.join("; ")
            ),
            Reason::NotYetHeld(parent) => format!(
                "rests on {}…, which this box does not hold yet",
                short(parent)
            ),
            Reason::RestsOn { parent, kind } => {
                use suzatary_signatures::wire::signatures::RefusalKind;
                let parent = short(parent);
                match kind {
                    RefusalKind::Final | RefusalKind::Outdated => {
                        format!("rests on {parent}…, which this box refused")
                    }
                    RefusalKind::Later => format!(
                        "rests on {parent}…, which this box has not taken yet, and waits with it"
                    ),
                    RefusalKind::Trust => format!(
                        "rests on {parent}…, which waits for its signer to be trusted, and waits with it"
                    ),
                }
            }
        }
    }
}

fn short(name: &Hash) -> String {
    name.as_str().chars().take(12).collect()
}

/// One literal the server would not take, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// `None` where the bytes were no object.
    pub name: Option<Hash>,
    /// Which refusal.
    pub reason: Reason,
}

/// What a post means, decided from the objects alone: take in `taken`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The objects to adopt with their canonical prints, parents first.
    pub taken: Vec<(Hash, String)>,
    /// The names the server already had.
    pub already: Vec<Hash>,
    /// What was refused, in the order the literals arrived.
    pub refused: Vec<Refused>,
}

/// What a post does, with no I/O: what the objects place, held to their signatures under the
/// chain's `root`, and refused where the chain's check under its `policy` would report them
/// ([`Dag::verify`]).
#[must_use]
pub fn receive(
    held: &Objects,
    posted: &[String],
    writer: &Actor,
    root: &PublicKey,
    policy: &Untrusted,
) -> Outcome {
    let placed = part("placed", || placed(held, posted, writer));
    let (taken, unproven) = part("proven", || {
        proven(held, placed.taken, &placed.taking, writer, root, policy)
    });
    Outcome {
        taken,
        already: placed.already,
        refused: placed.refused.into_iter().chain(unproven).collect(),
    }
}

/// What a post's objects would place; nothing it places is proven yet, so only [`receive`]
/// answers with it.
#[must_use]
fn placed(held: &Objects, posted: &[String], writer: &Actor) -> Placed {
    let mut candidates: Vec<(Hash, String, Envelope)> = Vec::new();
    let mut refused: Vec<Refused> = Vec::new();
    let mut already: Vec<Hash> = Vec::new();
    let parsed: Vec<_> = posted.iter().map(|literal| named(literal)).collect();
    // A record posted beside its lifecycle event seals it as a held one does.
    let decisions: BTreeSet<TodoId> = parsed
        .iter()
        .flatten()
        .filter_map(|(_, _, envelope)| decision(envelope).cloned())
        .collect();
    for literal in parsed {
        match literal {
            Err(reason) => refused.push(Refused { name: None, reason }),
            Ok((name, canonical, envelope)) => {
                // Already-held comes before the actor rule: a replica offers back the box's own
                // `triage` events, and refusing them would leave a stuck branch.
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
                if let Some(
                    event @ (TodoEvent::Completed(_)
                    | TodoEvent::Cancelled(_)
                    | TodoEvent::Reopened(_)
                    | TodoEvent::Tended(_)),
                ) = envelope.event()
                    && (held.is_decision(event.todo()) || decisions.contains(event.todo()))
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
    taken: Vec<(Hash, String)>,
    taking: BTreeMap<Hash, Envelope>,
    /// The names the server already had.
    already: Vec<Hash>,
    /// What was refused, in the order the literals arrived.
    refused: Vec<Refused>,
}

/// `candidates` placed by fixpoint, parents first; each left over refused at its first missing parent.
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

/// What a store holds, as a post to it is read: every object by name, and what its rule reads
/// beyond it.
pub struct Holding<'a, A: Admission> {
    /// Every object, by name: the history a posted change joins.
    pub history: &'a Dag<A::Schema>,
    /// What the rule reads beyond its store.
    pub world: &'a A::World,
}

impl<A: Admission> Clone for Holding<'_, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: Admission> Copy for Holding<'_, A> {}

/// What a post to a store a tab writes means: its changes one at a time, as if each were posted
/// alone, in one canonical order (parents first, ties by name). A `Change` as `writer` is taken
/// only beside a posted `Signed` of it whose key every chain trusts for `writer`
/// ([`Trusting::admits`]), only where its parents are held or taken before it, only where its
/// store's own rule takes it ([`Admission`]) against what is held with every change taken before
/// it, and only where the store's check would report nothing of it once held ([`Dag::verify`]);
/// a taken change is taken in before the next is judged. So a post takes exactly what its
/// changes posted one by one in that order take, and every invariant the rule keeps for one
/// change holds for any post. The check is asked once for what the post takes; a change it
/// reports is refused, and the post is judged again, until the check reports none.
#[must_use]
pub fn receive_changes<A: Admission>(
    holding: Holding<'_, A>,
    posted: &[String],
    beside: &A::Beside,
    writer: &Actor,
    trusting: &Trusting,
) -> Received
where
    <A::Schema as prodrome_core::schema::Schema>::Vocabulary: Default,
{
    let Holding { history, world } = holding;
    let read = read_post::<A::Schema>(history.objects(), posted);
    let door = Door::<A> {
        history,
        world,
        signers: admitted(&read.parsed, writer, trusting),
        read: &read,
        beside,
        writer,
        trusting,
    };
    let mut reported = BTreeMap::new();
    loop {
        let judged = door.judge(&reported);
        let taking = judged
            .taking
            .iter()
            .map(|name| (name, &read.parsed[name]))
            .collect();
        // Every store a tab writes opens with no untrusted actor (`suzatary_io::objects`).
        let found = part("found", || found(history, &taking, &Untrusted::none()));
        if found.is_empty() {
            return door.received(judged);
        }
        reported.extend(found);
    }
}

/// A post to a store a tab writes, read once, and what judges it.
struct Door<'a, A: Admission> {
    history: &'a Dag<A::Schema>,
    world: &'a A::World,
    read: &'a Read<A::Schema>,
    /// The keys of each posted change's signatures the trust admits ([`admitted`]).
    signers: BTreeMap<&'a Hash, Vec<prodrome_core::sign::PublicKey>>,
    beside: &'a A::Beside,
    writer: &'a Actor,
    trusting: &'a Trusting,
}

/// A post judged: what it takes, its changes by name, and what it refuses with each kind.
struct Judged {
    taken: Vec<(Hash, String)>,
    taking: BTreeSet<Hash>,
    refused: Vec<Refused>,
    kinds: BTreeMap<Hash, suzatary_signatures::wire::signatures::RefusalKind>,
}

impl<A: Admission> Door<'_, A> {
    /// The post judged one change at a time, each of `reported` refused in the check's words.
    fn judge(&self, reported: &BTreeMap<Hash, Vec<String>>) -> Judged {
        use prodrome_core::event::Envelope as E;
        let (parsed, writer, trusting) = (&self.read.parsed, self.writer, self.trusting);
        let names = self.history.objects();
        let mut judged = Judged {
            taken: Vec::new(),
            taking: BTreeSet::new(),
            refused: self.read.refused.clone(),
            kinds: BTreeMap::new(),
        };
        let mut changes: Vec<&Hash> = Vec::new();
        for (name, envelope) in parsed {
            let reason = match envelope {
                E::Change(change) if change.event.actor() == writer => {
                    changes.push(name);
                    None
                }
                E::Change(change) => Some(Reason::ForeignActor {
                    named: change.event.actor().as_str().to_owned(),
                    poster: writer.as_str().to_owned(),
                }),
                E::Signed(_) => None,
                E::Genesis(_) | E::KeyAdded(_) | E::KeyRevoked(_) | E::Snapshot(_) => {
                    Some(Reason::BoxOnly)
                }
                other @ (E::Sealed { .. } | E::Woven { .. }) => {
                    Some(Reason::NotAChange(other.name()))
                }
            };
            if let Some(reason) = reason {
                refuse(&mut judged.refused, &mut judged.kinds, name, reason);
            }
        }
        let mut ruled = part("held", || A::held(self.history));
        for name in canonical(&changes, parsed) {
            let E::Change(change) = &parsed[name] else {
                continue;
            };
            let parents = rests_on(&parsed[name]);
            let why = resting(parents.clone(), &judged.kinds)
                .or_else(|| {
                    parents
                        .iter()
                        .find(|parent| {
                            !names.contains_key(*parent) && !judged.taking.contains(*parent)
                        })
                        .map(|parent| Reason::NotYetHeld(parent.clone()))
                })
                .or_else(|| proof(name, parsed, writer, trusting).map(Reason::Unproven))
                .or_else(|| reported.get(name).cloned().map(Reason::Reported))
                .or_else(|| {
                    let post = suzatary_store::admission::Post::<A> {
                        name,
                        beside: self.beside,
                        held: &ruled,
                        world: self.world,
                        signers: self.signers.get(name).map_or(&[], Vec::as_slice),
                    };
                    part("rule", || A::refuses(change, &post)).map(Reason::Inadmissible)
                });
            if let Some(reason) = why {
                refuse(&mut judged.refused, &mut judged.kinds, name, reason);
            } else {
                part("take", || A::take(&mut ruled, name, change));
                judged.taking.insert(name.clone());
                judged
                    .taken
                    .push((name.clone(), self.read.prints[name].clone()));
                // Each admitted signature of it goes with it.
                for (signature, envelope) in parsed {
                    if let E::Signed(signed) = envelope
                        && signed.object == *name
                        && trusting.admits(writer, signed).is_ok()
                    {
                        judged
                            .taken
                            .push((signature.clone(), self.read.prints[signature].clone()));
                    }
                }
            }
        }
        judged
    }

    /// `judged` answered: each signature not taken with its change refused, and the refusals to
    /// record.
    fn received(&self, judged: Judged) -> Received {
        let Judged {
            taken,
            taking,
            mut refused,
            mut kinds,
        } = judged;
        unaccompanied(
            &self.read.parsed,
            &taken,
            &taking,
            self.history.objects(),
            self.writer,
            self.trusting,
            &mut refused,
            &mut kinds,
        );
        let recorded = recorded(&self.read.parsed, &refused, self.trusting);
        Received {
            outcome: Outcome {
                taken,
                already: self.read.already.clone(),
                refused,
            },
            recorded,
        }
    }
}

/// Each signature of `parsed` not taken with its change, refused: of a taken change it proves
/// nothing; of a held one it came without what it signs; of a refused one it rests on that; else
/// it waits for it.
#[expect(
    clippy::too_many_arguments,
    reason = "the fold's state, read and extended once"
)]
fn unaccompanied<E: prodrome_core::schema::Schema>(
    parsed: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    taken: &[(Hash, String)],
    taking: &BTreeSet<Hash>,
    names: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    writer: &Actor,
    trusting: &Trusting,
    refused: &mut Vec<Refused>,
    kinds: &mut BTreeMap<Hash, suzatary_signatures::wire::signatures::RefusalKind>,
) {
    use prodrome_core::event::Envelope as E;
    for (name, envelope) in parsed {
        if let E::Signed(signed) = envelope
            && !taken.iter().any(|(one, _)| one == name)
        {
            let reason = if taking.contains(&signed.object) {
                trusting.admits(writer, signed).err().map(Reason::Unproven)
            } else if names.contains_key(&signed.object) {
                Some(Reason::Unaccompanied)
            } else {
                resting(vec![signed.object.clone()], kinds)
                    .or_else(|| Some(Reason::NotYetHeld(signed.object.clone())))
            };
            if let Some(reason) = reason {
                refuse(refused, kinds, name, reason);
            }
        }
    }
}

/// A post's literals read: each object new to the store by name, beside its print; each the store
/// holds already; and each that is no object, refused.
struct Read<E> {
    parsed: BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    prints: BTreeMap<Hash, String>,
    refused: Vec<Refused>,
    already: Vec<Hash>,
}

/// `posted` read at the schema `E`, against the store's objects by name.
fn read_post<E: prodrome_core::schema::Schema>(
    names: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    posted: &[String],
) -> Read<E>
where
    E::Vocabulary: Default,
{
    let vocabulary = E::Vocabulary::default();
    let mut read = Read {
        parsed: BTreeMap::new(),
        prints: BTreeMap::new(),
        refused: Vec::new(),
        already: Vec::new(),
    };
    for literal in posted {
        match named_at::<E>(&vocabulary, literal) {
            Err(reason) => read.refused.push(Refused { name: None, reason }),
            Ok((name, _, _)) if names.contains_key(&name) => read.already.push(name),
            Ok((name, canonical, envelope)) => {
                read.prints.insert(name.clone(), canonical);
                read.parsed.insert(name, envelope);
            }
        }
    }
    read
}

/// The keys of each posted object's signatures the trust admits for `writer`: only such a
/// signature proves its key, and so its seat; one merely posted beside the line, naming a key it
/// was not made with, proves nothing.
fn admitted<'p, E>(
    parsed: &'p BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    writer: &Actor,
    trusting: &Trusting,
) -> BTreeMap<&'p Hash, Vec<prodrome_core::sign::PublicKey>> {
    let mut signers: BTreeMap<&Hash, Vec<prodrome_core::sign::PublicKey>> = BTreeMap::new();
    for envelope in parsed.values() {
        if let prodrome_core::event::Envelope::Signed(signed) = envelope
            && trusting.admits(writer, signed).is_ok()
        {
            signers
                .entry(&signed.object)
                .or_default()
                .push(signed.key.clone());
        }
    }
    signers
}

/// `name` refused for `reason`, its kind kept for what rests on it.
fn refuse(
    refused: &mut Vec<Refused>,
    kinds: &mut BTreeMap<Hash, suzatary_signatures::wire::signatures::RefusalKind>,
    name: &Hash,
    reason: Reason,
) {
    kinds.insert(name.clone(), reason.kind());
    refused.push(Refused {
        name: Some(name.clone()),
        reason,
    });
}

/// `changes` in the order a post is judged in: each after every parent it has in the post, and
/// of those ready together the least name first.
fn canonical<'n, E>(
    changes: &[&'n Hash],
    parsed: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
) -> Vec<&'n Hash> {
    let posted: BTreeSet<&Hash> = changes.iter().copied().collect();
    let mut waits: BTreeMap<&Hash, usize> = BTreeMap::new();
    let mut children: BTreeMap<Hash, Vec<&'n Hash>> = BTreeMap::new();
    for &name in changes {
        let parents: BTreeSet<Hash> = rests_on(&parsed[name])
            .into_iter()
            .filter(|parent| posted.contains(parent))
            .collect();
        waits.insert(name, parents.len());
        for parent in parents {
            children.entry(parent).or_default().push(name);
        }
    }
    let mut ready: BTreeSet<&'n Hash> = changes
        .iter()
        .copied()
        .filter(|name| waits[name] == 0)
        .collect();
    let mut order = Vec::with_capacity(changes.len());
    while let Some(name) = ready.pop_first() {
        order.push(name);
        for &child in children.get(name).map_or(&[][..], Vec::as_slice) {
            let left = waits.get_mut(child).expect("a posted change");
            *left -= 1;
            if *left == 0 {
                ready.insert(child);
            }
        }
    }
    order
}

/// A post to a store a tab writes, and the refusals to record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Received {
    /// What to take, and the answer.
    pub outcome: Outcome,
    /// Each refusal for good (final or outdated) a trusted key's posted signature proves, so an untrusted poster
    /// cannot fill the asks.
    pub recorded: Vec<(Hash, suzatary_store::receipt::Grounds, Reason)>,
}

/// Of `parents`, the refused one an object is refused with: a final refusal first, then an
/// outdated one, then one awaiting trust.
fn resting(
    parents: Vec<Hash>,
    kinds: &BTreeMap<Hash, suzatary_signatures::wire::signatures::RefusalKind>,
) -> Option<Reason> {
    let mut found: Vec<(Hash, suzatary_signatures::wire::signatures::RefusalKind)> = parents
        .into_iter()
        .filter_map(|parent| kinds.get(&parent).map(|kind| (parent, *kind)))
        .collect();
    found.sort_by_key(|(_, kind)| match kind {
        suzatary_signatures::wire::signatures::RefusalKind::Final => 0,
        suzatary_signatures::wire::signatures::RefusalKind::Outdated => 1,
        suzatary_signatures::wire::signatures::RefusalKind::Later => 2,
        suzatary_signatures::wire::signatures::RefusalKind::Trust => 3,
    });
    found
        .into_iter()
        .next()
        .map(|(parent, kind)| Reason::RestsOn { parent, kind })
}

/// Why no signature of `name` among `signed` proves it for `actor`; a revocation wins.
fn proof<E>(
    name: &Hash,
    signed: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    actor: &Actor,
    trusting: &Trusting,
) -> Option<Unproven> {
    let mut why = Unproven::Unsigned;
    for object in signed.values() {
        if let prodrome_core::event::Envelope::Signed(signature) = object
            && signature.object == *name
        {
            match trusting.admits(actor, signature) {
                Ok(()) => return None,
                Err(revoked @ Unproven::Revoked { .. }) => return Some(revoked),
                Err(other) => why = other,
            }
        }
    }
    Some(why)
}

/// Of `refused`, each change refused for good that a posted signature proves for its actor,
/// once, however many times the post names it.
fn recorded<E: prodrome_core::schema::Schema>(
    parsed: &BTreeMap<Hash, prodrome_core::event::Envelope<E>>,
    refused: &[Refused],
    trusting: &Trusting,
) -> Vec<(Hash, suzatary_store::receipt::Grounds, Reason)> {
    use suzatary_signatures::wire::signatures::RefusalKind;
    use suzatary_store::receipt::Grounds;
    let once: BTreeMap<&Hash, (Grounds, &Reason)> = refused
        .iter()
        .filter_map(|one| {
            let grounds = match one.reason.kind() {
                RefusalKind::Final => Grounds::Final,
                RefusalKind::Outdated => Grounds::Outdated,
                RefusalKind::Trust | RefusalKind::Later => return None,
            };
            Some((one.name.as_ref()?, (grounds, &one.reason)))
        })
        .collect();
    once.into_iter()
        .filter_map(|(name, (grounds, reason))| {
            let event = parsed.get(name)?.event()?;
            proof(name, parsed, event.actor(), trusting)
                .is_none()
                .then(|| (name.clone(), grounds, reason.clone()))
        })
        .collect()
}

/// `taken` held to its signatures (SPEC law 42), then everything resting on something refused.
fn proven(
    held: &Objects,
    taken: Vec<(Hash, String)>,
    taking: &BTreeMap<Hash, Envelope>,
    writer: &Actor,
    root: &PublicKey,
    policy: &Untrusted,
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
    let found = part("found", || found(&union, &taking.iter().collect(), policy));
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
            // A bare merge asserts no fact but grows the chain, so it needs a signature too.
            _ if object.event().is_none() => unasserted(&union, &trust, &name, writer),
            _ => trust.why(&name),
        };
        let reason = why
            .map(Reason::Unproven)
            .or_else(|| {
                rests_on(object)
                    .into_iter()
                    .find(|parent| refused.iter().any(|one| one.name.as_ref() == Some(parent)))
                    .map(Reason::MissingParent)
            })
            .or_else(|| found.get(&name).cloned().map(Reason::Reported));
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

/// What the store's check finds of each object of `taken` once it holds them all, by name, in
/// the check's sentences: each finding [`Dag::verify`] makes under the store's `policy` that names
/// one of `taken` ([`about`]). Such a finding is a function of the object it names and what that
/// rests on, so the check is asked once, of `taken` and the closure of what they rest on in
/// `history`, never of the whole store; a finding the store already holds names an object it
/// holds, never one of `taken`.
fn found<E: prodrome_core::schema::Schema>(
    history: &Dag<E>,
    taken: &BTreeMap<&Hash, &prodrome_core::event::Envelope<E>>,
    policy: &Untrusted,
) -> BTreeMap<Hash, Vec<String>> {
    if taken.is_empty() {
        return BTreeMap::new();
    }
    let beneath = history.closure(
        taken
            .values()
            .flat_map(|object| parents_of(object))
            .filter(|parent| !taken.contains_key(parent)),
    );
    let mut closed: Dag<E> = beneath
        .into_iter()
        .filter_map(|name| Some((name.clone(), history.get(&name)?.clone())))
        .collect();
    closed.extend(
        taken
            .iter()
            .map(|(name, object)| ((*name).clone(), (*object).clone())),
    );
    let mut found: BTreeMap<Hash, Vec<String>> = BTreeMap::new();
    for finding in suzatary_signatures::signatures::findings_of(&closed, policy) {
        if let Some(name) = about(&finding).filter(|name| taken.contains_key(name)) {
            found
                .entry(name.clone())
                .or_default()
                .push(finding.to_string());
        }
    }
    found
}

/// The one object `finding` refuses at a door, where there is one. A signature's findings are the
/// proof's, which judges every posted change against the post's signatures and the store's keys
/// before the check, and which the closure of what a change rests on does not hold.
fn about(finding: &prodrome_core::dag::Finding) -> Option<&Hash> {
    use prodrome_core::dag::Finding as F;
    match finding {
        F::Foreign { change, .. } | F::Redundant { change, .. } => Some(change),
        F::Stranger { object, .. } | F::Crossing { object, .. } => Some(object),
        F::Dated { name, .. } | F::Unread { name, .. } => Some(name),
        F::Below { write, .. } => Some(write),
        F::Incomplete { snapshot, .. } => Some(snapshot),
        // The proof's; or a name no object is (a missing parent, which a door refuses first), or
        // the store's files, which `suzatary verify` reports.
        F::Unsigned { .. }
        | F::Forged { .. }
        | F::Broken { .. }
        | F::Cycle(_)
        | F::Io(_)
        | F::Garbage(_)
        | F::Leftover(_)
        | F::Quarantined(_)
        | F::Unignored(_)
        | F::StaleHeads { .. } => None,
    }
}

/// Why an object carrying no event is not admitted.
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

/// An object's parents and the genesis a change names, which verify-on-receive walks to.
fn rests_on<E>(envelope: &prodrome_core::event::Envelope<E>) -> Vec<Hash> {
    let mut names = parents_of(envelope);
    names.extend(envelope.genesis().cloned());
    names
}

/// One literal parsed, reprinted and named by the hash of the print, never the client's name.
fn named(literal: &str) -> Result<(Hash, String, Envelope), Reason> {
    named_at(&suzatary_record::record::TodoVocabulary::default(), literal)
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
    use prodrome_core::event::{
        Hash, mk_cancelled, mk_completed, mk_created, mk_reopened, mk_sealed, mk_woven,
    };
    use prodrome_core::literal::Datetime;
    use prodrome_core::store::{MemoryStore, Replica};
    use suzatary_record::record::TodoEvent;

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
            .iter()
            .map(|name| {
                let envelope = by_name[name].clone();
                (name.clone(), envelope)
            })
            .collect()
    }

    fn apply(held: &mut Objects, taken: &[(Hash, String)]) {
        for (name, literal) in taken {
            let envelope =
                parse_envelope(&suzatary_record::record::TodoVocabulary::default(), literal)
                    .expect("we printed it");
            assert_eq!(&seal_hash(&envelope), name);
            held.insert(name.clone(), envelope);
        }
    }

    fn folded(set: &Objects) -> Vec<suzatary_record::record::Entry> {
        let nodes = prodrome_core::dag::Dag::from_iter(objects_of(set))
            .nodes()
            .expect("a closed set reads");
        prodrome_core::view::entries(&nodes, at(28), &prodrome_core::policy::Untrusted::none())
            .expect("a closed set folds")
    }

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

    /// The name is the print's hash, never the client's claim.
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

    /// The actor rule applies only to a write: re-offering a held object is `already`.
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
            suzatary_record::record::mk_authored(
                "settlethis",
                at(1),
                "assistant",
                "decision",
                at(1),
                suzatary_record::record::MarkupSource::escaped("Send the reply"),
                prodrome_core::fpl::mk_absent(),
                vec!["because he asked".to_owned()],
                "",
                "",
                suzatary_record::record::MarkupSource::escaped(
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

    /// Two diverged replicas leave two heads, and nothing joins them.
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
        tab.receive(
            heads(&set),
            &|name| prints.get(name).map(|print| print.clone().into_bytes()),
            prodrome_core::store::Sender::Stranger,
        )
        .expect("a closed set is received");
        tab
    }

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

    fn seed(byte: char) -> prodrome_core::sign::Secret {
        prodrome_core::sign::Secret::new(&byte.to_string().repeat(64)).expect("a seed")
    }

    fn print(object: &Envelope) -> (Hash, String) {
        (seal_hash(object), canonical_envelope(object))
    }

    fn rooted(held: &mut Objects, root: &prodrome_core::sign::Secret, object: Envelope) -> Hash {
        let name = seal_hash(&object);
        held.insert(name.clone(), object);
        let signed = Envelope::Signed(root.sign(&name));
        held.insert(seal_hash(&signed), signed);
        name
    }

    /// A little history with `phone` registered to ada by the root, held by the box and a tab.
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
            parse_envelope(&suzatary_record::record::TodoVocabulary::default(), print)
                .expect("a print");
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
        let outcome = receive(
            &held,
            &[change, signature],
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
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
            &Untrusted::none(),
        );
        assert_eq!(
            reasons(&outcome),
            vec![Reason::Unproven(Unproven::Unsigned)]
        );
        // An unsigned write waits for its device, where a forged one never goes.
        assert_eq!(
            Reason::Unproven(Unproven::Unsigned).kind(),
            suzatary_signatures::wire::signatures::RefusalKind::Trust
        );
        assert_eq!(
            Reason::Unproven(Unproven::Forged).kind(),
            suzatary_signatures::wire::signatures::RefusalKind::Final
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
            &Untrusted::none(),
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
        let outcome = receive(
            &held,
            &posted,
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
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
            receive(
                &held,
                &posted,
                &actor("ada"),
                &root.public(),
                &Untrusted::none()
            )
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
        let outcome = receive(
            &held,
            &posted,
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
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
            &Untrusted::none(),
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
        let outcome = receive(
            &held,
            &[after, late],
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
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
        let outcome = receive(
            &held,
            &[text],
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
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
        let outcome = receive(
            &held,
            &[text],
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
        assert_eq!(reasons(&outcome), vec![Reason::BoxOnly]);
        // Nor a prodrome of its own, which would wedge every enrolment that reads the first.
        let (_, begun) = print(&Envelope::Genesis(
            prodrome_core::genesis::mk_genesis("chain", &"7".repeat(32)).expect("a genesis"),
        ));
        let outcome = receive(
            &held,
            &[begun],
            &actor("ada"),
            &root.public(),
            &Untrusted::none(),
        );
        assert_eq!(reasons(&outcome), vec![Reason::BoxOnly]);
    }

    /// A genesis, a key or an epoch is the box's alone, refused before any signature is read.
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

    use proptest::prelude::*;

    /// A forest of chains that occasionally branch.
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

    fn a_delivery(size: usize) -> impl Strategy<Value = (Vec<usize>, Vec<usize>)> {
        (
            proptest::collection::vec(0..size.max(1), size),
            proptest::collection::vec(1usize..4, 1..6),
        )
    }

    fn deliver(objects: &[Envelope], order: &[usize], batches: &[usize]) -> Objects {
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
        // The exchange repeats until it takes nothing: convergence is the fixpoint.
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
        #[test]
        fn receiving_in_any_order_or_batching_yields_one_store(
            objects in a_dag(),
            (order, batches) in a_delivery(9),
            (other_order, other_batches) in a_delivery(9),
        ) {
            let one = deliver(&objects, &order, &batches);
            let two = deliver(&objects, &other_order, &other_batches);
            for object in &objects {
                prop_assert!(one.holds(&seal_hash(object)));
                prop_assert!(two.holds(&seal_hash(object)));
            }
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

        /// A box that took two diverged logs in IS their union, so it folds as the union.
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
            let left = branch(tip.as_ref(), "left", mine);
            let right = branch(tip.as_ref(), "right", theirs);
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
