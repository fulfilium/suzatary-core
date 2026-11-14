use crate::common::*;

fn stream_read() -> impl Strategy<Value = Value> {
    proptest::collection::vec((turn_row(0), prop::sample::select(vec![json!([]), json!([{"topic": "garden", "by": "Owner"}]), json!([{"topic": "garden", "by": "Owner"}, {"topic": "trip", "by": "Inferred"}])]), any::<bool>()), 0..4).prop_map(|rs| {
        let rounds: Vec<Value> = rs
            .into_iter()
            .enumerate()
            .map(|(n, (opening, topics, boxed))| {
                json!({"target": format!("round:c{n}/0"), "conversation": format!("c{n}"), "title": "", "turn": 0,
                    "opening": opening, "asked": if boxed { json!({"by": "box", "title": "The day's brief"}) } else { json!({"by": "person"}) },
                    "answer": null, "topics": topics, "room": 2})
            })
            .collect();
        let total = rounds.len() + 1;
        json!({"stream": {"rounds": rounds, "total": total}, "bar": {"topics": [{"topic": "garden", "rounds": 2, "disputed": []}, {"topic": "trip", "rounds": 1, "disputed": ["travel"]}]}})
    })
}

fn secretary_msg() -> impl Strategy<Value = secretary::Msg> {
    let host = |i: Input<secretary::Hold, secretary::Ask>| secretary::Msg::Host(Box::new(i));
    prop_oneof![
        held(secretary::Hold::Stream, stream_read()).prop_map(host),
        held(secretary::Hold::Found, Just(json!({"query": "zephyr", "hits": [{"conversation": "c0", "title": "", "kind": "topic", "turn": 0, "at": "2026-10-05T04:00:00", "side": "person", "excerpt": "when"}]}))).prop_map(host),
        held(secretary::Hold::Scroll, prop::sample::select(vec![json!({"by": "reader", "at": "above"}), json!({"by": "page", "at": "bottom"})])).prop_map(host),
        Just(secretary::Msg::Earlier),
        prop::sample::select(vec!["garden", "trip"]).prop_map(|t| secretary::Msg::Rename(t.to_owned())),
        Just(secretary::Msg::Add("round:c0/0".to_owned())),
        Just(secretary::Msg::Chip(suzatary_pages::runtime::TopicWrite::Remove { target: "round:c0/0".to_owned(), topic: "garden".to_owned() })),
        Just(secretary::Msg::Chip(suzatary_pages::runtime::TopicWrite::Add { target: "round:c0/0".to_owned(), topic: "trip".to_owned() })),
        prop::sample::select(vec!["", "Trip", "trip", "garden"]).prop_map(|t| secretary::Msg::Typed(t.to_owned())),
        Just(secretary::Msg::Write("trip".to_owned())),
        Just(secretary::Msg::Submit),
        Just(secretary::Msg::Close),
        prop::sample::select(vec!["", "zephyr", " zephyr "]).prop_map(|t| secretary::Msg::Searched(t.to_owned())),
        (0u32..4).prop_map(move |n| host(Input::Answered { ask: secretary::Ask::Search(chats::Asking::Rested(n)), answer: Answer::Done { body: json!(1_790_000_000_000_i64) } })),
        (prop::sample::select(vec![None, Some(("garden".to_owned(), "yard".to_owned()))]), outcome()).prop_map(move |(renamed, answer)| host(Input::Answered { ask: secretary::Ask::Topic(secretary::Wrote { renamed }), answer })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn secretary_laws(writes: bool, chosen: bool, msgs in proptest::collection::vec(secretary_msg(), 0..14)) {
        page_laws::<secretary::Secretary>(
            || secretary::Flags { context: context(writes), topics: if chosen { vec!["trip".to_owned(), "garden".to_owned()] } else { Vec::new() }, now: 1_790_000_000_000 },
            &msgs,
        );
    }
}
