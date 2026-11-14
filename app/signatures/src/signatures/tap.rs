//! The tap: the one door out of a conversation and into the world, minted only from a request
//! the door proved.
//!
//! A route is not generic over a capability record, so the function that sends takes a
//! `&Tap`. Its one constructor takes a [`Trusted`], a request signed by a device key the history
//! trusts for the gate's actor, which loopback never makes (it names no actor) and which no route
//! holds without the door having read the trust.

use prodrome_core::event::Actor;

use super::{ActHeaders, Seat, Trusted, check_csrf};
use suzatary_base::refusal::Refusal;

/// A witness that the owner himself asked, from a device he trusts, not a permission for any
/// particular act.
///
/// It is deliberately not `Clone`, `Copy` or `Default`, so it cannot outlive its request, and
/// its field is private, so nothing builds one but [`Tap::of`]:
///
/// ```compile_fail
/// let tap = suzatary_core::signatures::Tap {
///     who: prodrome_core::event::Actor::new("ada").unwrap(),
///     seat: suzatary_core::signatures::Seat::Device,
/// };
/// ```
#[derive(Debug)]
pub struct Tap {
    who: Actor,
    seat: Seat,
}

impl Tap {
    /// Mints one from a request the door proved, made from `seat`, checking the CSRF triple:
    /// whom it is for is the actor the request was proven for.
    ///
    /// # Errors
    /// [`Refusal::forbidden`] for a request without the app's own headers.
    pub fn of(asked: &Trusted, headers: &ActHeaders<'_>, seat: Seat) -> Result<Self, Refusal> {
        check_csrf(headers)?;
        Ok(Self {
            who: asked.proven().actor().clone(),
            seat,
        })
    }

    /// Where it was tapped.
    #[must_use]
    pub fn seat(&self) -> Seat {
        self.seat
    }

    /// Who tapped it: the deployment's mapping of the peer, never a request field.
    #[must_use]
    pub fn who(&self) -> &Actor {
        &self.who
    }
}
