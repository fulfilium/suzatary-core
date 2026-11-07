use crate::common::*;

fn briefing_payload() -> impl Strategy<Value = Value> {
    let block = (words(), proptest::option::of(words())).prop_map(|(source, html)| BlockRow {
        plain: source.clone(),
        source,
        kind: BlockKind::Prose,
        html,
    });
    let day = prop_oneof![
        (words(), proptest::collection::vec(block, 0..3)).prop_map(|(text, blocks)| {
            BriefingDay::Brief {
                conversation: "c1".to_owned(),
                turn: 2,
                text,
                blocks,
            }
        }),
        (words(), proptest::option::of(words()))
            .prop_map(|(source, rich)| BriefingDay::Saved { source, rich }),
    ];
    day.prop_map(|day| {
        printed(&BriefingPayload {
            date: "2026-09-04".to_owned(),
            day,
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn briefing_laws(msgs in one_msgs(briefing_payload())) {
        page_laws::<One<briefing::Briefing>>(|| opened(true, briefing::At { date: "2026-09-04".to_owned() }), &msgs);
    }
}
