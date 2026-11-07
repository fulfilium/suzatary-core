//! A replica of a store whose nodes are its objects, each carrying its literal, so a reading
//! never waits on a fetch and no key locks its words: the rows of its pull ([`Pulling`]) beside
//! the objects this replica wrote itself, each with how the box answered it. Nothing here names a
//! Suzatary store: a store gives the schema its objects are read at.
//!
//! Every [`Move`] is a join: the rows' deltas but a purge's are, and an own object's answer only
//! climbs ([`Answer`]'s order), so two tabs' moves taken in any order and any number of times
//! hold the same replica. An own object stays whatever the rows do, so a reset or a later epoch
//! never drops what the box has not taken.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;

use prodrome_core::event::{Envelope, Hash, parents_of, parse_envelope};
use prodrome_core::schema::Schema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::replica::form::{self, DeltaForm, Name, RowForm, bytes, hash};
use crate::replica::held::{Node, Said};
use crate::replica::pulling::{Delta, Pulling};
use crate::replica::row::{Keys, Row};
use crate::snapshot::{self, NotASnapshot, Unprinted};

/// One object: its name, what it rests on, the prodrome it names itself as in, and its print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    name: Hash,
    rests_on: Vec<Hash>,
    prodrome: Option<Hash>,
    literal: String,
}

impl Object {
    /// `literal`, named by its bytes, read at `schema` for what it rests on and its prodrome. A
    /// print that does not parse at the schema rests on nothing and names no prodrome: the
    /// reading at the schema leaves it out with whatever rests on it (Prodrome SPEC law 44), so
    /// one such print never stops a pull.
    #[must_use]
    pub fn read<E: Schema>(schema: &E::Vocabulary, literal: String) -> Self {
        let name = Hash::new(hex::encode(Sha256::digest(literal.as_bytes())))
            .unwrap_or_else(|_| unreachable!("a SHA-256 in lowercase hex is a name"));
        let (rests_on, prodrome) = match parse_envelope::<E>(schema, &literal) {
            Ok(envelope) => {
                let prodrome = match &envelope {
                    Envelope::Genesis(_) => Some(name.clone()),
                    other => other.genesis().cloned(),
                };
                (parents_of(&envelope), prodrome)
            }
            Err(_) => (Vec::new(), None),
        };
        Object {
            name,
            rests_on,
            prodrome,
            literal,
        }
    }

    /// The print.
    #[must_use]
    pub fn literal(&self) -> &str {
        &self.literal
    }
}

impl Node for Object {
    fn name(&self) -> &Hash {
        &self.name
    }

    fn rests_on(&self) -> &[Hash] {
        &self.rests_on
    }

    fn prodrome(&self) -> Option<&Hash> {
        self.prodrome.as_ref()
    }
}

/// The keys of a store that locks no words: none, ever.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoKeys;

impl Keys for NoKeys {
    type Name = Infallible;

    fn union(self, _: &Self) -> Self {
        self
    }

    fn without(self, _: &BTreeSet<Infallible>) -> Self {
        self
    }

    fn is_empty(&self) -> bool {
        true
    }
}

/// The rows of a store of objects.
pub(crate) type ObjectRows = Pulling<Object, NoKeys>;

/// One move of those rows.
pub(crate) type ObjectDelta = Delta<Object, NoKeys>;

/// Why the box refused an object: for good, or until the device that signed it is trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Refusal {
    /// Taken once the device is trusted: posted again.
    Trust,
    /// Never taken as it is: held back, with everything resting on it.
    Final,
}

/// How the box answered an object this replica wrote. The order is the join's: an answer only
/// climbs, so a refusal stands over the post it answered and a taking over every refusal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Answer {
    /// Not answered yet.
    Unsent,
    /// Refused, in the box's words.
    Refused {
        /// For good, or until trusted.
        kind: Refusal,
        /// The box's sentence.
        reason: String,
    },
    /// The box holds it.
    Taken,
}

/// An object this replica wrote, and how the box answered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Own {
    /// The object.
    pub object: Object,
    /// The box's answer.
    pub answer: Answer,
}

/// The replica: the rows, and what it wrote by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Objects {
    rows: ObjectRows,
    own: BTreeMap<Hash, Own>,
}

/// One move of the replica.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    /// The rows moved.
    Rows(ObjectDelta),
    /// Objects written, or answered: each joined into what was written.
    Own(Vec<Own>),
}

impl Objects {
    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &ObjectRows {
        &self.rows
    }

