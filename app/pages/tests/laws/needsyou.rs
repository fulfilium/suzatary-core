use crate::cards::*;
use crate::common::*;

fn needsyou_msg() -> impl Strategy<Value = needsyou::Msg> {
    let host = |i: needsyou::Input| needsyou::Msg::Host(Box::new(i));
    prop_oneof![
        inbox_read()
            .prop_flat_map(move |v| held(needsyou::Hold::Rows(Watched::Proposals), Just(v)))
            .prop_map(host),
        held(
            needsyou::Hold::Route,
            prop::sample::select(vec![
                json!({"page": "inbox", "conversation": ""}),
                json!({"page": "todos", "conversation": ""}),
                json!({"page": "conversation", "conversation": "talk"})
            ])
        )
        .prop_map(host),
        (0u8..4).prop_map(|n| needsyou::Msg::Close(format!("p{n}"))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn needsyou_laws(msgs in proptest::collection::vec(needsyou_msg(), 0..12)) {
        page_laws::<needsyou::NeedsYou>(|| needsyou::Flags { context: context(true) }, &msgs);
    }
}
