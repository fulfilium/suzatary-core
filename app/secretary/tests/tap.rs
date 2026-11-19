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
        &suzatary_secretary::stores::scene("ada"),
        &triple(),
        Seat::Device,
    )
    .expect("a tap");
    assert_eq!(tap.who().as_str(), "ada");
}

#[test]
fn a_write_without_the_apps_own_header_gets_no_tap() {
    let refused = Tap::of(
        &suzatary_secretary::stores::scene("ada"),
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

/// The tap's rule holds on a standing proposal, performs once under the inbox's Decision, and
/// its recorded response discharges it: a second tap, and the rule after a restart, perform
/// nothing.
#[test]
fn a_tap_performs_once_and_its_response_discharges_the_rule() {
    use prodrome_core::literal::Datetime;
    use suzatary_base::conversation_id::ConversationId;
    use suzatary_base::reference::ProposalRef;
    use suzatary_inbox::secretary::inbox;
    use suzatary_inbox::secretary::inbox::Response;
    use suzatary_inbox::secretary::inbox_event::InboxEvent;
    use suzatary_inbox::secretary::proposal::{Draft, Proposal, Reasons};
    use suzatary_inbox::secretary::responded::{Done, Performed};
    use suzatary_inbox::secretary::tapped::Tapped;
    use suzatary_secretary::stores::InMemory;
    use suzatary_triage::rule::Rule as _;

    let now = Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant");
    let store = InMemory::<InboxEvent>::default();
    let asked = inbox::Asked {
        proposer: suzatary_inbox::secretary::proposer::Proposer::Secretary,
        proposal: Proposal::Email(Draft::default()),
        reasons: Reasons::new(
            "He asked for the scan.",
            vec!["mail:a1b2c3d4e5f6".parse().expect("a reference")],
        )
        .expect("reasons"),
        asked: ProposalRef {
            conversation: ConversationId::new("default").expect("a slug"),
            index: 0,
        },
        price: inbox::Price::unpriced(),
    };
    let id = inbox::ask(
        &store,
        asked,
        &suzatary_lines::secretary::transcript::writer(),
        now,
    )
    .expect("filed");
    let rule = Tapped { now };
    let standing = |store: &InMemory<InboxEvent>| {
        inbox::fold(store)
            .expect("reads")
            .get(&id)
            .expect("held")
            .clone()
    };
    assert!(rule.holds(&standing(&store)));

    let tap = Tap::of(
        &suzatary_secretary::stores::scene("ada"),
        &triple(),
        Seat::Device,
    )
    .expect("a tap");
    let mut performed = 0;
    let mut send = |_: &inbox::Asked| {
        performed += 1;
        Ok(Performed::Done(Done::Sent {
            recipients: vec!["a@b.test".to_owned()],
            message_id: "<x@box.example>".to_owned(),
        }))
    };
    assert!(matches!(
        rule.fire(&store, &id, &tap, &mut send).expect("taps"),
        Response::Ran(_)
    ));
    assert!(matches!(
        rule.fire(&store, &id, &tap, &mut send).expect("taps"),
        Response::Recorded(_)
    ));
    assert_eq!(performed, 1);
    assert!(!rule.holds(&standing(&store)));
    assert!(!Tapped { now }.holds(&standing(&store)));
}
