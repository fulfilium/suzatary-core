//! A Microsoft account's calendars through Microsoft Graph: the reads a sync makes, each bounded,
//! read into the calendar's own occurrences, and the one write a tap makes. What is generic over
//! providers (the bounds, the occurrence, the event a tap writes) is [`crate::calendar`]'s; this
//! file holds only what Graph spells in its own way.

use chrono::{NaiveDate, NaiveDateTime};
use suzatary_mail::mail::oauth::Endpoints;

use crate::calendar::bounds::{items, json};
use crate::calendar::{
    CALENDARS, CalendarName, Calendars, EVENTS, Listed, NewEvent, Occurrence, Point, Said, Status,
    Unread, When,
};

/// The header that makes Graph answer every instant in UTC, which [`read_events`] relies on.
pub const PREFER_UTC: (&str, &str) = ("Prefer", "outlook.timezone=\"UTC\"");

#[must_use]
pub fn list_url(endpoints: &Endpoints) -> String {
    format!("{}/me/calendars?$top={CALENDARS}", endpoints.graph())
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
        "{}/me/calendars/{}/calendarView?{}&$top={EVENTS}",
        endpoints.graph(),
        crate::url::percent_encode(calendar),
        crate::url::encode_form(&[
            ("startDateTime", &instant(from)),
            ("endDateTime", &instant(to)),
        ])
    )
}

#[must_use]
pub fn insert_url(endpoints: &Endpoints) -> String {
    format!("{}/me/calendar/events", endpoints.graph())
}

/// Read the calendar list's answer: at most [`CALENDARS`], the account's default first.
pub fn read_calendars(body: &[u8]) -> Result<Calendars, Unread> {
    let answer = json(body)?;
    Ok(Calendars::of(
        items(&answer, "value", CALENDARS)
            .into_iter()
            .filter_map(|item| {
                Listed::new(
                    item.get("id")?.as_str()?,
                    item.get("isDefaultCalendar")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true),
                )
            })
            .collect(),
    ))
}

/// Read one calendar's events into its occurrences, named `name`: at most [`EVENTS`]; an event
/// the calendar cannot read (no start, an inverted span, an instant not in UTC) is left out.
pub fn read_events(body: &[u8], name: &CalendarName) -> Result<Vec<Occurrence>, Unread> {
    let answer = json(body)?;
    Ok(items(&answer, "value", EVENTS)
        .into_iter()
        .filter_map(|item| occurrence(item, name))
        .collect())
}

/// A Graph `dateTimeTimeZone`, read only when it is in UTC.
fn utc(value: Option<&serde_json::Value>) -> Option<NaiveDateTime> {
    let value = value?;
    (value.get("timeZone")?.as_str()? == "UTC").then_some(())?;
    NaiveDateTime::parse_from_str(value.get("dateTime")?.as_str()?, "%Y-%m-%dT%H:%M:%S%.f").ok()
}

