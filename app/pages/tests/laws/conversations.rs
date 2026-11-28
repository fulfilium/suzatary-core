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

/// A search is listened to from the moment its typing rests until its words change or go, and
/// before it rests, and after it ends, nothing reads it.
#[test]
fn a_search_is_listened_to_between_its_rest_and_its_end() {
    use conversations::{Hold, Msg};
    let (mut rt, _) = Runtime::<conversations::Conversations>::start(conversations::Flags {
        context: context(true),
        now: 1_790_000_000_000,
    });
    let found = Slot::Held(Hold::Found);
    let rested = |n| {
        Msg::Host(Box::new(Input::Answered {
            ask: chats::Asking::Rested(n),
            answer: Answer::Done {
                body: Text::printed(&json!(null)),
            },
        }))
    };
    let searched = |words: &str| Msg::Searched(words.to_owned());
    rt.send(searched("zephyr"));
    assert_eq!(rt.active().get(&found), None, "a typing waits for its rest");
    rt.send(rested(1));
    assert!(matches!(rt.active().get(&found), Some(Sub::Watch { .. })));
    rt.send(searched("zephyrs"));
    assert_eq!(
        rt.active().get(&found),
        None,
        "new words stop the old reading"
    );
    rt.send(rested(2));
    assert!(matches!(rt.active().get(&found), Some(Sub::Watch { .. })));
    rt.send(searched(""));
    assert_eq!(
        rt.active().get(&found),
        None,
        "a search that ends reads nothing"
    );
}
