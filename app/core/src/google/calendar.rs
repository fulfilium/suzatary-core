//! A Google account's calendars through Google's Calendar API: the reads a sync makes, each
//! bounded, read into the calendar's own occurrences, and the one write a tap makes.
//!
//! Every read is bounded where it is asked and where it is read: [`CALENDARS`] calendars, at
//! most [`EVENTS`] events each within [`window`] of now, and no answer longer than
//! [`MAX_ANSWER`] bytes is read at all. What is stored is held to the calendar's own limits
//! ([`crate::calendar::Content`]): a longer title or place is cut, and attendees past the list's
//! limit are left out, so a meeting with a long invitation is still a meeting.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime};

use super::Endpoints;
use crate::calendar::{CalendarName, Content, Occurrence, OccurrenceId, Status, When};
use crate::mail::address::EmailAddress;

/// The calendar's schema: the text the calendar plugin declares (`app/plugin-ics/ics.schema`,
/// which a test holds this to), so the calendar reads what this source writes as it reads the
/// plugin's.
pub const SCHEMA: &str = "Schema(key='occurrence', events=(Event(name='Learned', fields=(Field(name='occurrence', type=Text()), Field(name='at', type=Instant()), Field(name='actor', type=Text()), Field(name='title', type=Text()), Field(name='when', type=Text()), Field(name='place', type=Text()), Field(name='attendees', type=List(item=Text())), Field(name='status', type=Enum(alternatives=('cancelled', 'confirmed', 'tentative'))))),), registers=(Register(name='attendees', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='attendees'),)), Register(name='place', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='place'),)), Register(name='status', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='status'),)), Register(name='title', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='title'),)), Register(name='when', order=Discrete(), inflationary=False, writes=(Write(event='Learned', field='when'),))))";

/// The actor a Google account's occurrences are written as, which names their calendars.
pub const ACTOR: &str = "google";

/// The most calendars of one account a sync reads.
pub const CALENDARS: usize = 20;

/// The most events of one calendar a sync reads.
pub const EVENTS: usize = 250;

/// The most bytes of one answer the box reads; a longer answer is not read.
pub const MAX_ANSWER: usize = 1 << 20;

/// The span a sync reads: a week back, for what just happened, and ninety days ahead.
#[must_use]
pub fn window(now: NaiveDateTime) -> (NaiveDateTime, NaiveDateTime) {
    (now - Duration::days(7), now + Duration::days(90))
}

/// Why an answer was not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unread {
    /// Longer than [`MAX_ANSWER`].
    #[error("an answer of {0} bytes is longer than the box reads")]
    TooLarge(usize),
    /// Not the JSON the API answers with.
    #[error("the answer is not the Calendar API's JSON")]
    NotJson,
}

/// One calendar the account lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: String,
    /// Whether it is the account's own.
    pub primary: bool,
}

/// The calendars an account lists, at most [`CALENDARS`] of them, its own first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendars(Vec<Listed>);

impl Calendars {
    /// Each, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Listed> {
        self.0.iter()
    }
}

/// The request listing the account's calendars, bounded where it is asked.
#[must_use]
pub fn list_url(endpoints: &Endpoints) -> String {
    format!(
        "{}/users/me/calendarList?maxResults={CALENDARS}&minAccessRole=reader",
        endpoints.calendar()
    )
}

/// The request for one calendar's events in `(from, to)`, each recurring event read as its
/// occurrences, bounded where it is asked.
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

/// `body` as JSON, refused unread past [`MAX_ANSWER`].
fn json(body: &[u8]) -> Result<serde_json::Value, Unread> {
    if body.len() > MAX_ANSWER {
        return Err(Unread::TooLarge(body.len()));
    }
    serde_json::from_slice(body).map_err(|_| Unread::NotJson)
}

/// The `items` of an answer, at most `most` of them.
fn items(answer: &serde_json::Value, most: usize) -> Vec<&serde_json::Value> {
    answer
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().take(most).collect())
        .unwrap_or_default()
}

/// Read the calendar list's answer: at most [`CALENDARS`], the account's own first.
///
/// # Errors
/// [`Unread`].
pub fn read_calendars(body: &[u8]) -> Result<Calendars, Unread> {
    let answer = json(body)?;
    let mut listed: Vec<Listed> = items(&answer, CALENDARS)
        .into_iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            (!id.is_empty() && id.len() <= 500).then(|| Listed {
                id: id.to_owned(),
                primary: item.get("primary").and_then(serde_json::Value::as_bool) == Some(true),
            })
        })
        .collect();
    listed.sort_by_key(|calendar| !calendar.primary);
    Ok(Calendars(listed))
}

