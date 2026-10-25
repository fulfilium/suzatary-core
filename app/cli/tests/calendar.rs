//! `/api/calendar/{date}` over the real cache-tier store: a week from a day, its cancellations
//! read as cancelled and its titles shown as text.

mod common;

use common::Fakes;
use prodrome_core::declared::Declaration;
use prodrome_core::event::Actor;
use suzatary_core::calendar::{CalendarName, Content, Occurrence, OccurrenceId, Status, When};
use suzatary_core::capability::Datetime;
use suzatary_core::store::learn;
use suzatary_io::plugin_store::PluginStore;

fn occurrence(uid: &str, start: &str, title: &str, status: Status) -> Occurrence {
    let begins =
        chrono::NaiveDateTime::parse_from_str(start, "%Y-%m-%dT%H:%M:%SZ").expect("a time");
    Occurrence {
        id: OccurrenceId::new(
            CalendarName::parse("PLUGIN_ICS_WORK").expect("a name"),
            uid,
            &begins.format("%Y%m%dT%H%M%SZ").to_string(),
        )
        .expect("an id"),
        content: Content::new(
            title.to_owned(),
            When::timed(begins, begins + chrono::Duration::hours(1)).expect("a span"),
            String::new(),
            Vec::new(),
            status,
        )
        .expect("content"),
    }
}

#[test]
fn a_week_reads_the_store_and_a_bad_day_is_refused() {
    let fakes = Fakes::new("calendar");
    let schema = Declaration::admit(include_str!("../../plugin-ics/ics.schema")).expect("lawful");
    let store = PluginStore::at(fakes.root.join("cache/plugins/ics/store"))
        .begin(&schema)
        .expect("begins")
        .store();
    let at = Datetime::new(2026, 10, 5, 10, 0, 0, 0).expect("an instant");
    let ics = Actor::new("ics").expect("an actor");
    learn(
        &store,
        [
            occurrence(
                "review",
                "2026-10-06T12:00:00Z",
                "Quarterly review",
                Status::Confirmed,
            ),
            occurrence(
                "call",
                "2026-10-07T12:00:00Z",
                "<script>alert(1)</script>",
                Status::Cancelled,
            ),
        ]
        .iter()
        .map(|occurrence| occurrence.event(&schema, at, &ics).expect("an event"))
        .collect(),
    )
    .expect("learns");
    let base = common::start(
        std::sync::Arc::new(fakes),
        common::deployment([], []),
        common::loopback_is_this_box(),
    );
    let mut response = common::get(&format!("{base}/api/calendar/2026-10-05"));
    assert_eq!(common::status(&response), 200);
    let body = common::body_json(&mut response);
    assert_eq!(body["from"], "2026-10-05");
    assert_eq!(body["days"], 7);
    let text = body["text"].as_str().expect("text");
    assert!(text.contains("Quarterly review"), "{text}");
    assert!(
        text.contains("<script>alert(1)</script> (cancelled)"),
        "{text}"
    );
    let bad = common::get(&format!("{base}/api/calendar/next-week"));
    assert_eq!(common::status(&bad), 400);
}
