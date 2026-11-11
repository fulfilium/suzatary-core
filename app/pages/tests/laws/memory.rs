use crate::common::*;

fn said() -> impl Strategy<Value = SaidOn> {
    prop_oneof![
        (0u32..3).prop_map(|turn| SaidOn::Turn {
            conversation: "c1".to_owned(),
            turn
        }),
        (0u32..3).prop_map(|proposal| SaidOn::Tapped {
            conversation: "c 2".to_owned(),
            proposal
        }),
    ]
}

fn memory_payload() -> impl Strategy<Value = Value> {
    let kind = prop_oneof![
        Just(Kind::Owner),
        Just(Kind::Person),
        Just(Kind::Preference),
        Just(Kind::Standing)
    ];
    let source = prop_oneof![
        said().prop_map(|on| FactSaid::Said { on }),
        (said(), said()).prop_map(|(o, round)| FactSaid::Merged {
            from: vec![FactId(1), FactId(2)],
            origins: vec![o],
            round
        }),
    ];
    let fact = (0u64..5, words(), kind, source).prop_map(|(id, fact, kind, said)| FactRow {
        id: FactId(id),
        at: "2026-09-01T00:00:00".to_owned(),
        fact,
        kind,
        said,
    });
    proptest::collection::vec(fact, 0..5).prop_map(|facts| {
        printed(&MemoryPayload {
            facts,
            cap: 40,
            forgotten: Vec::new(),
            merged: Vec::new(),
            cards: Vec::new(),
        })
    })
}

fn record_payload() -> impl Strategy<Value = Value> {
    let summary = (0u8..3, 1u32..3, words()).prop_map(|(n, lines, text)| RecordSummary {
        node: format!("n{n}"),
        handle: "1+2".to_owned(),
        lines,
        text,
    });
    proptest::collection::vec(summary, 0..3)
        .prop_map(|summaries| printed(&RecordPayload { summaries }))
}

fn act() -> impl Strategy<Value = memory::Act> {
    prop_oneof![
        (0u64..5).prop_map(|id| memory::Act::Forget(FactId(id))),
        (0u8..3).prop_map(|n| memory::Act::Retract(format!("n{n}"))),
    ]
}

/// A second Forget of the same fact sends nothing until the first is answered; an answer rereads
/// the memory.
#[test]
fn a_forget_runs_once_until_it_is_answered() {
    let (mut rt, _) = Runtime::<memory::Memory>::start(opened(true, Nothing {}));
    let tap = || memory::Msg::Run(memory::Act::Forget(FactId(3)));
    let sent = rt.step(tap()).1;
    assert_eq!(
        sent,
        vec![Effect::Post {
            ask: memory::Act::Forget(FactId(3)),
            path: "/api/secretary/memory/3/forget".to_owned(),
            body: Body::Empty {},
        }]
    );
    assert_eq!(rt.step(tap()).1, Vec::new());
    let done = Input::Answered {
        ask: memory::Act::Forget(FactId(3)),
        answer: Answer::Done { body: json!({}) },
    };
    assert_eq!(
        rt.step(memory::Msg::Host(Box::new(done))).1,
        vec![Effect::Again {
            path: "/api/secretary/memory".to_owned()
        }]
    );
    assert_eq!(
        rt.step(tap()).1.len(),
        1,
        "an answered act may be tapped again"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn memory_laws(writes: bool, msgs in proptest::collection::vec(prop_oneof![
        held::<memory::Hold, memory::Ask, _>(memory::Hold::Memory, memory_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        held::<memory::Hold, memory::Ask, _>(memory::Hold::Record, record_payload()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        answered::<memory::Hold, memory::Act>(act()).prop_map(|i| memory::Msg::Host(Box::new(i))),
        any::<bool>().prop_map(|w| memory::Msg::Host(Box::new(Input::Context(context(w))))),
        act().prop_map(memory::Msg::Run),
        Just(memory::Msg::Back),
        Just(memory::Msg::Retry),
    ], 0..10)) {
        page_laws::<memory::Memory>(|| opened(writes, Nothing {}), &msgs);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Law: the facts draw as soon as they answer, whatever the record's reading, which is a fold
    /// of every conversation and may take longer; while the record reads, its section says so.
    #[test]
    fn the_facts_draw_whatever_the_record(
        facts in memory_payload(),
        record in reading(record_payload()),
    ) {
        let (mut rt, _) = Runtime::<memory::Memory>::start(opened(true, Nothing {}));
        let held = |ask, reading| {
            memory::Msg::Host(Box::new(Input::Held { ask, reading, behind: None }))
        };
        let answered = Reading::Val { v: Answer::Done { body: facts.clone() } };
        rt.step(held(memory::Hold::Memory, answered));
        rt.step(held(memory::Hold::Record, record.clone()));
        let drawn = format!(
            "{:?}",
            viewtree::dom::fresh(&<memory::Memory as viewtree::Program>::view(rt.model()))
        );
        let payload: MemoryPayload = serde_json::from_value(facts).expect("a memory payload");
        for fact in &payload.facts {
            let printed = format!("{:?}", fact.fact);
            prop_assert!(drawn.contains(&printed[1..printed.len() - 1]), "{drawn}");
        }
        if matches!(record, Reading::Loading) {
            prop_assert!(drawn.contains("Reading the summaries"), "{drawn}");
        }
    }
}
