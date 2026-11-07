use crate::common::*;

fn briefings_payload() -> impl Strategy<Value = Value> {
    let saved = (1u8..4, words()).prop_map(|(d, title)| BriefingRow {
        date: format!("2026-08-0{d}"),
        title,
        words: 120,
    });
    let brief = (1u8..4, words()).prop_map(|(d, title)| BriefRow {
        date: format!("2026-09-1{d}"),
        conversation: "c1".to_owned(),
        turn: 1,
        title,
        words: 1,
        text: String::new(),
    });
    (
        proptest::collection::vec(saved, 0..3),
        proptest::collection::vec(brief, 0..3),
    )
        .prop_map(|(briefings, briefs)| printed(&BriefingsPayload { briefings, briefs }))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn briefings_laws(msgs in one_msgs(briefings_payload())) {
        page_laws::<One<briefings::Briefings>>(|| opened(true, Nothing {}), &msgs);
    }
}
