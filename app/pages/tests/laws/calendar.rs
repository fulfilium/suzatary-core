use crate::common::*;

fn calendar_payload() -> impl Strategy<Value = Value> {
    (words(), proptest::option::of(words())).prop_map(|(text, rich)| {
        printed(&CalendarPayload {
            from: "2026-09-07".to_owned(),
            days: 7,
            text,
            rich,
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn calendar_laws(msgs in one_msgs(calendar_payload())) {
        page_laws::<One<calendar::Calendar>>(|| opened(true, calendar::At { from: "2026-09-07".to_owned() }), &msgs);
    }
}
