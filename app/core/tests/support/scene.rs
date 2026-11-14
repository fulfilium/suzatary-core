//! A request the cast's phone signed, proven, and a chain trusting the phone: test support,
//! never production.

use prodrome_core::event::Actor;

use suzatary_record::record::TodoEvent;
use suzatary_signatures::signatures::{Asking, Request, Trusted, body_digest, request_name, trust};
use suzatary_store::store::Prodrome as _;

/// A real [`Trusted`] for a scene: the cast's root adopting a chain in memory, the owner's phone
/// trusted on it, and a request the phone signed, through the one constructor.
///
/// # Panics
/// Never for the cast's keys.
#[must_use]
pub fn scene(actor: &str) -> Trusted {
    use crate::cast::keys;
    let actor = Actor::new(actor).expect("a roster name");
    let chain: super::InMemory<TodoEvent> = super::InMemory::default();
    let begun = prodrome_core::event::mk_completed(
        "scene",
        prodrome_core::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
        actor.as_str(),
        "",
    )
    .expect("an event");
    chain.append(begun).expect("begins");
    let phone = keys::secret(keys::PHONE);
    trust(&chain, &keys::secret(keys::ROOT), &actor, &phone.public())
        .expect("the phone is trusted");
    let (at, nonce, body) = (
        "2026-09-02T12:00:00.000Z",
        "0123456789abcdef0123456789abcdef",
        b"{}",
    );
    let name = request_name(
        "POST",
        "/api/scene",
        at,
        nonce,
        &body_digest(body),
        phone.public().as_str(),
    );
    let signed = phone.sign(&name);
    let request = Request {
        method: "POST",
        target: "/api/scene",
        at: Some(at),
        nonce: Some(nonce),
        body,
        key: Some(signed.key.as_str()),
        signature: Some(signed.signature.as_str()),
    };
    let now = chrono::DateTime::parse_from_rfc3339(at)
        .expect("an instant")
        .with_timezone(&chrono::Utc);
    Asking::of(&request, actor, now)
        .expect("a well-formed request")
        .trusted(&[&chain], &keys::root())
        .expect("the phone is trusted")
}

/// A chain in memory whose root, the cast's, trusts the cast's phone for each of `actors`.
///
/// # Panics
/// Never for the cast's keys.
#[must_use]
pub fn phone_trusted(actors: &[&str]) -> crate::stores::InMemory<TodoEvent> {
    use crate::cast::keys;
    let chain: crate::stores::InMemory<TodoEvent> = crate::stores::InMemory::default();
    let begun = prodrome_core::event::mk_completed(
        "scene",
        prodrome_core::literal::Datetime::new(2026, 9, 2, 12, 0, 0, 0).expect("an instant"),
        "scene",
        "",
    )
    .expect("an event");
    chain.append(begun).expect("begins");
    for actor in actors {
        trust(
            &chain,
            &keys::secret(keys::ROOT),
            &Actor::new(*actor).expect("a roster name"),
            &keys::secret(keys::PHONE).public(),
        )
        .expect("the phone is trusted");
    }
    chain
}
