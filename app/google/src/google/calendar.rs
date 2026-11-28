//! A Google account's calendars through Google's Calendar API: the reads a sync makes, each
//! bounded, read into the calendar's own occurrences, and the one write a tap makes.
//!
//! Every read is bounded where it is asked and where it is read. What is stored is held to the
//! calendar's own limits: a longer title or place is cut, and attendees past the list's limit
//! are counted without their names, so a meeting with a long invitation is still a meeting.

use chrono::{DateTime, NaiveDate, NaiveDateTime};

use super::Endpoints;
use crate::calendar::bounds::{items, json};
use crate::calendar::{
    CALENDARS, CalendarName, Calendars, EVENTS, Listed, NewEvent, Occurrence, Point, Said, Status,
    Unread, When,
};

#[must_use]
pub fn list_url(endpoints: &Endpoints) -> String {
    format!(
        "{}/users/me/calendarList?maxResults={CALENDARS}&minAccessRole=reader",
        endpoints.calendar()
    )
}

/// The request for one calendar's events in `(from, to)`, each recurring event read as its
/// occurrences.
#[must_use]
pub fn events_url(
    endpoints: &Endpoints,
    calendar: &str,
    (from, to): (NaiveDateTime, NaiveDateTime),
) -> String {
    let instant = |at: NaiveDateTime| at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    format!(
        "{}/calendars/{}/events?{}",
        endpoints.calendar(),
        crate::url::percent_encode(calendar),
        crate::url::encode_form(&[
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
            ("maxResults", &EVENTS.to_string()),
            ("timeMin", &instant(from)),
            ("timeMax", &instant(to)),
        ])
    )
}

/// Read the calendar list's answer: at most [`CALENDARS`], the account's own first.
pub fn read_calendars(body: &[u8]) -> Result<Calendars, Unread> {
    let answer = json(body)?;
    Ok(Calendars::of(
        items(&answer, "items", CALENDARS)
            .into_iter()
            .filter_map(|item| {
                Listed::new(
                    item.get("id")?.as_str()?,
                    item.get("primary").and_then(serde_json::Value::as_bool) == Some(true),
                )
            })
            .collect(),
    ))
}

