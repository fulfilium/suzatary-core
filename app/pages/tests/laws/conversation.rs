use crate::cards::*;
use crate::common::*;

fn windowed() -> impl Strategy<Value = Value> {
    prop_oneof![
        3 => (proptest::collection::vec(any::<bool>(), 0..5), conversation_cards("talk"), prop::sample::select(vec!["idle", "thinking"])).prop_flat_map(|(shape, cards, state)| {
            let rows: Vec<_> = (0..shape.len()).map(turn_row).collect();
            rows.prop_map(move |turns| {
                let rounds: Vec<Value> = (0..turns.len())
                    .map(|i| json!({"key": i, "startIndex": i, "endIndex": i, "opening": i, "trace": [], "answer": null,
                        "proposals": if i == 0 { json!([0, 1]) } else { json!([]) }, "earlier": [], "day": "2026-10-05", "label": "Monday 5 October", "marked": i == 0}))
                    .collect();
                let total = turns.len();
                let mut c = cards.clone();
                c["turns"] = json!(turns);
                c["rounds"] = json!(rounds);
                c["total"] = json!(total);
                c["state"] = json!(state);
                c["again"] = if total > 0 { json!(0) } else { json!(null) };
                json!({"type": "Read", "payload": c})
            })
        }),
        1 => Just(json!({"type": "NotHere", "lines": 3, "why": "offline"})),
        1 => Just(json!({"type": "Unheard"})),
    ]
}

fn frame() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        r#"{"kind":"state","state":"thinking"}"#,
        r#"{"kind":"working","verb":"search"}"#,
        r#"{"kind":"delta","text":"Hi\n\nthere"}"#,
        r#"{"kind":"block","n":0,"sha":"ab","source":"Hi\n\n"}"#,
        r#"{"kind":"turn"}"#,
        r#"{"kind":"state","state":"idle"}"#,
        "not a frame",
    ])
    .prop_map(str::to_owned)
}

fn uploaded() -> impl Strategy<Value = Answer> {
    prop::sample::select(vec![
        Answer::Done {
            body: json!({"kind": "kept", "file": {"id": "f1", "media_type": "application/pdf", "name": "w2.pdf", "private": ""}}),
        },
        Answer::Done {
            body: json!({"kind": "sensitive", "why": "its text names an identity number", "folder": "/my-files/Suzatary/Private"}),
        },
        Answer::Refused {
            status: 413,
            words: "too big".to_owned(),
        },
    ])
}

fn conversation_msg() -> impl Strategy<Value = conversation::Msg> {
    use conversation::{Ask, Hold, Msg};
    let host = |i: conversation::Input| Msg::Host(Box::new(i));
    prop_oneof![
        (reading(windowed()), behind()).prop_map(move |(reading, behind)| host(Input::Held { ask: Hold::Window, reading, behind })),
        frame().prop_map(move |frame| host(Input::Heard { ask: conversation::Stream::Conversation, frame })),
        held(Hold::Draft, prop::sample::select(vec![json!({"text": "", "others": []}), json!({"text": "draft", "others": ["elsewhere"]})])).prop_map(host),
        held(Hold::Scroll, prop::sample::select(vec![json!({"by": "reader", "at": "above"}), json!({"by": "reader", "at": "bottom"}), json!({"by": "page", "at": "above"})])).prop_map(host),
        held(Hold::Block("ab".to_owned()), Just(json!({"html": "<p>Hi</p>", "key": "k"}))).prop_map(host),
        held(Hold::Chats, Just(json!({"conversations": [{"id": "talk", "title": "", "kind": "topic", "opened": "", "turns": 2, "standing": 1, "unplaced": [], "last": "hi", "last_at": "2026-10-05T04:00:00"}]}))).prop_map(host),
        composer_ev().prop_map(Msg::Composer),
        card_ev().prop_map(|(k, ev)| Msg::Card(k, ev)),
        Just(Msg::Quote("a line".to_owned())),
        Just(Msg::Earlier),
        Just(Msg::Pill),
        Just(Msg::Stop),
        Just(Msg::Regenerate),
        Just(Msg::Edit),
        (0usize..3).prop_map(|t| Msg::Erasing(Some(t))),
        Just(Msg::Erasing(None)),
        (0usize..3).prop_map(Msg::Erase),
        Just(Msg::Forget(3)),
        Just(Msg::Rename),
        Just(Msg::Renamed("A name".to_owned())),
        prop::option::of(Just("A name".to_owned())).prop_map(Msg::Named),
        prop::sample::select(vec!["", "zephyr"]).prop_map(|t| Msg::Searched(t.to_owned())),
        outcome().prop_map(move |answer| host(Input::Answered { ask: Ask::Said(Box::new(conversation::Spoken { text: "hello".to_owned(), references: Vec::new(), files: Vec::new(), edit: false })), answer })),
        (prop::sample::select(vec!["A name", "Another"]), outcome()).prop_map(move |(title, answer)| host(Input::Answered { ask: Ask::Rename { title: title.to_owned() }, answer })),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered { ask: Ask::Composer(composer::Asking::Upload(n)), answer })),
        (1u32..3, uploaded()).prop_map(move |(n, answer)| host(Input::Answered { ask: Ask::Composer(composer::Asking::Keep(n)), answer })),
        Just(host(Input::Answered { ask: Ask::Slice, answer: Answer::Done { body: json!(1_790_000_005_000_i64) } })),
        Just(host(Input::Opened { ask: conversation::Stream::Conversation })),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn conversation_laws(writes: bool, panel: bool, msgs in proptest::collection::vec(conversation_msg(), 0..16)) {
        let at = if panel {
            conversation::At::Panel { about: "todo:life/renewcard".to_owned() }
        } else {
            conversation::At::Page { id: "talk".to_owned(), refs: vec!["todo:life/renewcard".to_owned()], focus: true, at: None }
        };
        page_laws::<conversation::Conversation>(|| conversation::Flags { context: context(writes), at: at.clone(), now: 1_790_000_000_000 }, &msgs);
    }
}