    /// The piece the box answered, `said` heard at `fetched_at`, kept beside the landed row.
    pub fn piece(&mut self, said: Said<Object>, fetched_at: u64) -> Move {
        Move::Rows(self.rows.piece(said, NoKeys, fetched_at))
    }

    /// The pending pieces landed.
    pub fn land(&mut self) -> Move {
        Move::Rows(self.rows.land())
    }

    /// Every row reset, in the generation after both; what this replica wrote stays.
    pub fn start_over(&mut self) -> Move {
        Move::Rows(self.rows.start_over())
    }

    /// `objects` written by this replica, unsent.
    pub fn write(&mut self, objects: Vec<Object>) -> Move {
        self.took(Move::Own(
            objects
                .into_iter()
                .map(|object| Own {
                    object,
                    answer: Answer::Unsent,
                })
                .collect(),
        ))
    }

    /// The box's answers to what this replica wrote; an answer about another object is passed
    /// over, since the replica has nothing of it to hold the answer by.
    pub fn answered(&mut self, answers: impl IntoIterator<Item = (Hash, Answer)>) -> Move {
        let owned: Vec<Own> = answers
            .into_iter()
            .filter_map(|(name, answer)| {
                self.own.get(&name).map(|own| Own {
                    object: own.object.clone(),
                    answer,
                })
            })
            .collect();
        self.took(Move::Own(owned))
    }

    fn took(&mut self, m: Move) -> Move {
        self.apply(&m);
        m
    }

    /// `m` taken: this replica's own, another tab's, or one read back from the device.
    pub fn apply(&mut self, m: &Move) {
        match m {
            Move::Rows(delta) => self.rows.apply(delta),
            Move::Own(owned) => {
                for own in owned {
                    match self.own.get_mut(own.object.name()) {
                        Some(held) => {
                            if own.answer > held.answer {
                                held.answer = own.answer.clone();
                            }
                        }
                        None => {
                            self.own.insert(own.object.name().clone(), own.clone());
                        }
                    }
                }
            }
        }
    }

    /// Every object a reading reads: the landed row's, and what this replica wrote that it does
    /// not hold, less each object refused for good, of a prodrome the box no longer holds, or
    /// resting on either.
    #[must_use]
    pub fn standing(&self) -> Vec<&Object> {
        let reading = self.rows.landed().replica().read();
        let mine = self.mine(|name| reading.holds(name));
        let blocked = self.blocked(&mine, reading.gone());
        reading
            .nodes()
            .chain(mine.into_iter().filter(|o| !blocked.contains(o.name())))
            .collect()
    }

    /// What this replica wrote that the box has not taken and may take: unsent, or refused until
    /// trusted, and neither held by what is held, landed and not, nor held back by a refusal for
    /// good.
    #[must_use]
    pub fn unsent(&self) -> Vec<&Object> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        let mine = self.mine(|name| reading.holds(name));
        let blocked = self.blocked(&mine, &BTreeSet::new());
        mine.into_iter()
            .filter(|o| {
                !blocked.contains(o.name())
                    && matches!(
                        self.own.get(o.name()).map(|own| &own.answer),
                        Some(
                            Answer::Unsent
                                | Answer::Refused {
                                    kind: Refusal::Trust,
                                    ..
                                }
                        )
                    )
            })
            .collect()
    }

    /// What this replica wrote that `held` does not hold.
    fn mine(&self, held: impl Fn(&Hash) -> bool) -> Vec<&Object> {
        self.own
            .values()
            .map(|own| &own.object)
            .filter(|o| !held(o.name()))
            .collect()
    }

    /// Those of `mine` refused for good or of a prodrome in `gone`, and every one resting on one.
    fn blocked<'o>(&self, mine: &[&'o Object], gone: &BTreeSet<Hash>) -> BTreeSet<&'o Hash> {
        let mut blocked: BTreeSet<&Hash> = mine
            .iter()
            .filter(|o| {
                o.prodrome().is_some_and(|p| gone.contains(p))
                    || matches!(
                        self.own.get(o.name()).map(|own| &own.answer),
                        Some(Answer::Refused {
                            kind: Refusal::Final,
                            ..
                        })
                    )
            })
            .map(|o| o.name())
            .collect();
        loop {
            let more: Vec<&Hash> = mine
                .iter()
                .filter(|o| {
                    !blocked.contains(o.name()) && o.rests_on().iter().any(|p| blocked.contains(p))
                })
                .map(|o| o.name())
                .collect();
            if more.is_empty() {
                return blocked;
            }
            blocked.extend(more);
        }
    }
}

