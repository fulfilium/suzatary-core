//! The plugin against feeds shaped like each provider's, and its items through the host's door
//! into the store the calendar reads.

use std::fmt::{Debug, Write as _};

use chrono::{NaiveDate, NaiveDateTime};
use proptest::prelude::*;
use suzatary_core::calendar::{self, Calendar, Occurrence};
use suzatary_plugin_ics::item::{self, Item, SCHEMA, Status, When};
use suzatary_plugin_ics::sync::{self, Fetched, Learned};

const GOOGLE: &str = include_str!("feeds/google.ics");
const MICROSOFT: &str = include_str!("feeds/microsoft.ics");
const PROTON: &str = include_str!("feeds/proton.ics");
const HOME: &str = "PLUGIN_ICS_HOME";
const WORK: &str = "PLUGIN_ICS_WORK";
const NOTHING: [Item; 0] = [];

/// Every call to the core's plugin API: each event admitted by the host's door and read as an
/// occurrence, then learned into a store, read as the calendar reads it.
fn hosted(events: &[Vec<u8>]) -> Result<Calendar, String> {
    use prodrome_core::declared::{Declaration, Declared};
    use suzatary_core::plugin::{PluginName, admit};
    use suzatary_core::store::{InMemory, Reads, learn};
    let ics = PluginName::parse("ics").map_err(|refused| refused.to_string())?;
    let schema =
        Declaration::admit(include_str!("../ics.schema")).map_err(|refused| refused.to_string())?;
    if schema.name().as_str() != SCHEMA || !Occurrence::reads(&schema) {
        return Err("the schema is not the calendar's under its name".to_owned());
    }
    let mut admitted = Vec::new();
    for bytes in events {
        let event = admit(&schema, &ics, bytes)
            .map_err(|refused| format!("{refused}: {}", String::from_utf8_lossy(bytes)))?;
        Occurrence::of(&event).map_err(|refused| refused.to_string())?;
        admitted.push(event);
    }
    let store = InMemory::<Declared>::at(schema);
    learn(&store, admitted).map_err(|refused| refused.to_string())?;
    Ok(Calendar::of(
        &store.nodes().map_err(|refused| refused.to_string())?,
    ))
}

fn at(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("a literal time")
}

fn monday() -> NaiveDateTime {
    at("2026-10-05 10:00")
}

fn answered(feed: &str) -> (u16, Vec<u8>) {
    (200, feed.as_bytes().to_vec())
}

/// One sync of `calendars`, each read through `feed`, with what it said.
fn sync_of(
    cursor: Option<&[u8]>,
    calendars: &[&str],
    now: NaiveDateTime,
    feed: &dyn Fn(&str) -> Fetched,
) -> (Learned, Vec<String>) {
    let mut said = Vec::new();
    let names: Vec<String> = calendars.iter().map(|name| (*name).to_owned()).collect();
    let learned = sync::sync(cursor, &names, now, &mut |name| feed(name), &mut |line| {
        said.push(line);
    });
    (learned, said)
}

fn sync_one(cursor: Option<&[u8]>, feed: &str, now: NaiveDateTime) -> Learned {
    sync_of(cursor, &[HOME], now, &|_| Ok(answered(feed))).0
}

fn work_and_home(name: &str) -> (u16, Vec<u8>) {
    answered(if name == WORK { MICROSOFT } else { PROTON })
}

fn of<'a>(items: &'a [Item], uid: &str) -> Vec<&'a Item> {
    items.iter().filter(|item| item.uid == uid).collect()
}

fn start(item: &Item) -> String {
    match &item.when {
        When::Timed { start, .. } | When::AllDay { start, .. } => start.clone(),
    }
}

/// What one case read and what it should read, each as its `Debug` print.
type Checks = Vec<(String, String)>;

