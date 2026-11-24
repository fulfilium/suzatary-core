//! A replica of a store whose nodes are its objects: the rows of its pull beside what this
//! replica wrote. Every [`Move`] is a join, so two tabs' moves in any order hold the same replica.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;

use prodrome_data::event::{Envelope, Hash, parents_of, parse_envelope};
use prodrome_data::schema::Schema;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::replica::form::{self, DeltaForm, Name, RowForm, bytes, hash};
use crate::replica::frontier::Frontier;
use crate::replica::held::{Node, Said};
use crate::replica::pulling::{Delta, Lands, Pulling};
use crate::replica::row::{Keys, Row};
use crate::replica::writes::{Answer, Own, Role, Writes};
use crate::snapshot::{self, NotASnapshot, Unprinted};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    name: Hash,
    rests_on: Vec<Hash>,
    prodrome: Option<Hash>,
    literal: String,
}

impl Object {
    /// A print that does not parse rests on nothing, so the reading leaves it out (SPEC law 44).
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

    #[must_use]
    pub fn literal(&self) -> &str {
        &self.literal
    }

    /// Read without a parse: a canonical print begins with its constructor's name, and a parsed
    /// signature rests on the one object it signs.
    #[must_use]
    pub fn is_signature(&self) -> bool {
        self.rests_on.len() == 1 && self.literal.starts_with("Signed(")
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

pub(crate) type ObjectRows = Pulling<Object, NoKeys>;

pub(crate) type ObjectDelta = Delta<Object, NoKeys>;

/// Who the box does not believe, and the root key a device key's signatures count under; `None`
/// for a store the box alone writes.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Policy {
    pub untrusted: BTreeSet<String>,
    pub root: Option<String>,
}

/// The policy the box read a piece under, at the frontier its answer named, in the generation of the
/// rows it was asked from. Joined by the later generation, then the later frontier, both of which
/// the box's answers decide, never a device's clock; two answers at one frontier hold the union of
/// who is not believed, so a tab never trusts an actor another answer at that frontier distrusts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heard {
    pub generation: u64,
    pub frontier: Frontier,
    pub policy: Policy,
}

