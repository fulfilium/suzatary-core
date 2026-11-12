//! A request the cast's phone signed, proven: test support, never production.

use prodrome_core::event::Actor;

use super::{Asking, Request, Trusted, body_digest, request_name, trust};
use crate::store::Prodrome as _;
use suzatary_record::record::TodoEvent;

/// A real [`Trusted`] for a scene: the cast's root adopting a chain in memory, the owner's phone
/// trusted on it, and a request the phone signed, through the one constructor.
///
/// # Panics
/// Never for the cast's keys.
#[must_use]
pub fn scene(actor: &str) -> Trusted {
    use crate::cast::keys;
    let actor = Actor::new(actor).expect("a roster name");
    let chain: crate::store::InMemory<TodoEvent> = crate::store::InMemory::default();
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
