use crate::common::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn message_laws(writes: bool, msgs in one_msgs(page())) {
        page_laws::<One<message::Message>>(|| opened(writes, message::At { id: "i1@example.test".to_owned() }), &msgs);
    }
}
