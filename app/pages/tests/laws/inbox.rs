use crate::cards::*;
use crate::common::*;

fn inbox_msg() -> impl Strategy<Value = inbox::Msg> {
    let host = |i: Input<inbox::Hold, inbox::Ask>| inbox::Msg::Host(Box::new(i));
    prop_oneof![
        watched().prop_flat_map(move |(w, v)| held(inbox::Hold::Rows(w), Just(v))).prop_map(host),
        held(inbox::Hold::Rows(Watched::Decisions("life".to_owned())), Just(json!({"at": "2026-10-05T04:00:00", "heads": [], "entries": [], "failed": [], "next": null}))).prop_map(host),
        held(inbox::Hold::Intents, Just(json!({"asked": [{"act": {"type": "Answer", "at": {"conversation": "setup", "index": 0, "from": "standing:0:"}, "answer": {"type": "Dismiss", "reason": "no"}}, "answer": null, "retracted": false}]}))).prop_map(host),
        card_ev().prop_map(|(k, ev)| inbox::Msg::Card(k, ev)),
        any::<bool>().prop_map(inbox::Msg::Setup),
        any::<bool>().prop_map(inbox::Msg::Settled),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn inbox_laws(writes: bool, opened: bool, msgs in proptest::collection::vec(inbox_msg(), 0..12)) {
        page_laws::<inbox::Inbox>(
            || inbox::Flags { context: context(writes), open: if opened { "p0".to_owned() } else { String::new() }, now: 1_790_000_000_000 },
            &msgs,
        );
    }
}