/// The name the calendar's occurrences are kept under: `PLUGIN_GOOGLE_` and the account's
/// folder for its own calendar, and a digest of the calendar's id after it for each other, so a
/// calendar keeps its name however the list is ordered. A folder too long for a calendar's name
/// is spelt by its digest.
///
/// # Errors
/// None for any account and calendar the box reads; the calendar's own constructor decides.
pub fn calendar_name(
    account: &EmailAddress,
    calendar: &Listed,
) -> Result<CalendarName, crate::calendar::NotAnOccurrence> {
    use sha2::Digest as _;
    let digest = |text: &str, digits: usize| {
        hex::encode_upper(sha2::Sha256::digest(text.as_bytes()))[..digits].to_owned()
    };
    let grant = super::grant_name(account);
    let folder = grant.trim_start_matches("GOOGLE_GRANT_");
    let folder = if folder.len() <= 60 {
        folder.to_owned()
    } else {
        digest(folder, 16)
    };
    CalendarName::parse(&if calendar.primary {
        format!("PLUGIN_GOOGLE_{folder}")
    } else {
        format!("PLUGIN_GOOGLE_{folder}_{}", digest(&calendar.id, 8))
    })
}

/// `text` cut to at most `most` characters.
fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

/// When one of the API's `start`/`end` objects is: a UTC instant, or a day.
enum Point {
    At(NaiveDateTime),
    Day(NaiveDate),
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
///
/// # Errors
/// [`Unread`].
pub fn read_events(body: &[u8], name: &CalendarName) -> Result<Vec<Occurrence>, Unread> {
    let answer = json(body)?;
    Ok(items(&answer, EVENTS)
        .into_iter()
        .filter_map(|item| occurrence(item, name))
        .collect())
}

fn occurrence(item: &serde_json::Value, name: &CalendarName) -> Option<Occurrence> {
    let text = |key: &str| item.get(key).and_then(serde_json::Value::as_str);
    let uid = text("iCalUID").or_else(|| text("id"))?;
    let when = match (point(item.get("start")), point(item.get("end"))) {
        (Some(Point::At(start)), Some(Point::At(end))) => When::timed(start, end).ok()?,
        (Some(Point::Day(start)), Some(Point::Day(end))) => When::all_day(start, end).ok()?,
        _ => return None,
    };
    // The occurrence's original start keys it, so a moved instance of a series stays itself.
    let key = match point(item.get("originalStartTime")).unwrap_or(match when {
        When::Timed { start, .. } => Point::At(start),
        When::AllDay { start, .. } => Point::Day(start),
    }) {
        Point::At(at) => at.format("%Y%m%dT%H%M%SZ").to_string(),
        Point::Day(day) => day.format("%Y%m%d").to_string(),
    };
    let attendees = item
        .get("attendees")
        .and_then(serde_json::Value::as_array)
        .map(|attendees| {
            attendees
                .iter()
                .filter_map(|attendee| {
                    attendee
                        .get("displayName")
                        .or_else(|| attendee.get("email"))
                        .and_then(serde_json::Value::as_str)
                })
                .take(crate::calendar::MAX_ATTENDEES)
                .map(|attendee| cut(attendee, crate::calendar::MAX_NAME))
                .collect()
        })
        .unwrap_or_default();
    let status = match text("status") {
        Some("cancelled") => Status::Cancelled,
        Some("tentative") => Status::Tentative,
        _ => Status::Confirmed,
    };
    Some(Occurrence {
        id: OccurrenceId::new(name.clone(), &cut(uid, 500), &key).ok()?,
        content: Content::new(
            cut(
                text("summary").unwrap_or_default(),
                crate::calendar::MAX_TEXT,
            ),
            when,
            cut(
                text("location").unwrap_or_default(),
                crate::calendar::MAX_TEXT,
            ),
            attendees,
            status,
        )
        .ok()?,
    })
}

/// An event's title, as a card shows it and the tap writes it: one line, at most 200
/// characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTitle(String);

impl EventTitle {
    /// Read one.
    ///
    /// # Errors
    /// Why it is not one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("an event needs a title".to_owned());
        }
        if text.chars().count() > 200 || text.chars().any(char::is_control) {
            return Err("an event's title is one line of at most 200 characters".to_owned());
        }
        Ok(Self(text.to_owned()))
    }

    /// The title.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An event to write to the account's own calendar, on a tap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEvent {
    pub title: EventTitle,
    pub when: When,
    /// Where, `""` for nowhere in particular.
    pub place: String,
}

impl NewEvent {
    /// An event, its place held to one line of at most 200 characters.
    ///
    /// # Errors
    /// Why it is not one.
    pub fn new(title: &str, when: When, place: &str) -> Result<Self, String> {
        let place = place.trim();
        if place.chars().count() > 200 || place.chars().any(char::is_control) {
            return Err("an event's place is one line of at most 200 characters".to_owned());
        }
        Ok(Self {
            title: EventTitle::parse(title)?,
            when,
            place: place.to_owned(),
        })
    }