impl Heard {
    #[must_use]
    pub fn join(self, other: &Heard) -> Heard {
        match (self.generation, &self.frontier).cmp(&(other.generation, &other.frontier)) {
            std::cmp::Ordering::Less => other.clone(),
            std::cmp::Ordering::Greater => self,
            std::cmp::Ordering::Equal => Heard {
                policy: Policy {
                    untrusted: self
                        .policy
                        .untrusted
                        .union(&other.policy.untrusted)
                        .cloned()
                        .collect(),
                    root: self.policy.root.max(other.policy.root.clone()),
                },
                ..self
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Objects {
    rows: ObjectRows,
    own: Writes<Object>,
    heard: Option<Heard>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    Rows(ObjectDelta),
    /// A piece the box answered, beside the policy it was read under.
    Piece {
        delta: ObjectDelta,
        heard: Heard,
    },
    Own(Vec<Own<Object>>),
}

impl Objects {
    #[must_use]
    pub fn rows(&self) -> &ObjectRows {
        &self.rows
    }

    pub fn piece(&mut self, said: Said<Object>, policy: Policy, fetched_at: u64) -> Move {
        let asked = self.rows.holding();
        let heard = Heard {
            generation: asked.generation(),
            frontier: asked.replica().frontier().answered(
                said.since,
                said.digest.clone(),
                said.count,
            ),
            policy,
        };
        let delta = self.rows.piece(said, NoKeys, fetched_at);
        self.hear(&heard);
        Move::Piece { delta, heard }
    }

    /// The policy the box served the landed rows' generation under, none before it has spoken in
    /// that generation.
    #[must_use]
    pub fn policy(&self) -> Option<&Policy> {
        self.heard
            .as_ref()
            .filter(|h| h.generation == self.rows.landed().generation())
            .map(|h| &h.policy)
    }

    fn hear(&mut self, heard: &Heard) {
        self.heard = Some(match self.heard.take() {
            None => heard.clone(),
            Some(h) => h.join(heard),
        });
    }

    pub fn land(&mut self) -> Move {
        Move::Rows(self.rows.land())
    }

    /// What this replica wrote stays.
    pub fn start_over(&mut self) -> Move {
        Move::Rows(self.rows.start_over())
    }

    pub fn write(&mut self, objects: Vec<Object>) -> Move {
        Move::Own(self.own.write(objects))
    }

    /// An answer for an object this replica did not write is passed over.
    pub fn answered(&mut self, answers: impl IntoIterator<Item = (Hash, Answer)>) -> Move {
        Move::Own(self.own.answered(answers))
    }

    pub fn refused(&mut self, refused: BTreeMap<Hash, (crate::receipt::Grounds, String)>) -> Move {
        Move::Own(self.own.refused(refused))
    }

    pub fn let_go(&mut self, names: &BTreeSet<Hash>) -> Move {
        Move::Own(self.own.let_go(names))
    }

    /// Every write this replica made, with the box's answer.
    pub fn own(&self) -> impl Iterator<Item = &Own<Object>> {
        self.own.iter()
    }

    /// The move of [`Objects::write`], made and not taken: a plan the caller keeps before the
    /// replica applies it.
    #[must_use]
    pub fn writing(objects: Vec<Object>) -> Move {
        Move::Own(Writes::writing(objects))
    }

    /// The move of [`Objects::let_go`], made and not taken.
    #[must_use]
    pub fn letting_go(&self, names: &BTreeSet<Hash>) -> Move {
        Move::Own(self.own.letting_go(names))
    }

    /// Every write the box has not taken and the rows do not hold, signatures included, each
    /// with its answer: what letting go loses, read by kind.
    #[must_use]
    pub fn untaken(&self) -> Vec<&Own<Object>> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own
            .untaken()
            .filter(|own| !reading.holds(own.node.name()))
            .collect()
    }

    /// The writes the box has not taken that rest in a prodrome it no longer serves.
    #[must_use]
    pub fn stranded(&self) -> Vec<&Object> {
        let reading = self.rows.landed().replica().read();
        let gone = reading.gone();
        self.untaken()
            .into_iter()
            .map(|own| &own.node)
            .filter(|o| o.prodrome.as_ref().is_some_and(|p| gone.contains(p)))
            .collect()
    }

    /// What `other` holds joined in: its rows, its writes and its policy. A load joins what it read
    /// into what this replica holds, so a write made meanwhile is never lost.
    pub fn join(&mut self, other: &Objects) {
        self.rows.join(&other.rows);
        self.own
            .apply(&other.own.iter().cloned().collect::<Vec<_>>());
        if let Some(h) = &other.heard {
            self.hear(h);
        }
    }

    /// Whether the landed rows hold all they say they hold ([`Pulling::closed`]): a store of
    /// objects lands each pull whole.
    #[must_use]
    pub fn closed(&self) -> bool {
        self.rows.closed(Lands::Whole)
    }

    pub fn apply(&mut self, m: &Move) {
        match m {
            Move::Rows(delta) => self.rows.apply(delta),
            Move::Piece { delta, heard } => {
                self.rows.apply(delta);
                self.hear(heard);
            }
            Move::Own(owned) => self.own.apply(owned),
        }
    }

    /// The landed row's objects and this replica's writes, less each refused for good, of a gone
    /// prodrome, or resting on either.
    #[must_use]
    pub fn standing(&self) -> Vec<&Object> {
        let reading = self.rows.landed().replica().read();
        let mine = self
            .own
            .standing(|name| reading.holds(name), reading.gone());
        reading.nodes().chain(mine).collect()
    }

    #[must_use]
    pub fn unsent(&self) -> Vec<&Object> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own.unsent(|name| reading.holds(name))
    }

    #[must_use]
    pub fn waiting(&self) -> Vec<(&Object, String)> {
        self.own.waiting()
    }

    #[must_use]
    pub fn refusals(&self) -> Vec<(&Object, crate::replica::writes::Refusal, String)> {
        self.own.refusals()
    }

    /// The unsent changes no signature of this replica covers; a signature refused for good covers
    /// nothing.
    #[must_use]
    pub fn unsigned<E: Schema>(&self, schema: &E::Vocabulary) -> Vec<&Object> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own
            .unsigned(|name| reading.holds(name), |o| role::<E>(schema, o))
    }

    /// [`Objects::unsent`] in posts that each fit `fit`, each change beside its signatures.
    #[must_use]
    pub fn unsent_pieces<E: Schema>(
        &self,
        schema: &E::Vocabulary,
        fit: crate::replica::writes::Fit,
        weight: impl Fn(&Object) -> usize,
    ) -> Vec<Vec<&Object>> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own.pieces(
            |name| reading.holds(name),
            |o| role::<E>(schema, o),
            fit,
            weight,
        )
    }
}

fn role<E: Schema>(schema: &E::Vocabulary, o: &Object) -> Role {
    match parse_envelope::<E>(schema, &o.literal) {
        Ok(Envelope::Change(_)) => Role::Change,
        Ok(Envelope::Signed(signature)) => Role::Signs(signature.object),
        Ok(_) | Err(_) => Role::Other,
    }
}

#[derive(Serialize, Deserialize)]
struct ObjectForm {
    name: Name,
    rests_on: Vec<Name>,
    prodrome: Option<Name>,
    literal: String,
}

