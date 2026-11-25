//! The tap: the one door out of a conversation into the world; the function that sends takes a
//! `&Tap`, minted only from a [`Trusted`] request, which loopback never makes.

use prodrome_data::event::Actor;

use super::{ActHeaders, Seat, Trusted, check_csrf};
use suzatary_base::refusal::Refusal;

/// A witness that the owner himself asked, from a device he trusts, not a permission for any
/// particular act.
///
/// It is deliberately not `Clone`, `Copy` or `Default`, so it cannot outlive its request, and
/// its field is private, so nothing builds one but [`Tap::of`]:
///
/// ```compile_fail
/// let tap = suzatary_signatures::signatures::Tap {
///     who: prodrome_data::event::Actor::new("ada").unwrap(),
///     seat: suzatary_signatures::signatures::Seat::Device,
/// };
/// ```
#[derive(Debug)]
pub struct Tap {
    who: Actor,
    seat: Seat,
}

impl Tap {
    /// Mints one for the actor the request was proven for, checking the CSRF triple.
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

    #[must_use]
    pub fn seat(&self) -> Seat {
        self.seat
    }

    /// The deployment's mapping of the peer, never a request field.
    #[must_use]
    pub fn who(&self) -> &Actor {
        &self.who
    }
}