    /// The event in words, as a card and a turn say it.
    #[must_use]
    pub fn sentence(&self) -> String {
        let when = match self.when {
            When::Timed { start, end } => format!(
                "{} to {} UTC",
                start.format("%Y-%m-%d %H:%M"),
                end.format("%H:%M")
            ),
            When::AllDay { start, end } if end == start.succ_opt().unwrap_or(end) => {
                format!("all day on {start}")
            }
            When::AllDay { start, end } => {
                format!("all day from {start} to {}", end.pred_opt().unwrap_or(end))
            }
        };
        if self.place.is_empty() {
            format!("“{}”, {when}", self.title.as_str())
        } else {
            format!("“{}”, {when}, at {}", self.title.as_str(), self.place)
        }
    }
}

/// The text form a page and a stored act read, `{title, when, place}`, `when` as the calendar's
/// store spells a span.
#[derive(serde::Serialize, serde::Deserialize)]
struct Spelled {
    title: String,
    when: String,
    place: String,
}

impl serde::Serialize for NewEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Spelled {
            title: self.title.as_str().to_owned(),
            when: self.when.print(),
            place: self.place.clone(),
        }
        .serialize(serializer)
    }
}

// Read through `new`, so a stored act names only an event the constructor admits.
impl<'de> serde::Deserialize<'de> for NewEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelled = Spelled::deserialize(deserializer)?;
        let when = When::parse(&spelled.when).map_err(serde::de::Error::custom)?;
        Self::new(&spelled.title, when, &spelled.place).map_err(serde::de::Error::custom)
    }
}

/// The request inserting an event into the account's own calendar.
#[must_use]
pub fn insert_url(endpoints: &Endpoints) -> String {
    format!("{}/calendars/primary/events", endpoints.calendar())
}

/// The body that inserts `event`.
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

/// Whether the box may read and write an account's Google calendar: a Google account's always,
/// its failures its health; any other account's exactly while its grant holds the calendars,
/// however its mail is read, since the calendar is a fact about the grant and the mail is not.
#[must_use]
pub fn reads_calendars(sync: &crate::mail::account::Sync, held: &super::Held) -> bool {
    use crate::mail::account::Sync;
    let granted =
        matches!(held, super::Held::Signed(parts) if parts.contains(super::Scope::Calendar));
    match sync {
        Sync::Google => true,
        Sync::Gmail | Sync::MsImap(_) | Sync::Proton(_) => granted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn primary() -> CalendarName {
        calendar_name(
            &ada(),
            &Listed {
                id: "ada@example.test".to_owned(),
                primary: true,
            },
        )
        .expect("a name")
    }

    /// An answer past the bound is not read, and a list longer than the bound is cut to it.
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

    /// An event is read in UTC and keyed by its original start; what it says is cut to what the
    /// calendar keeps.
    #[test]
    fn an_event_is_read_in_utc_and_held_to_the_calendars_limits() {
        let attendees: Vec<serde_json::Value> = (0..80)
            .map(|n| serde_json::json!({"email": format!("p{n}@example.test")}))
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
        assert_eq!(
            meeting.content.attendees().len(),
            crate::calendar::MAX_ATTENDEES
        );
        assert_eq!(meeting.content.status(), Status::Tentative);
        assert_eq!(read[1].content.when().print(), "2026-10-23/2026-10-24");
    }

    /// The account's own calendar is named by its folder, and another by a digest of its id, so
    /// a name is the calendar's whatever the list's order.
    #[test]
    fn a_calendars_name_is_its_own() {
        assert_eq!(primary().as_str(), "PLUGIN_GOOGLE_ADA_AT_EXAMPLE_TEST");
        let team = calendar_name(
            &ada(),
            &Listed {
                id: "team@group.calendar.example.test".to_owned(),
                primary: false,
            },
        )
        .expect("a name");
        assert!(
            team.as_str()
                .starts_with("PLUGIN_GOOGLE_ADA_AT_EXAMPLE_TEST_"),
            "{team}"
        );
        assert!(team.of_plugin("google"));
        let long =
            EmailAddress::new(&format!("{}@example.test", "a".repeat(120))).expect("an address");
        for primary in [true, false] {
            let name = calendar_name(
                &long,
                &Listed {
                    id: "x".to_owned(),
                    primary,
                },
            )
            .expect("a long address's calendar is named by its digest");
            assert!(name.of_plugin("google"), "{name}");
        }
    }

    /// An event to write is inserted into the account's own calendar as it was shown.
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
