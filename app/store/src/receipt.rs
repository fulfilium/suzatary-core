//! A keeper's refusal for good of a request a replica posted, keyed by the request's name: final,
//! or outdated, sealed under rules the keeper no longer seals by. Generic over the store-name type,
//! so it can move to the Prodrome. Two refusals of one are both read.

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
    /// Why it is held back.
    pub grounds: Grounds,
    /// The keeper's sentence, shown verbatim.
    pub reason: String,
}

/// Why a request is held back for good; each is a constructor of its own, since a shipped one's
/// fields are frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grounds {
    /// Never taken as it is: `Refused(…)`.
    Final,
    /// Sealed under rules older than the keeper's, so a newer writer seals it again:
    /// `Outdated(…)`.
    Outdated,
}

impl Grounds {
    /// The kind a replica's writes hold it as.
    #[must_use]
    pub fn kind(self) -> crate::replica::writes::Refusal {
        match self {
            Grounds::Final => crate::replica::writes::Refusal::Final,
            Grounds::Outdated => crate::replica::writes::Refusal::Outdated,
        }
    }
}

/// A refusal event: `Refused(request, at, actor, store, reason)`.
pub type RefusalEvent<S> = Event<Refused<S>>;

/// The schema's whitelist.
pub type RefusalVocabulary<S> = Constructors<Refused<S>>;

const REFUSED: Constructor = ("Refused", &["request", "at", "actor", "store", "reason"]);
const OUTDATED: Constructor = ("Outdated", &["request", "at", "actor", "store", "reason"]);

/// The keeper's refusal of `request`, posted to `store`, on `grounds`.
#[must_use]
pub fn refused<S>(
    request: Hash,
    at: Datetime,
    actor: Actor,
    (store, grounds): (S, Grounds),
    reason: String,
) -> RefusalEvent<S>
where
    S: Literal + Clone + PartialEq + Debug + Send + Sync + 'static,
{
    Event::of(
        request,
        at,
        actor,
        Refused {
            store,
            grounds,
            reason,
        },
    )
}

impl<S> Body for Refused<S>
where
    S: Literal + Clone + PartialEq + Debug + Send + Sync + 'static,
{
    const NAME: &'static str = "Refused";
    type Key = Hash;
    type Register = Only;
    type Registers<'a> = One<'a, Whole<RefusalEvent<S>>>;
    type Refusal = Infallible;

    const EVENTS: &'static [Constructor] = &[REFUSED, OUTDATED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        let constructor = match self.grounds {
            Grounds::Final => REFUSED,
            Grounds::Outdated => OUTDATED,
        };
        (
            constructor,
            vec![self.store.to_value(), self.reason.to_value()],
        )
    }

    fn parse(fields: &Fields<'_>) -> Result<RefusalEvent<S>, ProdromeError> {
        let grounds = match fields.name() {
            "Refused" => Grounds::Final,
            "Outdated" => Grounds::Outdated,
            other => return Err(prodrome_core::literal::codec::no_arm(Self::NAME, other)),
        };
        Ok(refused(
            fields.get("request")?,
            fields.get("at")?,
            fields.get("actor")?,
            (fields.get("store")?, grounds),
            fields.get("reason")?,
        ))
    }
}

/// Every request of `store` that `objects` refuse, two refusals of one joined in name order, on
/// the first one's grounds.
pub fn refusals<'a, S>(
    objects: impl IntoIterator<Item = (&'a Hash, &'a Envelope<RefusalEvent<S>>)>,
    store: &S,
) -> BTreeMap<Hash, (Grounds, String)>
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
    let mut said: BTreeMap<Hash, (Grounds, String)> = BTreeMap::new();
    for event in named.into_values() {
        let body = event.body();
        said.entry(event.key().clone())
            .and_modify(|(_, reason)| {
                reason.push_str("; ");
                reason.push_str(&body.reason);
            })
            .or_insert_with(|| (body.grounds, body.reason.clone()));
    }
    said
}

#[cfg(test)]
mod tests {
    use prodrome_core::event::{Envelope, canonical, parse_event};

    use super::*;

    /// Each grounds prints under its own constructor and reads back to itself, so the shipped
    /// `Refused` keeps its fields and an `Outdated` is told from it by name alone.
    #[test]
    fn each_grounds_reads_back_under_its_own_constructor() {
        let request = Hash::new("a".repeat(64)).expect("a name");
        let at = Datetime::new(2026, 11, 17, 9, 0, 0, 0).expect("an instant");
        let actor = Actor::new("box").expect("an actor");
        for (grounds, constructor) in [
            (Grounds::Final, "Refused("),
            (Grounds::Outdated, "Outdated("),
        ] {
            let event = refused(
                request.clone(),
                at,
                actor.clone(),
                ("lines".to_owned(), grounds),
                "older rules".to_owned(),
            );
            let print = canonical(&event);
            assert!(print.starts_with(constructor), "{print}");
            let back: RefusalEvent<String> =
                parse_event(&RefusalVocabulary::<String>::default(), &print).expect("parses");
            assert_eq!(back, event);
            let genesis = Hash::new("b".repeat(64)).expect("a name");
            let change =
                prodrome_core::change::mk_change(genesis, Vec::new(), back).expect("a change");
            let objects = [(request.clone(), Envelope::Change(change))];
            let said = refusals(objects.iter().map(|(n, o)| (n, o)), &"lines".to_owned());
            assert_eq!(
                said.get(&request),
                Some(&(grounds, "older rules".to_owned()))
            );
        }
    }
}