fn point(value: Option<&serde_json::Value>) -> Option<Point> {
    let value = value?;
    if let Some(at) = value.get("dateTime").and_then(serde_json::Value::as_str) {
        return DateTime::parse_from_rfc3339(at)
            .ok()
            .map(|at| Point::At(at.naive_utc()));
    }
    value
        .get("date")
        .and_then(serde_json::Value::as_str)
        .and_then(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .map(Point::Day)
}

/// Read one calendar's events into its occurrences, named `name`: at most [`EVENTS`], each held
/// to the calendar's limits; an event the calendar cannot read (no start, an inverted span) is
/// left out.
pub fn read_events(body: &[u8], name: &CalendarName) -> Result<Vec<Occurrence>, Unread> {
    let answer = json(body)?;
    Ok(items(&answer, "items", EVENTS)
        .into_iter()
        .filter_map(|item| occurrence(item, name))
        .collect())
}

fn occurrence(item: &serde_json::Value, name: &CalendarName) -> Option<Occurrence> {
    let text = |key: &str| item.get(key).and_then(serde_json::Value::as_str);
    let when = match (point(item.get("start")), point(item.get("end"))) {
        (Some(Point::At(start)), Some(Point::At(end))) => When::timed(start, end).ok()?,
        (Some(Point::Day(start)), Some(Point::Day(end))) => When::all_day(start, end).ok()?,
        _ => return None,
    };
    Occurrence::said(
        name,
        Said {
            uid: text("iCalUID").or_else(|| text("id"))?,
            when,
            moved_from: point(item.get("originalStartTime")),
            title: text("summary").unwrap_or_default(),
            place: text("location").unwrap_or_default(),
            // An attendee's `email` is never read: one with no `displayName` is counted unnamed.
            attendees: item
                .get("attendees")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .map(|attendee| {
                    attendee
                        .get("displayName")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                })
                .collect(),
            status: match text("status") {
                Some("cancelled") => Status::Cancelled,
                Some("tentative") => Status::Tentative,
                _ => Status::Confirmed,
            },
        },
    )
}

#[must_use]
pub fn insert_url(endpoints: &Endpoints) -> String {
    format!("{}/calendars/primary/events", endpoints.calendar())
}

#[must_use]
pub fn insert_body(event: &NewEvent) -> serde_json::Value {
    let (start, end) = match event.when {
        When::Timed { start, end } => {
            let at = |at: NaiveDateTime| serde_json::json!({"dateTime": at.format("%Y-%m-%dT%H:%M:%SZ").to_string()});
            (at(start), at(end))
        }
        When::AllDay { start, end } => {
            let day =
                |day: NaiveDate| serde_json::json!({"date": day.format("%Y-%m-%d").to_string()});
            (day(start), day(end))
        }
    };
    let mut body = serde_json::json!({
        "summary": event.title.as_str(),
        "start": start,
        "end": end,
    });
    if !event.place.is_empty() {
        body["location"] = serde_json::Value::String(event.place.clone());
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{MAX_ANSWER, Provider};
    use chrono::Duration;
    use suzatary_mail::mail::address::EmailAddress;

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn primary() -> CalendarName {
        CalendarName::of(
            Provider::Google,
            &ada(),
            &Listed {
                id: "ada@example.test".to_owned(),
                primary: true,
            },
        )
        .expect("a name")
    }

    #[test]
    fn an_answer_past_the_bound_is_not_read_and_a_list_is_cut() {
        let long = vec![b' '; MAX_ANSWER + 1];
        assert_eq!(read_calendars(&long), Err(Unread::TooLarge(MAX_ANSWER + 1)));
        assert_eq!(
            read_events(&long, &primary()),
            Err(Unread::TooLarge(MAX_ANSWER + 1))
        );
        let many: Vec<serde_json::Value> = (0..60)
            .map(|n| serde_json::json!({"id": format!("c{n}@group.calendar.example.test")}))
            .chain([serde_json::json!({"id": "ada@example.test", "primary": true})])
            .collect();
        let listed = read_calendars(serde_json::json!({"items": many}).to_string().as_bytes())
            .expect("a list");
        assert_eq!(listed.iter().count(), CALENDARS);
        let events: Vec<serde_json::Value> = (0..400)
            .map(|n| {
                serde_json::json!({
                    "iCalUID": format!("e{n}@example.test"),
                    "summary": "Standup",
                    "start": {"dateTime": "2026-10-19T09:00:00+02:00"},
                    "end": {"dateTime": "2026-10-19T09:15:00+02:00"},
                })
            })
            .collect();
        let read = read_events(
            serde_json::json!({"items": events}).to_string().as_bytes(),
            &primary(),
        )
        .expect("events");
        assert_eq!(read.len(), EVENTS);
    }

    #[test]
    fn an_event_is_read_in_utc_and_held_to_the_calendars_limits() {
        let address = |n: usize| format!("p{n}@example.test");
        let attendees: Vec<serde_json::Value> = (0..80)
            .map(|n| match n {
                0..10 => serde_json::json!({"email": address(n)}),
                10..15 => serde_json::json!({"email": address(n), "displayName": address(n)}),
                15 => serde_json::json!({"email": address(n), "displayName": "Room 4", "resource": true}),
                _ => serde_json::json!({"email": address(n), "displayName": format!("Ada {n}")}),
            })
            .collect();
        let body = serde_json::json!({"items": [{
            "id": "abc_20261019T070000Z",
            "iCalUID": "abc@example.test",
            "status": "tentative",
            "summary": "x".repeat(900),
            "location": "Room 4",
            "start": {"dateTime": "2026-10-19T10:00:00+02:00"},
            "end": {"dateTime": "2026-10-19T10:30:00+02:00"},
            "originalStartTime": {"dateTime": "2026-10-19T09:00:00+02:00"},
            "attendees": attendees,
        }, {
            "iCalUID": "day@example.test",
            "summary": "Holiday",
            "start": {"date": "2026-10-23"},
            "end": {"date": "2026-10-24"},
        }, {
            "iCalUID": "broken@example.test",
            "start": {"dateTime": "2026-10-19T10:00:00Z"},
        }]});
        let read = read_events(body.to_string().as_bytes(), &primary()).expect("events");
        assert_eq!(read.len(), 2, "the event with no end is left out");
        let meeting = &read[0];
        assert_eq!(
            meeting.id.key(),
            "PLUGIN_GOOGLE_ADA_AT_EXAMPLE_TEST 20261019T070000Z abc@example.test"
        );
        assert_eq!(
            meeting.content.when().print(),
            "2026-10-19T08:00:00Z/2026-10-19T08:30:00Z"
        );
        assert_eq!(
            meeting.content.title().chars().count(),
            crate::calendar::MAX_TEXT
        );
        let attendees = meeting.content.attendees();
        let named: Vec<&str> = attendees
            .named()
            .iter()
            .map(crate::calendar::AttendeeName::as_str)
            .collect();
        assert_eq!(named.len(), 35, "the named among the first fifty");
        assert_eq!((named[0], named[1]), ("Room 4", "Ada 16"));
        assert_eq!(
            named.len() + attendees.unnamed() as usize,
            80,
            "every attendee is counted"
        );
        assert!(named.iter().all(|name| !name.contains('@')), "{named:?}");
        assert_eq!(meeting.content.status(), Status::Tentative);
        assert_eq!(read[1].content.when().print(), "2026-10-23/2026-10-24");
    }

    #[test]
    fn an_event_is_written_as_it_was_shown() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 20)
            .and_then(|day| day.and_hms_opt(14, 0, 0))
            .expect("an instant");
        let event = NewEvent::new(
            "Dentist",
            When::timed(start, start + Duration::minutes(45)).expect("a span"),
            "Main St 4",
        )
        .expect("an event");
        assert_eq!(
            insert_url(&Endpoints::google()),
            "https://www.googleapis.com/calendar/v3/calendars/primary/events"
        );
        assert_eq!(
            insert_body(&event),
            serde_json::json!({
                "summary": "Dentist",
                "location": "Main St 4",
                "start": {"dateTime": "2026-10-20T14:00:00Z"},
                "end": {"dateTime": "2026-10-20T14:45:00Z"},
            })
        );
        assert_eq!(
            event.sentence(),
            "“Dentist”, 2026-10-20 14:00 to 14:45 UTC, at Main St 4"
        );
        assert!(NewEvent::new("", event.when, "").is_err());
        assert!(NewEvent::new("a\nb", event.when, "").is_err());
    }
}
