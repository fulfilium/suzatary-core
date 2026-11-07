use crate::common::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn thread_laws(msgs in one_msgs(
        proptest::collection::vec(page(), 0..4).prop_map(|ms| json!({"thread": "t1", "messages": ms})),
    )) {
        page_laws::<One<thread::Thread>>(|| opened(true, thread::At { id: "t1".to_owned() }), &msgs);
    }
}
