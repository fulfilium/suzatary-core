//! The calendar read over its in-memory stores: a later write supersedes, and two stores join.

use prodrome_core::declared::{Declaration, Declared};
use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;

use suzatary_core::stores::InMemory;
use suzatary_google::calendar::{
    Calendar, CalendarName, Content, Occurrence, OccurrenceId, Status, When,
};
use suzatary_store::store::Reads as _;
use suzatary_store::store::learn;

fn schema() -> Declaration {
    Declaration::admit(include_str!("../../plugin-ics/ics.schema")).expect("lawful")
}

fn occurrence(uid: &str, key: &str, title: &str, when: &str, status: Status) -> Occurrence {
    Occurrence {
        id: OccurrenceId::new(
            CalendarName::parse("PLUGIN_ICS_HOME").expect("a name"),
            uid,
            key,
        )
        .expect("an id"),
        content: Content::new(
            title.to_owned(),
            When::parse(when).expect("a span"),
            String::new(),
            Vec::new(),
            status,
        )
        .expect("content"),
    }
}

fn standup(status: Status) -> Occurrence {
    occurrence(
        "standup",
        "20261005T130000Z",
        "Standup",
        "2026-10-05T13:00:00Z/2026-10-05T13:30:00Z",
        status,
    )
}

fn learnt(store: &InMemory<Declared>, occurrences: &[Occurrence]) {
    let schema = schema();
    let at = Datetime::new(2026, 10, 5, 10, 0, 0, 0).expect("an instant");
    let ics = Actor::new("ics").expect("an actor");
    let events = occurrences
        .iter()
        .map(|occurrence| occurrence.event(&schema, at, &ics).expect("an event"))
        .collect();
    learn(store, events).expect("learns");
}

#[test]
fn a_cancellation_supersedes_and_reads_as_cancelled() {
    let store = InMemory::<Declared>::at(schema());
    learnt(&store, &[standup(Status::Confirmed)]);
    learnt(&store, &[standup(Status::Cancelled)]);
    let read = Calendar::of(&store.nodes().expect("reads"));
    let candidates = read.candidates(&standup(Status::Cancelled).id);
    assert_eq!(candidates.len(), 1, "the later write rests on the earlier");
    assert_eq!(candidates[0].status(), Status::Cancelled);
    assert_eq!(read.unread(), 0);
}

#[test]
fn two_stores_read_as_their_union() {
    let one = InMemory::<Declared>::at(schema());
    let two = InMemory::<Declared>::at(schema());
    learnt(&one, &[standup(Status::Confirmed)]);
    learnt(
        &two,
        &[occurrence(
            "retreat",
            "20261004",
            "Retreat",
            "2026-10-04/2026-10-06",
            Status::Confirmed,
        )],
    );
    let mut read = Calendar::of(&one.nodes().expect("reads"));
    read.join(Calendar::of(&two.nodes().expect("reads")));
    assert_eq!(read.len(), 2);
}
