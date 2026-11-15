//! A keeper's final refusal of a request a replica posted, keyed by the request's name. Generic
//! over the store-name type, so it can move to the Prodrome. Two refusals of one are both read.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fmt::Debug;

use prodrome_core::event::{Actor, Envelope, Hash};
use prodrome_core::fold::sole::{One, Only, Whole};
use prodrome_core::literal::codec::{Constructor, Fields, Literal};
use prodrome_core::literal::{Datetime, ProdromeError, Value};
use prodrome_core::schema::Schema as _;
use prodrome_core::schema::headed::{Body, Constructors, Event};

/// The request `key` refused for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused<S> {
    /// Its store.
    pub store: S,
    /// The keeper's sentence, shown verbatim.
    pub reason: String,
}

/// A refusal event: `Refused(request, at, actor, store, reason)`.
pub type RefusalEvent<S> = Event<Refused<S>>;

/// The schema's whitelist.
pub type RefusalVocabulary<S> = Constructors<Refused<S>>;

const REFUSED: Constructor = ("Refused", &["request", "at", "actor", "store", "reason"]);

/// The keeper's refusal of `request`, posted to `store`.
#[must_use]
pub fn refused<S>(
    request: Hash,
    at: Datetime,
    actor: Actor,
    store: S,
    reason: String,
) -> RefusalEvent<S>
where
    S: Literal + Clone + PartialEq + Debug + Send + Sync + 'static,
{
    Event::of(request, at, actor, Refused { store, reason })
}

impl<S> Body for Refused<S>
where
    S: Literal + Clone + PartialEq + Debug + Send + Sync + 'static,
{
    type Key = Hash;
    type Register = Only;
    type Registers<'a> = One<'a, Whole<RefusalEvent<S>>>;
    type Refusal = Infallible;

    const EVENTS: &'static [Constructor] = &[REFUSED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        (REFUSED, vec![self.store.to_value(), self.reason.to_value()])
    }

    fn parse(fields: &Fields<'_>) -> Result<RefusalEvent<S>, ProdromeError> {
        match fields.name() {
            "Refused" => Ok(refused(
                fields.get("request")?,
                fields.get("at")?,
                fields.get("actor")?,
                fields.get("store")?,
                fields.get("reason")?,
            )),
            other => Err(prodrome_core::literal::codec::no_arm("a refusal", other)),
        }
    }
}

/// Every request of `store` that `objects` refuse, two refusals of one joined in name order.
pub fn refusals<'a, S>(
    objects: impl IntoIterator<Item = (&'a Hash, &'a Envelope<RefusalEvent<S>>)>,
    store: &S,
) -> BTreeMap<Hash, String>
where
    S: Literal + Clone + PartialEq + Debug + Send + Sync + 'static,
{
    let mut named: BTreeMap<&Hash, &RefusalEvent<S>> = BTreeMap::new();
    for (name, envelope) in objects {
        if let Some(event) = envelope.event()
            && event.body().store == *store
        {
            named.insert(name, event);
        }
    }
    let mut said: BTreeMap<Hash, String> = BTreeMap::new();
    for event in named.into_values() {
        said.entry(event.key().clone())
            .and_modify(|reason| {
                reason.push_str("; ");
                reason.push_str(&event.body().reason);
            })
            .or_insert_with(|| event.body().reason.clone());
    }
    said
}
