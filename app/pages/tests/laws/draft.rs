use crate::common::*;

fn draft_msg() -> impl Strategy<Value = draft::Msg> {
    let host = |i: draft::Input| draft::Msg::Host(Box::new(i));
    prop_oneof![
        composer_ev().prop_map(draft::Msg::Composer),
        prop::sample::select(vec!["", "The zephyr filing"])
            .prop_map(|t| draft::Msg::Named(t.to_owned())),
        prop::sample::select(vec![
            Answer::Done {
                body: json!({"id": "2026-10-05-1"})
            },
            Answer::Refused {
                status: 403,
                words: "no".to_owned()
            }
        ])
        .prop_map(move |answer| host(Input::Answered {
            ask: draft::Ask::Open,
            answer
        })),
        outcome().prop_map(move |answer| host(Input::Answered {
            ask: draft::Ask::Said("2026-10-05-1".to_owned()),
            answer
        })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn draft_laws(writes: bool, msgs in proptest::collection::vec(draft_msg(), 0..12)) {
        page_laws::<draft::Draft>(|| draft::Flags { context: context(writes) }, &msgs);
    }
}
