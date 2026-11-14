//! `Object::is_signature` reads the print without a parse; it must say what the envelope's
//! parse says, for every kind of envelope and for a print that does not parse.

use std::convert::Infallible;

use prodrome_core::change::mk_change;
use prodrome_core::event::{
    Actor, Envelope, Hash, canonical_envelope, mk_sealed, mk_woven, parse_envelope,
};
use prodrome_core::fold::sole::{One, Only, Whole};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::codec::{Constructor, Fields};
use prodrome_core::literal::{Datetime, ProdromeError, Value};
use prodrome_core::schema::headed::{Body, Constructors, Event};
use prodrome_core::sign::{KeyAdded, Secret, mk_key_revoked};
use prodrome_core::snapshot::mk_snapshot;
use suzatary_store::replica::objects::Object;

/// The least schema: a note on a key.
#[derive(Debug, Clone, PartialEq)]
struct Noted {
    text: String,
}

type Note = Event<Noted>;

const NOTED: Constructor = ("Noted", &["key", "at", "actor", "text"]);

impl Body for Noted {
    type Key = Hash;
    type Register = Only;
    type Registers<'a> = One<'a, Whole<Note>>;
    type Refusal = Infallible;

    const EVENTS: &'static [Constructor] = &[NOTED];

    fn writes(&self) -> impl Iterator<Item = Only> {
        std::iter::once(Only)
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        (NOTED, vec![Value::str(&self.text)])
    }

    fn parse(fields: &Fields<'_>) -> Result<Note, ProdromeError> {
        Ok(Event::of(
            fields.get("key")?,
            fields.get("at")?,
            fields.get("actor")?,
            Noted {
                text: fields.get("text")?,
            },
        ))
    }
}

fn name(n: u8) -> Hash {
    Hash::new(format!("{n:02x}").repeat(32)).expect("64 hex")
}

fn note() -> Note {
    let at = Datetime::new(2026, 11, 14, 8, 0, 0, 0).expect("an instant");
    let actor = Actor::new("ada").expect("an actor");
    Event::of(
        name(9),
        at,
        actor,
        Noted {
            text: "a note".to_owned(),
        },
    )
}

/// One envelope of each kind, the match naming every kind so a new one fails to compile here.
fn every_kind() -> Vec<Envelope<Note>> {
    let key = Secret::new(&"07".repeat(32)).expect("a seed").public();
    let actor = Actor::new("ada").expect("an actor");
    let all = vec![
        mk_sealed(None, note()),
        mk_woven(vec![name(1), name(2)], Some(note())).expect("two parents"),
        Envelope::Genesis(mk_genesis("notes", &"0".repeat(32)).expect("a genesis")),
        Envelope::Change(mk_change(name(1), vec![name(2)], note()).expect("a change")),
        Envelope::Snapshot(mk_snapshot(name(1), vec![name(2)], None).expect("a snapshot")),
        Envelope::Signed(
            Secret::new(&"07".repeat(32))
                .expect("a seed")
                .sign(&name(3)),
        ),
        Envelope::KeyAdded(KeyAdded {
            genesis: name(1),
            actor: actor.clone(),
            key: key.clone(),
        }),
        Envelope::KeyRevoked(
            mk_key_revoked(name(1), vec![name(2)], actor, key).expect("a revocation"),
        ),
    ];
    for envelope in &all {
        match envelope {
            Envelope::Sealed { .. }
            | Envelope::Woven { .. }
            | Envelope::Genesis(_)
            | Envelope::Change(_)
            | Envelope::Snapshot(_)
            | Envelope::Signed(_)
            | Envelope::KeyAdded(_)
            | Envelope::KeyRevoked(_) => {}
        }
    }
    all
}

#[test]
fn a_signature_is_what_the_envelope_parse_says() {
    let schema = Constructors::<Noted>::default();
    let unparsed = [
        "Signed(object='x')".to_owned(),
        "Signed(".to_owned(),
        "Change(".to_owned(),
    ];
    let kinds = every_kind();
    let prints = kinds.iter().map(canonical_envelope).chain(unparsed);
    for print in prints {
        let parsed = matches!(
            parse_envelope::<Note>(&schema, &print),
            Ok(Envelope::Signed(_))
        );
        let object = Object::read::<Note>(&schema, print.clone());
        assert_eq!(object.is_signature(), parsed, "{print}");
    }
}
