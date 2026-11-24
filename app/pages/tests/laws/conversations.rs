use crate::common::*;

fn conversations_msg() -> impl Strategy<Value = conversations::Msg> {
    use conversations::{Hold, Msg};
    let host = |i: conversations::Input| Msg::Host(Box::new(i));
    prop_oneof![
        held(Hold::Chats, Just(json!({"conversations": [{"id": "talk", "title": "", "kind": "topic", "opened": "", "turns": 2, "standing": 1, "unplaced": [], "last": "hi", "last_at": "2026-10-05T04:00:00"}]}))).prop_map(host),
        held(Hold::Route, prop::sample::select(vec![json!({"page": "conversation", "conversation": "talk", "selected": "", "shared": ""}), json!({"page": "conversations", "conversation": null, "selected": "", "shared": ""})])).prop_map(host),
        held(Hold::Found, Just(json!({"hits": []}))).prop_map(host),
        prop::sample::select(vec!["", "zephyr"]).prop_map(|t| Msg::Searched(t.to_owned())),
        (1u32..3).prop_map(move |n| host(Input::Answered { ask: chats::Asking::Rested(n), answer: Answer::Done { body: Text::printed(&json!(null)) } })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn conversations_laws(writes: bool, msgs in proptest::collection::vec(conversations_msg(), 0..12)) {
        page_laws::<conversations::Conversations>(|| conversations::Flags { context: context(writes), now: 1_790_000_000_000 }, &msgs);
    }
}
