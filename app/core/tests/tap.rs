use suzatary_signatures::signatures::{ActHeaders, Seat, Tap};

fn triple() -> ActHeaders<'static> {
    ActHeaders {
        act: Some("1"),
        client: Some(suzatary_signatures::signatures::CLIENT),
        content_type: Some("application/json"),
        site: Some("same-origin"),
    }
}

#[test]
fn a_trusted_request_mints_a_tap_naming_its_actor() {
    let tap = Tap::of(
        &suzatary_core::stores::scene("ada"),
        &triple(),
        Seat::Device,
    )
    .expect("a tap");
    assert_eq!(tap.who().as_str(), "ada");
}

#[test]
fn a_write_without_the_apps_own_header_gets_no_tap() {
    let refused = Tap::of(
        &suzatary_core::stores::scene("ada"),
        &ActHeaders {
            act: None,
            ..triple()
        },
        Seat::Device,
    )
    .expect_err("no header, no tap");
    assert!(
        refused
            .message()
            .contains(suzatary_signatures::signatures::ACT_HEADER)
    );
}
