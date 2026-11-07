use crate::common::*;

fn typst_msg() -> impl Strategy<Value = typstlist::Msg> {
    use typstlist::{Ask, Hold, Msg};
    let host = |i: Input<Hold, Ask>| Msg::Host(Box::new(i));
    let route = prop::sample::select(vec!["", "a"]).prop_map(
        |selected| json!({"page": "typst", "conversation": "", "selected": selected, "shared": ""}),
    );
    prop_oneof![
        held::<Hold, Ask, suzatary_pages::runtime::Never>(Hold::Fold, list_fold()).prop_map(host),
        held::<Hold, Ask, suzatary_pages::runtime::Never>(Hold::Route, route).prop_map(host),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
        prop::sample::select(vec![
            "open todo:life/a",
            "complete todo:life/a",
            "complete todo:work/a",
            "rm -rf"
        ])
        .prop_map(|w| Msg::Intent(w.to_owned())),
        Just(host(Input::Answered {
            ask: Ask::Act {
                todo: "a".to_owned(),
                acting: suzatary_pages::todoparts::Acting::Verb(Verb::Done),
                phase: suzatary_pages::todoparts::Phase::Sealed(Seal::Event)
            },
            answer: Answer::Done {
                body: json!({"type": "Answered", "answer": {"type": "Taken"}, "held": false})
            },
        })),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn typst_laws(writes: bool, msgs in proptest::collection::vec(typst_msg(), 0..12)) {
        page_laws::<typstlist::Typst>(|| typstlist::Flags { context: context(writes), chain: "life".to_owned() }, &msgs);
    }
}
