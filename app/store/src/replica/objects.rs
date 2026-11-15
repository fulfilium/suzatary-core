//! A replica of a store whose nodes are its objects, each with its literal: the rows of its pull
//! ([`Pulling`]) beside what this replica wrote ([`Writes`]). It names no Suzatary store. Every
//! [`Move`] is a join, so two tabs' moves in any order hold the same replica, and an own object
//! stays whatever the rows do.

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
use crate::replica::writes::{Answer, Own, Role, Writes};
use crate::snapshot::{self, NotASnapshot, Unprinted};

/// One object: its name, parents, prodrome and print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    name: Hash,
    rests_on: Vec<Hash>,
    prodrome: Option<Hash>,
    literal: String,
}

impl Object {
    /// `literal`, named by its bytes and read at `schema`; a print that does not parse rests on
    /// nothing, so the reading leaves it out (Prodrome SPEC law 44) and it never stops a pull.
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

    /// Whether this is a signature, which asserts no event. Read without a parse: a canonical
    /// print begins with its constructor's name, and a signature that parsed rests on the one
    /// object it signs.
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

/// The keys of a store that locks no words: none.
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

/// The rows, and what this replica wrote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Objects {
    rows: ObjectRows,
    own: Writes<Object>,
}

/// One move of the replica.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    /// The rows moved.
    Rows(ObjectDelta),
    /// Objects written or answered, joined in.
    Own(Vec<Own<Object>>),
}

impl Objects {
    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &ObjectRows {
        &self.rows
    }

    /// The piece `said`, heard at `fetched_at`, kept beside the landed row.
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
        Move::Own(self.own.write(objects))
    }

    /// The box's answers to what this replica wrote; any other is passed over.
    pub fn answered(&mut self, answers: impl IntoIterator<Item = (Hash, Answer)>) -> Move {
        Move::Own(self.own.answered(answers))
    }

    /// The asks' refusals, joined in as final where they name an own object.
    pub fn refused(&mut self, refused: BTreeMap<Hash, String>) -> Move {
        Move::Own(self.own.refused(refused))
    }

    /// `m` taken, whoever made it.
    pub fn apply(&mut self, m: &Move) {
        match m {
            Move::Rows(delta) => self.rows.apply(delta),
            Move::Own(owned) => self.own.apply(owned),
        }
    }

    /// Every object a reading reads: the landed row's and what this replica wrote, less each
    /// object refused for good, of a prodrome the box no longer holds, or resting on either.
    #[must_use]
    pub fn standing(&self) -> Vec<&Object> {
        let reading = self.rows.landed().replica().read();
        let mine = self
            .own
            .standing(|name| reading.holds(name), reading.gone());
        reading.nodes().chain(mine).collect()
    }

    /// What this replica wrote that the box has not taken and may take.
    #[must_use]
    pub fn unsent(&self) -> Vec<&Object> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own.unsent(|name| reading.holds(name))
    }

    /// What the box refused for good of this replica's writes, with its sentence.
    #[must_use]
    pub fn refusals(&self) -> Vec<(&Object, &str)> {
        self.own.refusals()
    }

    /// The unsent changes that no signature this replica wrote covers, read at `schema`; a
    /// signature the box refused for good covers nothing.
    #[must_use]
    pub fn unsigned<E: Schema>(&self, schema: &E::Vocabulary) -> Vec<&Object> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own
            .unsigned(|name| reading.holds(name), |o| role::<E>(schema, o))
    }

    /// [`Objects::unsent`] in posts of at most `most`, each change beside its signatures.
    #[must_use]
    pub fn unsent_pieces<E: Schema>(
        &self,
        schema: &E::Vocabulary,
        most: usize,
    ) -> Vec<Vec<&Object>> {
        let holding = self.rows.holding();
        let reading = holding.replica().read();
        self.own
            .pieces(|name| reading.holds(name), |o| role::<E>(schema, o), most)
    }
}

/// What `o` is to a post, read at `schema`.
fn role<E: Schema>(schema: &E::Vocabulary, o: &Object) -> Role {
    match parse_envelope::<E>(schema, &o.literal) {
        Ok(Envelope::Change(_)) => Role::Change,
        Ok(Envelope::Signed(signature)) => Role::Signs(signature.object),
        Ok(_) | Err(_) => Role::Other,
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
    /// 1: the landed and pending rows and the own objects, every name as its 32 bytes.
    const FORM: u32 = 1;

    fn given(&self) -> Given {
        Given {
            landed: row_form(self.rows.landed()),
            landings: self.rows.landings(),
            pending: row_form(self.rows.pending()),
            own: self.own.iter().map(own_form).collect(),
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

/// The move `bytes` print, or none for a frame of another form.
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