/// An object as the device keeps it.
#[derive(Serialize, Deserialize)]
struct ObjectForm {
    name: Name,
    rests_on: Vec<Name>,
    prodrome: Option<Name>,
    literal: String,
}

/// A row of objects: no keys.
type ObjectRowForm = RowForm<ObjectForm, ()>;

#[derive(Serialize, Deserialize)]
struct OwnForm {
    object: ObjectForm,
    answer: Answer,
}

/// What a snapshot of a store of objects prints.
#[derive(Serialize, Deserialize)]
pub struct Given {
    landed: ObjectRowForm,
    landings: u64,
    pending: ObjectRowForm,
    own: Vec<OwnForm>,
}

#[derive(Serialize, Deserialize)]
enum MoveForm {
    Rows(DeltaForm<ObjectRowForm, ()>),
    Own(Vec<OwnForm>),
}

impl snapshot::Held for Objects {
    type Given = Given;
    /// 1: the landed row and the pending pieces, each object with its edges and print, every
    /// name as its 32 bytes, and what the replica wrote with the box's answer.
    const FORM: u32 = 1;

    fn given(&self) -> Given {
        Given {
            landed: row_form(self.rows.landed()),
            landings: self.rows.landings(),
            pending: row_form(self.rows.pending()),
            own: self.own.values().map(own_form).collect(),
        }
    }

    fn of_given(given: Given) -> Result<Self, String> {
        Ok(Objects {
            rows: Pulling::of(
                row_of(given.landed)?,
                given.landings,
                row_of(given.pending)?,
            ),
            own: given
                .own
                .into_iter()
                .map(|form| own_of(form).map(|own| (own.object.name.clone(), own)))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// `m` as a frame of the log: the snapshot's form, then the move in postcard.
///
/// # Errors
/// [`Unprinted`] where it refuses to print.
pub fn frame(m: &Move) -> Result<Vec<u8>, Unprinted> {
    let form = match m {
        Move::Rows(delta) => MoveForm::Rows(form::delta_form(delta, row_form, |n| match *n {})),
        Move::Own(owned) => MoveForm::Own(owned.iter().map(own_form).collect()),
    };
    snapshot::print(<Objects as snapshot::Held>::FORM, &form)
}

/// The move `bytes` print, or none for a frame of another form, which this build passes over.
///
/// # Errors
/// [`NotASnapshot`]: a frame of this form cut short, unread, or holding what no replica holds.
pub fn unframe(bytes: &[u8]) -> Result<Option<Move>, NotASnapshot> {
    let form: MoveForm = match snapshot::read(<Objects as snapshot::Held>::FORM, bytes) {
        Ok(form) => form,
        Err(NotASnapshot::Form { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let m = match form {
        MoveForm::Rows(delta) => Move::Rows(
            form::delta_of(delta, row_of, |(): &()| {
                Err("a store of objects holds no key to let go".to_owned())
            })
            .map_err(NotASnapshot::Refused)?,
        ),
        MoveForm::Own(owned) => Move::Own(
            owned
                .into_iter()
                .map(own_of)
                .collect::<Result<_, _>>()
                .map_err(NotASnapshot::Refused)?,
        ),
    };
    Ok(Some(m))
}

fn row_form(row: &Row<Object, NoKeys>) -> ObjectRowForm {
    form::row_form(row, object_form, |_| ())
}

fn row_of(form: ObjectRowForm) -> Result<Row<Object, NoKeys>, String> {
    form::row_of(form, object_of, |()| Ok(NoKeys))
}

fn object_form(o: &Object) -> ObjectForm {
    ObjectForm {
        name: bytes(o.name.as_str()),
        rests_on: o.rests_on.iter().map(|p| bytes(p.as_str())).collect(),
        prodrome: o.prodrome.as_ref().map(|p| bytes(p.as_str())),
        literal: o.literal.clone(),
    }
}

fn object_of(form: ObjectForm) -> Result<Object, String> {
    Ok(Object {
        name: hash(&form.name)?,
        rests_on: form.rests_on.iter().map(hash).collect::<Result<_, _>>()?,
        prodrome: form.prodrome.as_ref().map(hash).transpose()?,
        literal: form.literal,
    })
}

fn own_form(own: &Own) -> OwnForm {
    OwnForm {
        object: object_form(&own.object),
        answer: own.answer.clone(),
    }
}

fn own_of(form: OwnForm) -> Result<Own, String> {
    Ok(Own {
        object: object_of(form.object)?,
        answer: form.answer,
    })
}
