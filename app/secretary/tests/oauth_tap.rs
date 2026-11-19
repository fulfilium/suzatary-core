//! A tap minted by the gate is the consent the device flow asks for.

use suzatary_mail::mail::account::Authority;
use suzatary_mail::mail::oauth::{Consent, Imap, device_request};

/// A minted tap builds the device flow's `Consent`, whose request names the IMAP scope.
#[test]
fn a_minted_tap_is_a_consent_for_the_device_flow() {
    let tap = suzatary_signatures::signatures::Tap::of(
        &suzatary_secretary::stores::scene("ada"),
        &suzatary_signatures::signatures::ActHeaders {
            act: Some("1"),
            client: Some(suzatary_signatures::signatures::CLIENT),
            content_type: Some("application/json"),
            site: Some("same-origin"),
        },
        suzatary_signatures::signatures::Seat::Device,
    )
    .expect("a tap");
    let consent = Consent::given_by_tap(&tap);
    let request = device_request::<Imap>(&consent, Authority::Organizations);
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
