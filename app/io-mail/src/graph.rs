//! The transport for `suzatary_google::microsoft`'s Graph requests: a Microsoft account's calendars
//! read with its calendar token, and the one event a tap writes. A token's bytes go only onto the
//! wire, never into a log or an error.

use std::sync::Arc;

use chrono::NaiveDateTime;
use suzatary_base::capability::CalendarToken;
use suzatary_google::calendar::{CalendarName, NewEvent, Occurrence, Provider, window};
use suzatary_google::microsoft::{
    PREFER_UTC, events_url, insert_body, insert_url, list_url, read_calendars, read_events,
};
use suzatary_io_access::http::{Http, Req, Ureq};
use suzatary_mail::mail::address::EmailAddress;
use suzatary_mail::mail::oauth::Endpoints;

use crate::calendar::{Inserted, ReadError, bearer, read};

#[derive(Clone)]
pub struct Graph {
    http: Arc<dyn Http>,
    endpoints: Endpoints,
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

impl Graph {
    #[must_use]
    pub fn new(endpoints: Endpoints) -> Self {
        Self::with_http(endpoints, Arc::new(Ureq::new()))
    }

    #[must_use]
    pub fn with_http(endpoints: Endpoints, http: Arc<dyn Http>) -> Self {
        Self { http, endpoints }
    }

    /// The account's calendars' occurrences within the window around `now`, each answer read
    /// only within `MAX_ANSWER` bytes.
    pub fn occurrences(
        &self,
        token: &CalendarToken,
        account: &EmailAddress,
        now: NaiveDateTime,
    ) -> Result<Vec<Occurrence>, ReadError> {
        let get = |url: &str| {
            read(self.http.as_ref(), url, token.secret(), &[PREFER_UTC]).map_err(ReadError::Failed)
        };
        let listed = read_calendars(&get(&list_url(&self.endpoints))?)
            .map_err(|failed| ReadError::Failed(failed.to_string()))?;
        let window = window(now);
        let mut occurrences = Vec::new();
        for calendar in listed.iter() {
            let Ok(name) = CalendarName::of(Provider::Microsoft, account, calendar) else {
                continue;
            };
            let body = get(&events_url(&self.endpoints, &calendar.id, window))?;
            occurrences.extend(
                read_events(&body, &name)
                    .map_err(|failed| ReadError::Failed(failed.to_string()))?,
            );
        }
        Ok(occurrences)
    }

    /// Write `event` into the account's default calendar.
    #[must_use]
    pub fn insert(&self, token: &CalendarToken, event: &NewEvent) -> Inserted {
        Inserted::of(
            self.http.send(bearer(
                Req::json(insert_url(&self.endpoints), &insert_body(event)),
                token.secret(),
            )),
            "Microsoft Graph",
        )
    }
}

#[cfg(test)]
mod tests {
    use suzatary_base::capability::Secret;
    use suzatary_google::calendar::{MAX_ANSWER, When};
    use suzatary_io_access::http::{HttpError, Resp, Scripted, header};

    use super::*;

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn token() -> CalendarToken {
        CalendarToken::from_refresh(Secret::new("EwB-fake-access"))
    }

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(9, 0, 0))
            .expect("an instant")
    }

    /// Each read is bounded, asks for UTC, and carries the token in the header alone.
    #[test]
    fn a_sync_reads_the_list_then_each_calendar_bounded() {
        let http = Arc::new(Scripted::answering(vec![
            Ok(Resp::new(
                200,
                serde_json::json!({"value": [{"id": "AAMk-own", "isDefaultCalendar": true}]})
                    .to_string(),
            )),
            Ok(Resp::new(
                200,
                serde_json::json!({"value": [{
                    "iCalUId": "e1@example.test",
                    "subject": "Standup",
                    "start": {"dateTime": "2026-10-19T09:00:00.0000000", "timeZone": "UTC"},
                    "end": {"dateTime": "2026-10-19T09:15:00.0000000", "timeZone": "UTC"},
                }]})
                .to_string(),
            )),
        ]));
        let graph = Graph::with_http(Endpoints::microsoft(), http.clone());
        let read = graph
            .occurrences(&token(), &ada(), now())
            .expect("occurrences");
        assert_eq!(read.len(), 1);
        let sent = http.sent();
        assert_eq!(sent.len(), 2);
        for req in &sent {
            assert_eq!(req.limit, MAX_ANSWER as u64);
            assert_eq!(
                header(&req.headers, "authorization"),
                Some("Bearer EwB-fake-access")
            );
            assert_eq!(
                header(&req.headers, "prefer"),
                Some("outlook.timezone=\"UTC\"")
            );
            assert!(!req.url.contains("EwB"), "{}", req.url);
        }
        assert!(sent[1].url.contains("%24top=250") || sent[1].url.contains("$top=250"));
    }

    #[test]
    fn a_refused_read_reads_nothing_and_says_so() {
        let http = Arc::new(Scripted::answering(vec![Ok(Resp::new(403, "{}"))]));
        let graph = Graph::with_http(Endpoints::microsoft(), http);
        assert!(matches!(
            graph.occurrences(&token(), &ada(), now()),
            Err(ReadError::Failed(_))
        ));
    }

    #[test]
    fn an_insert_says_whether_it_may_have_gone() {
        let event = NewEvent::new(
            "Dentist",
            When::parse("2026-10-20/2026-10-21").expect("a span"),
            "",
        )
        .expect("an event");
        let answering = |answer: Result<Resp, HttpError>| {
            Graph::with_http(
                Endpoints::microsoft(),
                Arc::new(Scripted::answering(vec![answer])),
            )
            .insert(&token(), &event)
        };
        assert_eq!(answering(Ok(Resp::new(201, "{}"))), Inserted::Done);
        assert!(matches!(
            answering(Ok(Resp::new(403, "{}"))),
            Inserted::NotSent(_)
        ));
        assert!(matches!(
            answering(Err(HttpError::Lost("reset".to_owned()))),
            Inserted::Unknown(_)
        ));
    }
}