fn occurrence(item: &serde_json::Value, name: &CalendarName) -> Option<Occurrence> {
    let text = |key: &str| item.get(key).and_then(serde_json::Value::as_str);
    let (start, end) = (utc(item.get("start"))?, utc(item.get("end"))?);
    let when = if item.get("isAllDay").and_then(serde_json::Value::as_bool) == Some(true) {
        When::all_day(start.date(), end.date()).ok()?
    } else {
        When::timed(start, end).ok()?
    };
    let moved_from = text("originalStart")
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.naive_utc());
    let response = item
        .get("responseStatus")
        .and_then(|response| response.get("response"))
        .and_then(serde_json::Value::as_str);
    Occurrence::said(
        name,
        Said {
            uid: text("iCalUId").or_else(|| text("id"))?,
            when,
            // An all-day instance of a series is keyed by its day, whatever time of day Graph
            // gives its original start.
            moved_from: moved_from.map(|at| match when {
                When::Timed { .. } => Point::At(at),
                When::AllDay { .. } => Point::Day(at.date()),
            }),
            title: text("subject").unwrap_or_default(),
            place: item
                .get("location")
                .and_then(|location| location.get("displayName"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
            // An attendee's address is never read: one whose name is its address is counted
            // unnamed.
            attendees: item
                .get("attendees")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .map(|attendee| {
                    attendee
                        .get("emailAddress")
                        .and_then(|address| address.get("name"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                })
                .collect(),
            status: if item.get("isCancelled").and_then(serde_json::Value::as_bool) == Some(true) {
                Status::Cancelled
            } else if text("showAs") == Some("tentative") || response == Some("tentativelyAccepted")
            {
                Status::Tentative
            } else {
                Status::Confirmed
            },
        },
    )
}

#[must_use]
pub fn insert_body(event: &NewEvent) -> serde_json::Value {
    let at = |at: NaiveDateTime| serde_json::json!({"dateTime": at.format("%Y-%m-%dT%H:%M:%S").to_string(), "timeZone": "UTC"});
    let midnight = |day: NaiveDate| at(day.and_time(chrono::NaiveTime::MIN));
    let (start, end, all_day) = match event.when {
        When::Timed { start, end } => (at(start), at(end), false),
        When::AllDay { start, end } => (midnight(start), midnight(end), true),
    };
    let mut body = serde_json::json!({
        "subject": event.title.as_str(),
        "isAllDay": all_day,
        "start": start,
        "end": end,
    });
    if !event.place.is_empty() {
        body["location"] = serde_json::json!({"displayName": event.place});
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::Provider;
    use suzatary_mail::mail::address::EmailAddress;

    fn primary() -> CalendarName {
        CalendarName::of(
            Provider::Microsoft,
            &EmailAddress::new("ada@example.test").expect("an address"),
            &Listed::new("AAMk-primary=", true).expect("a calendar"),
        )
        .expect("a name")
    }

    #[test]
    fn a_calendar_list_reads_the_default_first_and_is_cut() {
        let many: Vec<serde_json::Value> = (0..60)
            .map(|n| serde_json::json!({"id": format!("c{n}"), "isDefaultCalendar": false}))
            .chain([serde_json::json!({"id": "own", "isDefaultCalendar": true})])
            .collect();
        let listed = read_calendars(serde_json::json!({"value": many}).to_string().as_bytes())
            .expect("a list");
        assert_eq!(listed.iter().count(), CALENDARS);
        let few = read_calendars(
            br#"{"value": [{"id": "team"}, {"id": "own", "isDefaultCalendar": true}]}"#,
        )
        .expect("a list");
        assert_eq!(
            few.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["own", "team"]
        );
        assert_eq!(
            read_calendars(&vec![b' '; crate::calendar::MAX_ANSWER + 1]),
            Err(Unread::TooLarge(crate::calendar::MAX_ANSWER + 1))
        );
    }

    #[test]
    fn an_event_is_read_in_utc_and_held_to_the_calendars_limits() {
        let body = serde_json::json!({"value": [{
            "id": "AAMk-1",
            "iCalUId": "abc@example.test",
            "subject": "x".repeat(900),
            "isCancelled": false,
            "showAs": "tentative",
            "location": {"displayName": "Room 4"},
            "start": {"dateTime": "2026-10-19T08:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-10-19T08:30:00.0000000", "timeZone": "UTC"},
            "originalStart": "2026-10-19T07:00:00Z",
            "attendees": [
                {"emailAddress": {"name": "Ada Lovelace", "address": "ada@example.test"}},
                {"emailAddress": {"name": "bo@example.test", "address": "bo@example.test"}},
                {"emailAddress": {"address": "cy@example.test"}},
            ],
        }, {
            "id": "AAMk-2",
            "subject": "Holiday",
            "isAllDay": true,
            "start": {"dateTime": "2026-10-23T00:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-10-24T00:00:00.0000000", "timeZone": "UTC"},
        }, {
            "id": "AAMk-3",
            "subject": "Cancelled",
            "isCancelled": true,
            "start": {"dateTime": "2026-10-20T08:00:00.0000000", "timeZone": "UTC"},
            "end": {"dateTime": "2026-10-20T09:00:00.0000000", "timeZone": "UTC"},
        }, {
            "id": "AAMk-4",
            "subject": "Not in UTC",
            "start": {"dateTime": "2026-10-20T08:00:00.0000000", "timeZone": "Pacific/Chatham"},
            "end": {"dateTime": "2026-10-20T09:00:00.0000000", "timeZone": "Pacific/Chatham"},
        }, {
            "id": "AAMk-5",
            "subject": "No end",
            "start": {"dateTime": "2026-10-20T08:00:00.0000000", "timeZone": "UTC"},
        }]});
        let read = read_events(body.to_string().as_bytes(), &primary()).expect("events");
        assert_eq!(
            read.len(),
            3,
            "the event not in UTC and the one with no end are left out"
        );
        let meeting = &read[0];
        assert_eq!(
            meeting.id.key(),
            "PLUGIN_MICROSOFT_ADA_AT_EXAMPLE_TEST 20261019T070000Z abc@example.test"
        );
        assert_eq!(
            meeting.content.when().print(),
            "2026-10-19T08:00:00Z/2026-10-19T08:30:00Z"
        );
        assert_eq!(
            meeting.content.title().chars().count(),
            crate::calendar::MAX_TEXT
        );
        assert_eq!(meeting.content.place(), "Room 4");
        assert_eq!(meeting.content.status(), Status::Tentative);
        let attendees = meeting.content.attendees();
        assert_eq!(attendees.named().len(), 1);
        assert_eq!(attendees.unnamed(), 2, "an address is never a name");
        assert_eq!(read[1].content.when().print(), "2026-10-23/2026-10-24");
        assert_eq!(read[2].content.status(), Status::Cancelled);
    }

    #[test]
    fn requests_ask_graph_for_utc_and_a_bounded_window() {
        let endpoints = Endpoints::microsoft();
        assert_eq!(
            list_url(&endpoints),
            "https://graph.microsoft.com/v1.0/me/calendars?$top=20"
        );
        let day = |d| {
            NaiveDate::from_ymd_opt(2026, 10, d)
                .and_then(|day| day.and_hms_opt(0, 0, 0))
                .expect("an instant")
        };
        assert_eq!(
            events_url(&endpoints, "AAMk-x=", (day(12), day(19))),
            "https://graph.microsoft.com/v1.0/me/calendars/AAMk-x%3D/calendarView?\
             startDateTime=2026-10-12T00%3A00%3A00Z&endDateTime=2026-10-19T00%3A00%3A00Z&$top=250"
        );
        assert_eq!(
            insert_url(&endpoints),
            "https://graph.microsoft.com/v1.0/me/calendar/events"
        );
        assert_eq!(PREFER_UTC.0, "Prefer");
    }

    #[test]
    fn an_event_is_written_as_it_was_shown() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 20)
            .and_then(|day| day.and_hms_opt(14, 0, 0))
            .expect("an instant");
        let timed = NewEvent::new(
            "Dentist",
            When::timed(start, start + chrono::Duration::minutes(45)).expect("a span"),
            "Main St 4",
        )
        .expect("an event");
        assert_eq!(
            insert_body(&timed),
            serde_json::json!({
                "subject": "Dentist",
                "isAllDay": false,
                "location": {"displayName": "Main St 4"},
                "start": {"dateTime": "2026-10-20T14:00:00", "timeZone": "UTC"},
                "end": {"dateTime": "2026-10-20T14:45:00", "timeZone": "UTC"},
            })
        );
        let day = NewEvent::new(
            "Holiday",
            When::all_day(start.date(), start.date().succ_opt().expect("a day")).expect("a span"),
            "",
        )
        .expect("an event");
        assert_eq!(
            insert_body(&day),
            serde_json::json!({
                "subject": "Holiday",
                "isAllDay": true,
                "start": {"dateTime": "2026-10-20T00:00:00", "timeZone": "UTC"},
                "end": {"dateTime": "2026-10-21T00:00:00", "timeZone": "UTC"},
            })
        );
    }
}
