//! What every provider's calendar transport shares: how a read fails, how a write ends, and how
//! what was read is learned into the one calendar store.

use std::time::Duration;

use suzatary_base::capability::Secret;
use suzatary_google::calendar::{MAX_ANSWER, Occurrence, Provider};
use suzatary_io_access::http::{Http, HttpError, Req, Resp};
use suzatary_mail::mail::oauth::NeedsLogin;

const TIMEOUT: Duration = Duration::from_secs(30);

/// `req` carrying `token` in a header alone, bounded in time and in the size of its answer.
pub(crate) fn bearer(req: Req, token: &Secret) -> Req {
    req.header("Authorization", format!("Bearer {}", token.expose()))
        .timeout(TIMEOUT)
        .limit(MAX_ANSWER as u64)
}

/// The body of the answer to a GET of `url` with `token`, and `headers` besides.
pub(crate) fn read(
    http: &dyn Http,
    url: &str,
    token: &Secret,
    headers: &[(&str, &str)],
) -> Result<Vec<u8>, String> {
    let req = headers.iter().fold(Req::get(url), |req, (name, value)| {
        req.header(*name, *value)
    });
    let resp = http
        .send(bearer(req, token))
        .map_err(|failed| failed.to_string())?;
    if resp.ok() {
        Ok(resp.body)
    } else {
        Err(format!("the service answered {}", resp.status))
    }
}

/// What a read of a provider's calendars could not do.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The account's grant mints no token: only the owner's sign-in mends it.
    #[error("{0}")]
    NeedsLogin(NeedsLogin),
    /// The token endpoint or the API refused, or could not be reached.
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inserted {
    Done,
    /// Nothing was written: the request never left, or the provider refused it.
    NotSent(String),
    /// The request left and no answer came: it may have been written.
    Unknown(String),
}

impl Inserted {
    /// How an insert ended, `api` naming the service in the words of a refusal.
    #[must_use]
    pub fn of(sent: Result<Resp, HttpError>, api: &str) -> Self {
        match sent {
            Ok(resp) if resp.ok() => Inserted::Done,
            Ok(resp) if resp.status < 500 => {
                Inserted::NotSent(format!("{api} refused it ({})", resp.status))
            }
            Ok(resp) => Inserted::Unknown(format!("{api} answered {}", resp.status)),
            Err(HttpError::TimedOut(why) | HttpError::Lost(why)) => Inserted::Unknown(why),
            Err(failed @ (HttpError::NotSent(_) | HttpError::Private { .. })) => {
                Inserted::NotSent(failed.to_string())
            }
        }
    }
}

/// Learn `occurrences` into the calendar store at `root`, written as `provider`'s at `at`: only
/// what the store does not already read, so a sync may run as often as it likes.
pub fn learn(
    root: &std::path::Path,
    provider: Provider,
    occurrences: &[Occurrence],
    at: suzatary_base::capability::Datetime,
) -> Result<suzatary_store::store::Learnt, String> {
    use prodrome_store::store::Replica as _;
    let schema = prodrome_declared::Declaration::admit(suzatary_google::calendar::SCHEMA)
        .map_err(|refused| format!("the calendar's schema: {refused}"))?;
    // Held until the learning ends, so no sync at another schema empties the store under it.
    let begun = suzatary_io_chains::plugin_store::PluginStore::at(root)
        .begin(&schema)
        .map_err(|failed| failed.to_string())?;
    let store = begun.store();
    let actor = prodrome_data::event::Actor::new(provider.actor())
        .map_err(|refused| refused.to_string())?;
    let items = occurrences
        .iter()
        .map(|occurrence| occurrence.event(store.inner().schema(), at, &actor))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|refused| refused.to_string())?;
    suzatary_store::store::learn(store, items).map_err(|failed| failed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_store_that_kept_an_address_is_emptied_before_any_fetch() {
        use prodrome_declared::Declaration;
        use suzatary_google::calendar::{
            Attendees, CalendarName, Content, Occurrence, OccurrenceId, Status, When,
        };
        let address = format!("{}@{}", "mira", "example.invalid");
        let schema = suzatary_google::calendar::SCHEMA;
        let before = schema
            .replace("Field(name='unnamed', type=Integer()), ", "")
            .replace(
                "Register(name='unnamed', order=Discrete(), inflationary=False, \
                 writes=(Write(event='Learned', field='unnamed'),)), ",
                "",
            );
        assert_ne!(before, schema);
        let before = Declaration::admit(&before).expect("the schema before");
        let dir = suzatary_testkit::scratch("calendar-address");
        let root = dir.path().join("store");
        let kept = prodrome_data::event::parse_event(
            &before,
            &format!(
                "Learned(occurrence='PLUGIN_GOOGLE_ADA 20261019T090000Z review@example.test', \
                 at=datetime(2026, 10, 18, 9, 0, 0), actor='google', title='Review', \
                 when='2026-10-19T09:00:00Z/2026-10-19T10:00:00Z', place='', \
                 attendees=('Ada', '{address}'), status='confirmed')"
            ),
        )
        .expect("an event of the schema before");
        let begun = suzatary_io_chains::plugin_store::PluginStore::at(&root)
            .begin(&before)
            .expect("begins");
        suzatary_store::store::learn(begun.store(), vec![kept]).expect("learns");
        drop(begun);
        let holding = || suzatary_testkit::files_holding(dir.path(), &address).len();
        assert_eq!(holding(), 1, "the store before holds the address");
        suzatary_io_chains::plugin_store::PluginStore::at(&root)
            .settle(&Declaration::admit(schema).expect("the calendar's schema"))
            .expect("settles");
        assert_eq!(
            holding(),
            0,
            "no file holds the address, and nothing was fetched"
        );
        let review = Occurrence {
            id: OccurrenceId::new(
                CalendarName::parse("PLUGIN_GOOGLE_ADA").expect("a name"),
                "review@example.test",
                "20261019T090000Z",
            )
            .expect("an id"),
            content: Content::new(
                "Review".to_owned(),
                When::parse("2026-10-19T09:00:00Z/2026-10-19T10:00:00Z").expect("a span"),
                String::new(),
                Attendees::of(["Ada", address.as_str()], 0),
                Status::Confirmed,
            )
            .expect("content"),
        };
        let at =
            suzatary_base::capability::Datetime::new(2026, 10, 19, 8, 0, 0, 0).expect("an instant");
        let learnt = learn(&root, Provider::Google, &[review], at).expect("learns");
        assert_eq!(learnt.written, 1, "the occurrence is learned again");
        assert_eq!(holding(), 0, "no file holds the address");
    }
}
