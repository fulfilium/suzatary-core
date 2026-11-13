//! A husk: an object whose sealed parts were dropped, kept under the name of the whole it was.
//!
//! A store maps each name to a whole print, which hashes to the name, or to a husk. Both read as
//! an object: a husk reads as its whole with each sealed part replaced by the name of the key it
//! was sealed under, and a reader opens a sealed part only with that key. So wherever no key a
//! holder keeps opens a part of the whole, the husk reads exactly as the whole, and since keys
//! are only destroyed, it does so for good.
//!
//! What a holder of a husk has verified of its name, and what it has not:
//! - it holds nothing any key could open: [`Husk::read`] parses at a vocabulary with no sealed
//!   constructor, and the schema refuses a sealed part in a husk;
//! - its parents are in the history, which a store checks as it does for a whole;
//! - its name is bound by each verified object that names it as a parent, and a husk no such
//!   object names is bound by its maker's word alone: SHA-256 cannot be recomputed without the
//!   dropped bytes;
//! - its clear fields are its maker's word: the holder that dropped them from a whole it had
//!   verified ([`Husk::of`]), or the party a replica took it from, checked against what that
//!   replica already holds of the name.
//!
//! This is git's promisor object, and it is the one answer for a store on disk and for a replica.
//! Generic over the schema and free of any application type, it belongs in the Prodrome beside
//! `store::replica`.

use prodrome_core::change::{Change, mk_change};
use prodrome_core::event::{Envelope, Hash, canonical_envelope};
use prodrome_core::literal::codec::{Constructor, Fields};
use prodrome_core::literal::{ProdromeError, Signature, Table, Value, Vocabulary};
use prodrome_core::literal::{parse_literal, print_literal};
use prodrome_core::schema::Schema;

/// A schema whose events hold sealed parts, each under a named key, which a holder may drop.
pub trait Husking: Schema {
    /// What names the key a part was sealed under.
    type Seal: Ord;
    /// The schema's constructors and each sealed part's dropped form, without its sealed form.
    type Husks: Vocabulary + Default;

    /// The event with every sealed part dropped, or `None` where it holds none.
    fn drop_sealed(&self) -> Option<Self>;

    /// The event a husk's `value` prints.
    ///
    /// # Errors
    /// Where any part of it is sealed, or it holds no part at all.
    fn read_dropped(value: &Value) -> Result<Self, ProdromeError>;

    /// The key each part was sealed under, in order.
    fn sealed_under(&self) -> impl Iterator<Item = &Self::Seal>;

    /// Whether a part of it is still sealed: a whole a holder could drop words from.
    fn holds_sealed(&self) -> bool {
        self.drop_sealed().is_some_and(|husk| husk != *self)
    }
}

/// An event with every sealed part dropped. Made only from an event ([`Husk::of`]) or from a print
/// that holds no sealed part ([`Husk::read`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Husk<E>(E);

/// A print that is no husk.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a husk that does not read: {0}")]
pub struct NotAHusk(String);

impl<E: Husking> Husk<E> {
    /// `event` with its sealed parts dropped; `None` for an event that holds none.
    #[must_use]
    pub fn of(event: &E) -> Option<Self> {
        event.drop_sealed().map(Self)
    }

    /// The event it reads as.
    #[must_use]
    pub fn event(&self) -> &E {
        &self.0
    }

    #[must_use]
    pub fn print(&self) -> String {
        print_literal(&self.0.to_value())
    }

    /// The husk `text` prints.
    ///
    /// # Errors
    /// [`NotAHusk`] for a print that is not one of the schema's events with every part dropped.
    pub fn read(text: &str) -> Result<Self, NotAHusk> {
        let value = parse_literal(text, &E::Husks::default()).map_err(refused)?;
        E::read_dropped(&value).map(Self).map_err(refused)
    }

    /// Whether a key `live` says is still kept opens a part of the whole it was.
    pub fn opens(&self, live: impl Fn(&E::Seal) -> bool) -> bool {
        self.0.sealed_under().any(live)
    }
}

/// A husk where it rests: the `Change` the whole was, its event husked. What a store keeps under
/// the whole's name, printed in the store's own syntax.
#[derive(Debug, Clone, PartialEq)]
pub struct HuskObject<E> {
    genesis: Hash,
    deps: Vec<Hash>,
    husk: Husk<E>,
}

/// `Change(genesis, deps, event)`, as the Prodrome prints it.
const CHANGE: Constructor = ("Change", &["genesis", "deps", "event"]);

/// A husk object's vocabulary: `Change` and the schema's husks.
struct ObjectVocabulary<V>(V);

impl<V: Vocabulary> Vocabulary for ObjectVocabulary<V> {
    fn signature(&self, name: &str) -> Option<Signature<'_>> {
        Table(&[CHANGE])
            .find(name)
            .or_else(|| self.0.signature(name))
    }
}

impl<E: Husking> HuskObject<E> {
    /// The husk of `whole`; `None` where its event holds no sealed part. Only a `Change` is
    /// husked: every other envelope a store writes (a `Genesis`, a key's, a `Snapshot`, a
    /// `Signed`) carries no event, and the legacy ones that do are written by nothing. The caller
    /// has verified `whole` against its name.
    #[must_use]
    pub fn of(change: &Change<E>) -> Option<Self> {
        Some(Self {
            genesis: change.genesis.clone(),
            deps: change.deps.clone(),
            husk: Husk::of(&change.event)?,
        })
    }

    #[must_use]
    pub fn husk(&self) -> &Husk<E> {
        &self.husk
    }

    /// The object it reads as, which every fold reads in place of the whole.
    #[must_use]
    pub fn object(&self) -> Envelope<E> {
        Envelope::Change(Change {
            genesis: self.genesis.clone(),
            deps: self.deps.clone(),
            event: self.husk.0.clone(),
        })
    }

    #[must_use]
    pub fn print(&self) -> String {
        canonical_envelope(&self.object())
    }

    /// The husk object `text` prints, kept under `name`.
    ///
    /// # Errors
    /// [`NotAHusk`] for a print that hashes to `name` (a whole), or that is not a `Change` whose
    /// event is a husk.
    pub fn read(name: &Hash, text: &str) -> Result<Self, NotAHusk> {
        if hex::encode(<sha2::Sha256 as sha2::Digest>::digest(text)) == name.as_str() {
            return Err(NotAHusk(format!("{} is its whole", name.as_str())));
        }
        let value = parse_literal(text, &ObjectVocabulary(E::Husks::default())).map_err(refused)?;
        let fields = Fields::of(&value, CHANGE).map_err(refused)?;
        let deps = fields
            .get::<Vec<String>>("deps")
            .map_err(refused)?
            .into_iter()
            .map(Hash::new)
            .collect::<Result<_, _>>()
            .map_err(refused)?;
        let genesis =
            Hash::new(fields.get::<String>("genesis").map_err(refused)?).map_err(refused)?;
        let event = prodrome_core::payload::required(fields.call(), "event").map_err(refused)?;
        let husk = Husk(E::read_dropped(event).map_err(refused)?);
        let Change { genesis, deps, .. } = mk_change(genesis, deps, ()).map_err(refused)?;
        Ok(Self {
            genesis,
            deps,
            husk,
        })
    }
}

fn refused(why: impl std::fmt::Display) -> NotAHusk {
    NotAHusk(why.to_string())
}