type ObjectRowForm = RowForm<ObjectForm, ()>;

#[derive(Serialize, Deserialize)]
struct OwnForm {
    object: ObjectForm,
    answer: Answer,
}

#[derive(Serialize, Deserialize)]
struct HeardForm {
    generation: u64,
    era: u64,
    count: u64,
    digest: Name,
    untrusted: Vec<String>,
    root: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Given {
    landed: ObjectRowForm,
    landings: u64,
    pending: ObjectRowForm,
    own: Vec<OwnForm>,
    heard: Option<HeardForm>,
}

#[derive(Serialize, Deserialize)]
enum MoveForm {
    Rows(DeltaForm<ObjectRowForm, ()>),
    Piece(DeltaForm<ObjectRowForm, ()>, HeardForm),
    Own(Vec<OwnForm>),
}

impl snapshot::Held for Objects {
    type Given = Given;
    /// 2: the landed and pending rows, the own objects, every name as its 32 bytes, and the
    /// policy heard.
    const FORM: u32 = 2;

    fn given(&self) -> Given {
        Given {
            landed: row_form(self.rows.landed()),
            landings: self.rows.landings(),
            pending: row_form(self.rows.pending()),
            own: self.own.iter().map(own_form).collect(),
            heard: self.heard.as_ref().map(heard_form),
        }
    }

    fn of_given(given: Given) -> Result<Self, String> {
        Ok(Objects {
            rows: Pulling::of(
                row_of(given.landed)?,
                given.landings,
                row_of(given.pending)?,
            ),
            own: Writes::of(
                &given
                    .own
                    .into_iter()
                    .map(own_of)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            heard: given.heard.map(heard_of).transpose()?,
        })
    }
}

/// `m` as a frame of the log: the snapshot's form, then the move in postcard.
pub fn frame(m: &Move) -> Result<Vec<u8>, Unprinted> {
    let delta = |delta| form::delta_form(delta, row_form, |n| match *n {});
    let form = match m {
        Move::Rows(d) => MoveForm::Rows(delta(d)),
        Move::Piece { delta: d, heard } => MoveForm::Piece(delta(d), heard_form(heard)),
        Move::Own(owned) => MoveForm::Own(owned.iter().map(own_form).collect()),
    };
    snapshot::print(<Objects as snapshot::Held>::FORM, &form)
}

/// The move `bytes` print. A frame of another form is [`NotASnapshot::Form`], as a snapshot of
/// another form is: the caller decides what an earlier form's copy becomes, for both alike.
pub fn unframe(bytes: &[u8]) -> Result<Move, NotASnapshot> {
    let form: MoveForm = snapshot::read(<Objects as snapshot::Held>::FORM, bytes)?;
    let delta = |delta| {
        form::delta_of(delta, row_of, |(): &()| {
            Err("a store of objects holds no key to let go".to_owned())
        })
        .map_err(NotASnapshot::Refused)
    };
    let m = match form {
        MoveForm::Rows(d) => Move::Rows(delta(d)?),
        MoveForm::Piece(d, heard) => Move::Piece {
            delta: delta(d)?,
            heard: heard_of(heard).map_err(NotASnapshot::Refused)?,
        },
        MoveForm::Own(owned) => Move::Own(
            owned
                .into_iter()
                .map(own_of)
                .collect::<Result<_, _>>()
                .map_err(NotASnapshot::Refused)?,
        ),
    };
    Ok(m)
}

fn heard_form(heard: &Heard) -> HeardForm {
    let (era, count, digest) = match &heard.frontier {
        Frontier::Nothing => (0, 0, [0; 32]),
        Frontier::Named { era, count, digest } => (*era, *count, bytes(digest.as_str())),
    };
    HeardForm {
        generation: heard.generation,
        era,
        count,
        digest,
        untrusted: heard.policy.untrusted.iter().cloned().collect(),
        root: heard.policy.root.clone(),
    }
}

/// A frontier named by an answer is never `Nothing`, so a print names one.
fn heard_of(form: HeardForm) -> Result<Heard, String> {
    Ok(Heard {
        generation: form.generation,
        frontier: Frontier::Named {
            era: form.era,
            count: form.count,
            digest: hash(&form.digest)?,
        },
        policy: Policy {
            untrusted: form.untrusted.into_iter().collect(),
            root: form.root,
        },
    })
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

fn own_form(own: &Own<Object>) -> OwnForm {
    OwnForm {
        object: object_form(&own.node),
        answer: own.answer.clone(),
    }
}

fn own_of(form: OwnForm) -> Result<Own<Object>, String> {
    Ok(Own {
        node: object_of(form.object)?,
        answer: form.answer,
    })
}
