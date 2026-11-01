//! The tap: the one door out of a conversation and into the world, minted only from a request
//! the door proved.
//!
//! A route is not generic over a capability record, so the function that sends takes a
//! `&Tap`. Its one constructor takes a [`Trusted`], a request signed by a device key the history
//! trusts for the gate's actor, which loopback never makes (it names no actor) and which no route
//! holds without the door having read the trust.

use prodrome_core::event::Actor;

use crate::gate::{ActHeaders, check_csrf};
use crate::refusal::Refusal;
use crate::seat::Seat;
use crate::secretary::sending::Sending;
use crate::secretary::store::Proposal;
use crate::signatures::Trusted;

/// A witness that the owner himself asked, from a device he trusts, not a permission for any
/// particular act.
///
/// It is deliberately not `Clone`, `Copy` or `Default`, so it cannot outlive its request, and
/// its field is private, so nothing builds one but [`Tap::of`]:
///
/// ```compile_fail
/// let tap = suzatary_core::secretary::tap::Tap {
///     who: prodrome_core::event::Actor::new("ada").unwrap(),
///     seat: suzatary_core::seat::Seat::Device,
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

    /// Whether this tap may perform `proposal`: the operator's tap performs no card that sends,
    /// because sending stays the owner's tap on his own device.
    ///
    /// # Errors
    /// [`Refusal::forbidden`] for the operator's tap on a card that sends.
    pub fn performs(&self, proposal: &Proposal) -> Result<(), Refusal> {
        match (self.seat, Sending::of(proposal)) {
            (Seat::Operator, Sending::Sends) => Err(Refusal::forbidden(
                "this card sends mail, and sending stays the owner's tap on his own device: \
                 the operator at the box's terminal may not perform it",
            )),
            (Seat::Device, Sending::Sends | Sending::SendsNothing)
            | (Seat::Operator, Sending::SendsNothing) => Ok(()),
        }
    }

    /// Who tapped it: the deployment's mapping of the peer, never a request field.
    #[must_use]
    pub fn who(&self) -> &Actor {
        &self.who
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triple() -> ActHeaders<'static> {
        ActHeaders {
            act: Some("1"),
            client: Some(crate::gate::CLIENT),
            content_type: Some("application/json"),
            site: Some("same-origin"),
        }
    }

    #[test]
    fn a_trusted_request_mints_a_tap_naming_its_actor() {
        let tap =
            Tap::of(&crate::signatures::scene("ada"), &triple(), Seat::Device).expect("a tap");
        assert_eq!(tap.who().as_str(), "ada");
    }

    /// A minted tap builds the device flow's `Consent`, whose request names the IMAP scope.
    #[test]
    fn a_minted_tap_is_a_consent_for_the_device_flow() {
        let tap =
            Tap::of(&crate::signatures::scene("ada"), &triple(), Seat::Device).expect("a tap");
        let consent = crate::mail::oauth::Consent::given_by_tap(&tap);
        let request = crate::mail::oauth::device_request::<crate::mail::oauth::Imap>(
            &consent,
            crate::mail::account::Authority::Organizations,
        );
        assert!(request.url.contains("devicecode"), "{}", request.url);
        assert!(
            request
                .form
                .iter()
                .any(|(key, value)| key == "scope" && value.contains("IMAP")),
            "{:?}",
            request.form
        );
    }

    #[test]
    fn a_write_without_the_apps_own_header_gets_no_tap() {
        let refused = Tap::of(
            &crate::signatures::scene("ada"),
            &ActHeaders {
                act: None,
                ..triple()
            },
            Seat::Device,
        )
        .expect_err("no header, no tap");
        assert!(refused.message().contains(crate::gate::ACT_HEADER));
    }
}
