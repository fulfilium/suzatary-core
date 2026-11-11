//! The default conversation is opened once, titled "Secretary", by the box (`serve` opens it at
//! startup, since a page reads it from its replica and no read writes); opening it again reads the
//! same conversation, and a tab's replica lists it as the default.

mod common;

use std::sync::Arc;

use common::Fakes;

#[test]
fn the_default_opens_once_and_a_replica_reads_it() {
    let fakes = Arc::new(Fakes::new("default-conversation"));
    let base = common::start(
        Arc::clone(&fakes),
        common::deployment([], []),
        common::loopback_is_this_box(),
    );
    let caps = fakes.server();
    let open = || {
        suzatary_core::secretary::conversations::read_or_open_default(
            caps.secretary.conversations,
            caps.secretary.model,
            &suzatary_base::conversation_id::DEFAULT,
            caps.secretary.clock.now(),
        )
        .expect("the default opens")
    };
    let first = open();
    let second = open();
    assert_eq!(first.opened, second.opened, "opened once, read twice");

    let read = common::reading(&base, "default", &[]);
    assert_eq!(read["title"], "Secretary");
    assert_eq!(
        read["turns"].as_array().expect("turns").len(),
        0,
        "opened with its first line and nothing else"
    );
    let listed = common::listing(&base);
    let rows = listed["conversations"].as_array().expect("conversations");
    assert_eq!(rows.len(), 1, "one conversation, not two: {rows:?}");
    assert_eq!(rows[0]["kind"], "default");
}