/// A case of the table: what it is, and how it reads.
type Case = (&'static str, fn() -> Checks);

fn same(got: impl Debug, want: impl Debug) -> (String, String) {
    (format!("{got:?}"), format!("{want:?}"))
}

fn google() -> Checks {
    let items = sync_one(None, GOOGLE, monday()).items;
    let standup = of(&items, "standup-7f3a@google.example");
    let keys: Vec<&str> = standup
        .iter()
        .map(|item| item.occurrence.as_str())
        .collect();
    let retreat = of(&items, "retreat-91c@google.example");
    let dentist = of(&items, "dentist-2b@google.example");
    vec![
        // 21 September ends before the window opens, 12 October is an EXDATE, and New York
        // leaves daylight time on 1 November.
        same(
            keys,
            [
                "20260928T130000Z",
                "20261005T130000Z",
                "20261019T130000Z",
                "20261026T130000Z",
                "20261102T140000Z",
                "20261109T140000Z",
                "20261116T140000Z",
                "20261123T140000Z",
                "20261130T140000Z",
            ],
        ),
        same(
            &standup[0].when,
            When::Timed {
                start: "2026-09-28T13:00:00Z".into(),
                end: "2026-09-28T13:30:00Z".into(),
            },
        ),
        same(&standup[0].title, "Weekly standup"),
        same(&standup[0].place, "Room 4, east wing"),
        same(&standup[0].attendees, ["Ada", "Solberg, Mira"]),
        same(standup[0].status, Status::Confirmed),
        same(&standup[2].title, "Weekly standup (moved to Tuesday)"),
        same(start(standup[2]), "2026-10-20T18:00:00Z"),
        same(standup[3].status, Status::Cancelled),
        same(retreat.len(), 1),
        same(
            &retreat[0].when,
            When::AllDay {
                start: "2026-10-09".into(),
                end: "2026-10-11".into(),
            },
        ),
        same(&retreat[0].occurrence, "20261009"),
        same(dentist[0].status, Status::Tentative),
        same(start(dentist[0]), "2026-10-07T16:30:00Z"),
        same(items.iter().all(|item| item.calendar == HOME), true),
    ]
}

fn microsoft() -> Checks {
    let items = sync_one(None, MICROSOFT, monday()).items;
    let lab = of(
        &items,
        "040000008200E00074C5B7101A82E00800000000A0F4C2B1D0C1DC01000000000000000010000000AAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    let holiday = of(&items, "040000008200E00074C5B7101A82E00800000000B1");
    vec![
        same(lab.len(), 2),
        same(
            &lab[0].when,
            When::Timed {
                start: "2026-10-13T17:00:00Z".into(),
                end: "2026-10-13T18:30:00Z".into(),
            },
        ),
        // Pacific standard time from 1 November.
        same(start(lab[1]), "2026-11-10T18:00:00Z"),
        same(
            &lab[0].title,
            "Lab meeting: quarterly review of the instruments",
        ),
        same(&lab[0].place, "Building 7"),
        same(&holiday[0].occurrence, "20261012"),
    ]
}

fn proton() -> Checks {
    let items = sync_one(None, PROTON, monday()).items;
    let yoga = of(&items, "yoga-c1e0@proton.example");
    vec![
        same(yoga.len(), 10),
        same(
            &yoga[0].when,
            When::Timed {
                start: "2026-10-20T05:30:00Z".into(),
                end: "2026-10-20T06:30:00Z".into(),
            },
        ),
        // Zurich leaves summer time on 25 October.
        same(start(yoga[4]), "2026-10-24T05:30:00Z"),
        same(start(yoga[5]), "2026-10-25T06:30:00Z"),
        same(start(yoga[9]), "2026-10-29T06:30:00Z"),
        same(&of(&items, "call-88d@proton.example")[0].attendees, ["Ada"]),
    ]
}

fn unchanged() -> Checks {
    let mut out = Vec::new();
    for feed in [GOOGLE, MICROSOFT, PROTON] {
        let first = sync_one(None, feed, monday());
        let second = sync_one(Some(&first.cursor), feed, monday());
        out.extend([
            same(first.items.is_empty(), false),
            same(second.items, NOTHING),
            same(second.cursor, first.cursor),
        ]);
    }
    out
}

fn new_week() -> Checks {
    let first = sync_one(None, GOOGLE, monday());
    let later = sync_one(Some(&first.cursor), GOOGLE, at("2026-10-12 10:00"));
    // 28 September leaves the window silently.
    vec![same(
        later
            .items
            .iter()
            .map(|item| item.occurrence.as_str())
            .collect::<Vec<_>>(),
        ["20261207T140000Z"],
    )]
}

fn gone_and_back() -> Checks {
    let first = sync_one(None, GOOGLE, monday());
    let without = GOOGLE.replace(
        "UID:dentist-2b@google.example",
        "UID:dentist-2b@google.example-renamed",
    );
    let second = sync_one(Some(&first.cursor), &without, monday());
    let gone = of(&second.items, "dentist-2b@google.example");
    let third = sync_one(Some(&second.cursor), &without, monday());
    let back = sync_one(Some(&third.cursor), GOOGLE, monday());
    vec![
        same(gone.len(), 1),
        same(gone[0].status, Status::Cancelled),
        same(&gone[0].title, "Dentist"),
        same(
            of(&second.items, "dentist-2b@google.example-renamed").len(),
            1,
        ),
        same(third.items, NOTHING),
        same(
            of(&back.items, "dentist-2b@google.example")[0].status,
            Status::Tentative,
        ),
    ]
}

fn excluded() -> Checks {
    let first = sync_one(None, GOOGLE, monday());
    let excluded = GOOGLE.replace(
        "EXDATE;TZID=America/New_York:20261012T090000",
        "EXDATE;TZID=America/New_York:20261012T090000,20261109T090000",
    );
    let second = sync_one(Some(&first.cursor), &excluded, monday());
    let learned: Vec<(&str, Status)> = second
        .items
        .iter()
        .map(|item| (item.occurrence.as_str(), item.status))
        .collect();
    vec![same(learned, [("20261109T140000Z", Status::Cancelled)])]
}

fn renamed() -> Checks {
    let first = sync_one(None, PROTON, monday());
    let renamed = PROTON.replace(
        "SUMMARY:Call with the bank",
        "SUMMARY:Call with the bank (rescheduled)",
    );
    let second = sync_one(Some(&first.cursor), &renamed, monday());
    vec![same(
        second
            .items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>(),
        ["Call with the bank (rescheduled)"],
    )]
}

fn failed_read() -> Checks {
    let first = sync_one(None, PROTON, monday());
    let mut out = Vec::new();
    for fetched in [
        Err("PLUGIN_ICS_HOME is not set".to_owned()),
        Ok((404, Vec::new())),
        Ok(answered("<html>sign in</html>")),
    ] {
        let (learned, said) = sync_of(Some(&first.cursor), &[HOME], monday(), &|_| fetched.clone());
        out.extend([
            same(learned.items, NOTHING),
            same(learned.cursor == first.cursor, true),
            same(
                said.iter().any(|line| {
                    line.starts_with("PLUGIN_ICS_HOME: not")
                        || line.starts_with("PLUGIN_ICS_HOME: the")
                }),
                true,
            ),
        ]);
    }
    out
}

fn removed_calendar() -> Checks {
    let (first, _) = sync_of(None, &[WORK, HOME], monday(), &|name| {
        Ok(work_and_home(name))
    });
    let held = first
        .items
        .iter()
        .filter(|item| item.calendar == WORK && item.status != Status::Cancelled)
        .count();
    let (second, _) = sync_of(Some(&first.cursor), &[HOME], monday(), &|name| {
        Ok(work_and_home(name))
    });
    let (third, _) = sync_of(Some(&second.cursor), &[HOME], monday(), &|name| {
        Ok(work_and_home(name))
    });
    vec![
        same(held > 0, true),
        same(second.items.len(), held),
        same(
            second
                .items
                .iter()
                .all(|item| item.calendar == WORK && item.status == Status::Cancelled),
            true,
        ),
        same(third.items, NOTHING),
    ]
}

fn own_credential() -> Checks {
    let asked = std::cell::RefCell::new(Vec::new());
    let (learned, _) = sync_of(None, &[WORK, HOME], monday(), &|name| {
        asked.borrow_mut().push(name.to_owned());
        Ok(work_and_home(name))
    });
    let mut calendars: Vec<&str> = learned
        .items
        .iter()
        .map(|item| item.calendar.as_str())
        .collect();
    calendars.sort_unstable();
    calendars.dedup();
    vec![
        same(asked.into_inner(), [HOME, WORK]),
        same(calendars, [HOME, WORK]),
    ]
}

fn unreadable_cursor() -> Checks {
    let afresh = sync_one(None, PROTON, monday());
    let (learned, said) = sync_of(Some(b"fence"), &[HOME], monday(), &|_| Ok(answered(PROTON)));
    vec![
        same(learned, afresh),
        same(
            said.iter().filter(|line| line.contains("cursor")).count(),
            1,
        ),
    ]
}

/// A DTSTART its rule does not make is still an instance, a floating time is read in the
/// calendar's X-WR-TIMEZONE, and a TZID with a path prefix by its IANA suffix.
fn restored() -> Checks {
    let feed = "BEGIN:VCALENDAR\r\nX-WR-TIMEZONE:America/New_York\r\n\
        BEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Europe/Paris:20261010T090000\r\n\
        RRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261013T000000Z\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20261007T090000\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:c\r\nDTSTART;TZID=/mozilla.org/20070129_1/Europe/London:\
        20261007T090000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let items = sync_one(None, feed, monday()).items;
    let starts: Vec<(&str, String)> = items
        .iter()
        .map(|item| (item.uid.as_str(), start(item)))
        .collect();
    vec![same(
        starts,
        [
            ("a", "2026-10-10T07:00:00Z"),
            ("a", "2026-10-12T07:00:00Z"),
            ("b", "2026-10-07T13:00:00Z"),
            ("c", "2026-10-07T08:00:00Z"),
        ],
    )]
}

/// A feed with a byte-order mark, two VCALENDARs and property names in lower case reads as the
/// same feed written plainly.
fn spelled_otherwise() -> Checks {
    let plain = sync_one(None, PROTON, monday()).items;
    let (head, tail) = PROTON.split_at(PROTON.find("BEGIN:VEVENT").expect("an event"));
    let split = format!(
        "\u{feff}{head}END:VCALENDAR\nbegin:vcalendar\n{}",
        tail.replace("SUMMARY:", "summary:")
    );
    let (learned, said) = sync_of(None, &[HOME], monday(), &|_| Ok(answered(&split)));
    vec![same(learned.items, plain), same(said.len(), 1)]
}

/// A rule too fine for any calendar, a DURATION past every date and an UNTIL that is no date
/// each cost their own event only, and each is said.
fn hostile() -> Checks {
    let feed = "BEGIN:VCALENDAR\r\n\
        BEGIN:VEVENT\r\nUID:fine\r\nDTSTART:20000101T000000Z\r\nRRULE:FREQ=SECONDLY\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:long\r\nDTSTART:20261007T090000Z\r\nDURATION:P99999999D\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:until\r\nDTSTART:20261007T090000Z\r\nRRULE:FREQ=DAILY;UNTIL=soon\r\nEND:VEVENT\r\n\
        BEGIN:VEVENT\r\nUID:ok\r\nDTSTART:20261008T090000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let (learned, said) = sync_of(None, &[HOME], monday(), &|_| Ok(answered(feed)));
    let uids: Vec<&str> = learned.items.iter().map(|item| item.uid.as_str()).collect();
    let told = |what: &str| said.iter().any(|line| line.contains(what));
    vec![
        same(uids, ["ok", "until"]),
        same(
            [
                told("UID fine: its rule was cut"),
                told("UID long:"),
                told("UID until: RRULE not read"),
            ],
            [true, true, true],
        ),
    ]
}

#[test]
fn each_feed_and_each_change_of_one_reads_as_the_calendar_says() {
    let cases: [Case; 15] = [
        (
            "a mark, two calendars and lower-case names",
            spelled_otherwise,
        ),
        ("a hostile event costs only itself", hostile),
        ("DTSTART, a floating time and a TZID with a path", restored),
        (
            "Google: a weekly meeting with an exclusion, a move and a cancellation",
            google,
        ),
        (
            "Microsoft: a Windows zone name read through its own definition",
            microsoft,
        ),
        (
            "Proton: a count across a change of offset, an organizer not an attendee",
            proton,
        ),
        ("an unchanged feed emits nothing the second time", unchanged),
        ("a new week emits only what entered the window", new_week),
        (
            "an event gone is learned cancelled once and may come back",
            gone_and_back,
        ),
        ("an excluded instance is learned cancelled", excluded),
        ("a changed title is learned once", renamed),
        (
            "a feed that cannot be read leaves its calendar as it was",
            failed_read,
        ),
        (
            "a removed calendar's occurrences are learned cancelled once",
            removed_calendar,
        ),
        (
            "each calendar is read through its own credential",
            own_credential,
        ),
        ("an unreadable cursor starts afresh", unreadable_cursor),
    ];
    for (case, run) in cases {
        for (index, (got, want)) in run().into_iter().enumerate() {
            assert_eq!(got, want, "{case}, check {index}");
        }
    }
}

/// Every day of the window, each occurrence on it with all it says, in UTC.
fn printed(read: &Calendar) -> String {
    let first = NaiveDate::from_ymd_opt(2026, 9, 27).expect("a literal day");
    let mut out = String::new();
    for day in read.days(first, 70, &|utc: NaiveDateTime| utc) {
        if day.entries.is_empty() {
            continue;
        }
        let _ = writeln!(out, "{}", day.date);
        for entry in &day.entries {
            let content = &entry.content;
            let _ = writeln!(
                out,
                "  {} | {} {} {} | {} | {} | {} | {} | {}{}",
                entry.time(),
                entry.id.calendar(),
                entry.id.occurrence(),
                entry.id.uid(),
                content.title(),
                content.place(),
                content.attendees().join("; "),
                content.status().as_str(),
                match content.when() {
                    calendar::When::Timed { .. } => "timed",
                    calendar::When::AllDay { .. } => "all_day",
                },
                if entry.conflicted {
                    " | conflicted"
                } else {
                    ""
                },
            );
        }
    }
    out
}

/// Whatever carries an occurrence from a feed into the store and back out, every day of the
/// window reads as `readings.txt` pins it.
#[test]
fn every_reading_of_the_fixture_feeds_is_pinned() {
    let feeds = [
        ("PLUGIN_ICS_GOOGLE", GOOGLE),
        ("PLUGIN_ICS_MICROSOFT", MICROSOFT),
        ("PLUGIN_ICS_PROTON", PROTON),
    ];
    let names = feeds.map(|(name, _)| name);
    let feed = |name: &str| {
        Ok(answered(
            feeds
                .iter()
                .find(|(feed, _)| *feed == name)
                .map_or("", |(_, text)| text),
        ))
    };
    let (learned, _) = sync_of(None, &names, monday(), &feed);
    let now = item::at(monday()).expect("a literal instant");
    let events: Vec<Vec<u8>> = learned.items.iter().map(|item| item.event(now)).collect();
    let read = hosted(&events).unwrap_or_else(|refused| panic!("{refused}"));
    assert_eq!(printed(&read), include_str!("readings.txt"));
}

/// The host admits every item each fixture's sync emits, at every instant it may be learned.
#[test]
fn every_item_from_every_fixture_is_admitted_and_reads_as_an_occurrence() {
    let mut events = Vec::new();
    for feed in [GOOGLE, MICROSOFT, PROTON] {
        for now in [
            monday(),
            at("2026-10-05 10:00") + chrono::Duration::microseconds(250_000),
        ] {
            let learned = sync_one(None, feed, now);
            let at = item::at(now).expect("a literal instant");
            events.extend(learned.items.iter().map(|item| item.event(at)));
        }
    }
    assert!(events.len() > 40, "{}", events.len());
    hosted(&events).unwrap_or_else(|refused| panic!("{refused}"));
}

/// A feed of simple events, each daily or weekly from a day in September, some counted.
fn feed(events: &[(u8, u8, bool, u8)]) -> String {
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
    for (n, (day, hour, weekly, count)) in events.iter().enumerate() {
        let freq = if *weekly { "WEEKLY" } else { "DAILY" };
        let count = if *count == 0 {
            String::new()
        } else {
            format!(";COUNT={count}")
        };
        let _ = write!(
            out,
            "BEGIN:VEVENT\r\nUID:e{n}\r\nDTSTART;TZID=Europe/Paris:202609{:02}T{:02}0000\r\nDURATION:PT45M\r\nRRULE:FREQ={freq}{count}\r\nSUMMARY:Event {n}\r\nEND:VEVENT\r\n",
            1 + day % 28,
            hour % 24
        );
    }
    out.push_str("END:VCALENDAR\r\n");
    out
}

proptest! {
    /// A sync is a function of its cursor, the feed and the time, and a second sync of an
    /// unchanged feed learns nothing.
    #[test]
    fn syncing_twice_is_syncing_once(events in prop::collection::vec((0u8..28, 0u8..24, any::<bool>(), 0u8..30), 0..8), days in 0i64..90) {
        let feed = feed(&events);
        let now = monday() + chrono::Duration::days(days);
        let first = sync_one(None, &feed, now);
        prop_assert_eq!(&sync_one(None, &feed, now), &first);
        let second = sync_one(Some(&first.cursor), &feed, now);
        prop_assert!(second.items.is_empty());
        prop_assert_eq!(second.cursor, first.cursor);
    }
}
