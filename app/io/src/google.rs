//! The transport for `suzatary_core::google`'s sans-I/O sign-in and Calendar API. A secret's
//! bytes go only onto the wire and into the secret store, never into a log or an error.

use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDateTime;
use suzatary_core::calendar::Occurrence;
use suzatary_core::capability::Secret;
use suzatary_core::google::{
    AccessToken, Client, ClientId, ClientSecret, Code, Endpoints, Exchanged, KeptGrant, NewEvent,
    RedirectUri, RefreshToken, Verifier,
};
use suzatary_core::mail::address::EmailAddress;
use suzatary_core::mail::oauth::{NeedsLogin, OauthError, Silent};

use crate::http::{Http, HttpError, Req, Ureq};
use crate::secrets::{Resolves, SecretError};

const TIMEOUT: Duration = Duration::from_secs(30);

/// Why the box asks the store, as its audit trail records it.
const ASKING: &str = "a Google account's sign-in";

#[derive(Clone)]
pub struct Google {
    http: Arc<dyn Http>,
    endpoints: Endpoints,
}

impl std::fmt::Debug for Google {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Google")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

/// What a read of Google's APIs (the calendars, the mailbox) could not do.
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
    /// Nothing was written: the request never left, or Google refused it.
    NotSent(String),
    /// The request left and no answer came: it may have been written.
    Unknown(String),
}

impl Google {
    #[must_use]
    pub fn new(endpoints: Endpoints) -> Self {
        Self::with_http(endpoints, Arc::new(Ureq::new()))
    }

    #[must_use]
    pub fn with_http(endpoints: Endpoints, http: Arc<dyn Http>) -> Self {
        Self { http, endpoints }
    }

    #[must_use]
    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }

    /// Trade a callback's code for the grant of the account the consent was for; a grant for
    /// another account is refused.
    pub fn exchange(
        &self,
        client: &Client,
        code: &Code,
        verifier: &Verifier,
        redirect: &RedirectUri,
        asked: &EmailAddress,
    ) -> Result<Exchanged, OauthError> {
        let answered = crate::oauth::post(
            self.http.as_ref(),
            &suzatary_core::google::exchange(&self.endpoints, client, code, verifier, redirect),
        )?;
        suzatary_core::google::read_exchange(&answered, asked)
    }

    /// A fresh access token for `account` from its kept grant.
    pub fn access(
        &self,
        client: &Client,
        refresh: &RefreshToken,
        account: &EmailAddress,
    ) -> Result<Silent<AccessToken>, OauthError> {
        let answered = crate::oauth::post(
            self.http.as_ref(),
            &suzatary_core::google::refresh(&self.endpoints, client, refresh),
        )?;
        suzatary_core::google::read_refresh(&answered, account)
    }

    fn read(&self, token: &AccessToken, url: &str) -> Result<Vec<u8>, String> {
        let resp = self
            .http
            .send(
                Req::get(url)
                    .header("Authorization", format!("Bearer {}", token.token.expose()))
                    .timeout(TIMEOUT)
                    .limit(suzatary_core::google::MAX_ANSWER as u64),
            )
            .map_err(|failed| failed.to_string())?;
        if resp.ok() {
            Ok(resp.body)
        } else {
            Err(format!("Google answered {}", resp.status))
        }
    }

    /// The account's calendars' occurrences within the window around `now`, each answer read
    /// only within `MAX_ANSWER` bytes.
    pub fn occurrences(
        &self,
        grant: &Grant,
        now: NaiveDateTime,
    ) -> Result<Vec<Occurrence>, ReadError> {
        let token = self.token(grant)?;
        let listed = self
            .read(&token, &suzatary_core::google::list_url(&self.endpoints))
            .and_then(|body| {
                suzatary_core::google::read_calendars(&body).map_err(|failed| failed.to_string())
            })
            .map_err(ReadError::Failed)?;
        let window = suzatary_core::google::window(now);
        let mut occurrences = Vec::new();
        for calendar in listed.iter() {
            let Ok(name) = suzatary_core::google::calendar_name(&grant.account, calendar) else {
                continue;
            };
            let url = suzatary_core::google::events_url(&self.endpoints, &calendar.id, window);
            let read = self
                .read(&token, &url)
                .and_then(|body| {
                    suzatary_core::google::read_events(&body, &name)
                        .map_err(|failed| failed.to_string())
                })
                .map_err(ReadError::Failed)?;
            occurrences.extend(read);
        }
        Ok(occurrences)
    }

    /// How many messages the account's mailbox holds: proof the grant reads its mail before the
    /// box hands lieer its tokens. Another account's mailbox fails.
    pub fn mailbox(&self, grant: &Grant) -> Result<u64, ReadError> {
        let token = self.token(grant)?;
        self.read(&token, &suzatary_core::google::profile_url(&self.endpoints))
            .and_then(|body| {
                suzatary_core::google::read_profile(&body, &grant.account)
                    .map_err(|failed| failed.to_string())
            })
            .map_err(ReadError::Failed)
    }

    /// Write `event` into `grant`'s account's own calendar.
    #[must_use]
    pub fn insert(&self, grant: &Grant, event: &NewEvent) -> Inserted {
        let token = match self.token(grant) {
            Ok(token) => token,
            Err(failed) => return Inserted::NotSent(failed.to_string()),
        };
        let sent = self.http.send(
            Req::json(
                suzatary_core::google::insert_url(&self.endpoints),
                &suzatary_core::google::insert_body(event),
            )
            .header("Authorization", format!("Bearer {}", token.token.expose()))
            .timeout(TIMEOUT)
            .limit(suzatary_core::google::MAX_ANSWER as u64),
        );
        match sent {
            Ok(resp) if resp.ok() => Inserted::Done,
            Ok(resp) if resp.status < 500 => {
                Inserted::NotSent(format!("the Calendar API refused it ({})", resp.status))
            }
            Ok(resp) => Inserted::Unknown(format!("the Calendar API answered {}", resp.status)),
            Err(HttpError::TimedOut(why) | HttpError::Lost(why)) => Inserted::Unknown(why),
            Err(failed @ (HttpError::NotSent(_) | HttpError::Private { .. })) => {
                Inserted::NotSent(failed.to_string())
            }
        }
    }

    fn token(&self, grant: &Grant) -> Result<AccessToken, ReadError> {
        match self
            .access(&grant.client, &grant.refresh, &grant.account)
            .map_err(|failed| ReadError::Failed(failed.to_string()))?
        {
            Silent::Token(token) => Ok(token),
            Silent::NeedsLogin(needs) => Err(ReadError::NeedsLogin(needs)),
        }
    }
}

/// One Google account's kept grant, read from the secret store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub account: EmailAddress,
    /// The box's client.
    pub client: Client,
    pub refresh: RefreshToken,
    /// The parts Google granted it.
    pub parts: suzatary_core::google::Scopes,
}

/// What the secret store keeps for one Google account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kept {
    Grant(Grant),
    /// No Google client the box can sign in through: its card asks for one.
    NoClient,
    /// No grant for the account: its sign-in card asks for one.
    NotSignedIn(NeedsLogin),
}

impl Grant {
    /// The box's Google client, as `store` keeps it; `None` when it keeps none Google issued.
    pub fn client(store: &dyn Resolves) -> Result<Option<Client>, SecretError> {
        let read = |name: &str| store.get(name, ASKING);
        Ok(
            match (
                read(suzatary_core::google::CLIENT_ID)?,
                read(suzatary_core::google::CLIENT_SECRET)?,
            ) {
                (Some(id), Some(secret)) => ClientId::parse(id.expose())
                    .zip(ClientSecret::parse(secret.expose()))
                    .map(|(id, secret)| Client { id, secret }),
                _ => None,
            },
        )
    }

    pub fn read(store: &dyn Resolves, account: &EmailAddress) -> Result<Kept, SecretError> {
        let Some(client) = Self::client(store)? else {
            return Ok(Kept::NoClient);
        };
        let Some(kept) = store
            .get(&suzatary_core::google::grant_name(account), ASKING)?
            .as_ref()
            .map(Secret::expose)
            .and_then(KeptGrant::parse)
        else {
            return Ok(Kept::NotSignedIn(NeedsLogin::Google {
                email: account.as_str().to_owned(),
                why: "it was never signed in with Google".to_owned(),
            }));
        };
        Ok(Kept::Grant(Self {
            account: account.clone(),
            client,
            refresh: kept.refresh,
            parts: kept.parts,
        }))
    }
}

/// Google and the store its grants are kept in.
#[derive(Clone)]
pub struct Grants {
    google: Google,
    store: Arc<dyn Resolves + Send + Sync>,
}

impl std::fmt::Debug for Grants {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Grants")
            .field("google", &self.google)
            .finish_non_exhaustive()
    }
}

/// Why a Google account's sync did not reach lieer, as its health records it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotReady {
    #[error("the box's Google client is not kept yet: paste it into its card in the Inbox")]
    NoClient,
    /// The account's grant is missing or Google refused it.
    #[error("{0}")]
    NeedsLogin(NeedsLogin),
    /// The store, the token endpoint or lieer's file failed.
    #[error("{0}")]
    Failed(String),
}

impl NotReady {
    /// What the box lacks to use `grant` of `account`, or the failure's words when it lacks
    /// nothing.
    pub fn lacking(
        self,
        account: &EmailAddress,
        grant: suzatary_core::mail::lacking::Grant,
    ) -> Result<suzatary_core::mail::lacking::Lacking, String> {
        match self {
            NotReady::NoClient => Ok(suzatary_core::mail::lacking::Lacking::GoogleClient),
            NotReady::NeedsLogin(_) => Ok(suzatary_core::mail::lacking::Lacking::Grant {
                account: account.clone(),
                grant,
            }),
            NotReady::Failed(words) => Err(words),
        }
    }

    /// Its failure's class, which names its fix card.
    #[must_use]
    pub fn class(&self) -> suzatary_core::mail::fault::Fault {
        match self {
            NotReady::NoClient => suzatary_core::mail::fault::Fault::Unresolved,
            NotReady::NeedsLogin(_) => suzatary_core::mail::fault::Fault::NeedsLogin,
            NotReady::Failed(_) => suzatary_core::mail::fault::Fault::Unclassified,
        }
    }
}

impl Grants {
    #[must_use]
    pub fn new(google: Google, store: Arc<dyn Resolves + Send + Sync>) -> Self {
        Self { google, store }
    }

    #[must_use]
    pub fn google(&self) -> &Google {
        &self.google
    }

    /// The grant kept for `account`, proved by minting a token from it.
    pub fn live(&self, account: &EmailAddress) -> Result<Grant, NotReady> {
        self.minted(account).map(|(grant, _)| grant)
    }

    /// An access token minted from `account`'s grant, for one use of Google's APIs.
    pub fn token(&self, account: &EmailAddress) -> Result<AccessToken, NotReady> {
        self.minted(account).map(|(_, token)| token)
    }

    fn minted(&self, account: &EmailAddress) -> Result<(Grant, AccessToken), NotReady> {
        let grant = match Grant::read(self.store.as_ref(), account)
            .map_err(|failed| NotReady::Failed(failed.to_string()))?
        {
            Kept::Grant(grant) => grant,
            Kept::NoClient => return Err(NotReady::NoClient),
            Kept::NotSignedIn(needs) => return Err(NotReady::NeedsLogin(needs)),
        };
        match self
            .google
            .access(&grant.client, &grant.refresh, account)
            .map_err(|failed| NotReady::Failed(failed.to_string()))?
        {
            Silent::Token(token) => Ok((grant, token)),
            Silent::NeedsLogin(needs) => Err(NotReady::NeedsLogin(needs)),
        }
    }

    /// Hand lieer an access token minted now from `account`'s grant, in `maildir`, so the
    /// `gmi sync` that follows reads the mail with it.
    pub fn ready(&self, account: &EmailAddress, maildir: &std::path::Path) -> Result<(), NotReady> {
        let token = self.token(account)?;
        hand_to_lieer(maildir, &token, chrono::Utc::now().naive_utc()).map_err(NotReady::Failed)
    }
}

/// Learn `occurrences` into the calendar store at `root`, written as Google's at `at`: only what
/// the store does not already read, so a sync may run as often as it likes.
pub fn learn(
    root: &std::path::Path,
    occurrences: &[Occurrence],
    at: suzatary_core::capability::Datetime,
) -> Result<suzatary_core::store::Learnt, String> {
    use prodrome_core::store::Replica as _;
    let schema = prodrome_core::declared::Declaration::admit(suzatary_core::google::SCHEMA)
        .map_err(|refused| format!("the calendar's schema: {refused}"))?;
    let store = crate::plugin_store::PluginStore::at(root)
        .begin(&schema)
        .map_err(|failed| failed.to_string())?
        .store();
    let actor = prodrome_core::event::Actor::new(suzatary_core::google::ACTOR)
        .map_err(|refused| refused.to_string())?;
    let items = occurrences
        .iter()
        .map(|occurrence| occurrence.event(store.inner().schema(), at, &actor))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|refused| refused.to_string())?;
    suzatary_core::store::learn(&store, items).map_err(|failed| failed.to_string())
}

/// lieer's credentials file in the account's maildir, in the authorized-user form lieer loads
/// with `Credentials.from_authorized_user_file`.
pub const LIEER_CREDENTIALS: &str = ".credentials.gmailieer.json";

/// The fields `Credentials.from_authorized_user_info` requires to be present, which the box
/// writes as null: lieer holds no grant, so nothing it keeps outlives the token.
const ABSENT: [&str; 3] = ["refresh_token", "client_id", "client_secret"];

/// Hand lieer `token`, minted at `minted` (UTC), in `maildir` (owner-only): the access token and
/// its expiry, and no refresh token or client secret, so lieer's file holds nothing that
/// outlives the token. Google's library takes such a token as valid until its expiry; a sync
/// that outlives it fails, and the next one resumes with a fresh token.
pub fn hand_to_lieer(
    maildir: &std::path::Path,
    token: &AccessToken,
    minted: NaiveDateTime,
) -> Result<(), String> {
    // Google's tokens last an hour; a lifetime past u32's seconds is read as that much.
    let expiry = minted
        + chrono::Duration::seconds(i64::from(
            u32::try_from(token.expires_in_seconds).unwrap_or(u32::MAX),
        ));
    let mut file = serde_json::Map::new();
    file.insert("token".to_owned(), token.token.expose().into());
    for absent in ABSENT {
        file.insert(absent.to_owned(), serde_json::Value::Null);
    }
    file.insert(
        "expiry".to_owned(),
        expiry.format("%Y-%m-%dT%H:%M:%SZ").to_string().into(),
    );
    file.insert(
        "scopes".to_owned(),
        suzatary_core::google::Scope::Mail.urls().to_vec().into(),
    );
    std::fs::create_dir_all(maildir).map_err(|failed| failed.to_string())?;
    crate::durable::replace(
        &maildir.join(LIEER_CREDENTIALS),
        serde_json::Value::Object(file).to_string().as_bytes(),
        crate::durable::Mode::Private,
    )
    .map_err(|failed| format!("writing lieer's token: {failed}"))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;
    use crate::http::{Resp, Scripted};

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn grant() -> Grant {
        Grant {
            account: ada(),
            client: Client {
                id: ClientId::parse("1-a.apps.googleusercontent.com").expect("an id"),
                secret: ClientSecret::parse("GOCSPX-x").expect("a secret"),
            },
            refresh: RefreshToken::parse("1//refresh").expect("a token"),
            parts: suzatary_core::google::Scopes::all(),
        }
    }

    fn token() -> Resp {
        Resp::new(200, r#"{"access_token":"ya29.a","expires_in":3600}"#)
    }

    /// Each read is bounded and carries the token in the header alone.
    #[test]
    fn a_sync_reads_the_list_then_each_calendar_bounded() {
        let http = Arc::new(Scripted::answering(vec![
            Ok(token()),
            Ok(Resp::new(
                200,
                serde_json::json!({"items": [{"id": "ada@example.test", "primary": true}]})
                    .to_string(),
            )),
            Ok(Resp::new(
                200,
                serde_json::json!({"items": [{
                    "iCalUID": "e1@example.test",
                    "summary": "Standup",
                    "start": {"dateTime": "2026-10-19T09:00:00Z"},
                    "end": {"dateTime": "2026-10-19T09:15:00Z"},
                }]})
                .to_string(),
            )),
        ]));
        let google = Google::with_http(Endpoints::google(), http.clone());
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(9, 0, 0))
            .expect("an instant");
        let read = google.occurrences(&grant(), now).expect("occurrences");
        assert_eq!(read.len(), 1);
        let sent = http.sent();
        assert_eq!(sent.len(), 3);
        for req in &sent[1..] {
            assert_eq!(req.limit, suzatary_core::google::MAX_ANSWER as u64);
            assert_eq!(
                crate::http::header(&req.headers, "authorization"),
                Some("Bearer ya29.a")
            );
            assert!(!req.url.contains("ya29"), "{}", req.url);
        }
        assert!(sent[2].url.contains("maxResults=250"), "{}", sent[2].url);
    }

    #[test]
    fn a_revoked_grant_needs_a_sign_in_and_reads_nothing() {
        let http = Arc::new(Scripted::answering(vec![Ok(Resp::new(
            400,
            r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#,
        ))]));
        let google = Google::with_http(Endpoints::google(), http.clone());
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(9, 0, 0))
            .expect("an instant");
        assert!(matches!(
            google.occurrences(&grant(), now),
            Err(ReadError::NeedsLogin(NeedsLogin::Google { .. }))
        ));
        assert_eq!(http.sent().len(), 1, "only the token endpoint was asked");
    }

    /// The fields Google's library requires are there, and null.
    #[test]
    fn lieer_is_handed_a_token_and_never_the_grant() {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-io-google-lieer-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let token = AccessToken {
            token: Secret::new("ya29.a"),
            expires_in_seconds: 3599,
        };
        let minted = chrono::NaiveDate::from_ymd_opt(2026, 10, 17)
            .and_then(|day| day.and_hms_opt(9, 0, 0))
            .expect("an instant");
        assert_eq!(hand_to_lieer(&dir, &token, minted), Ok(()));
        let text = std::fs::read_to_string(dir.join(LIEER_CREDENTIALS)).expect("the file");
        for kept in ["1//refresh", "GOCSPX"] {
            assert!(!text.contains(kept), "{text}");
        }
        let written: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(written["token"], "ya29.a");
        assert_eq!(written["expiry"], "2026-10-17T09:59:59Z");
        for absent in ABSENT {
            assert_eq!(
                written.get(absent),
                Some(&serde_json::Value::Null),
                "{absent}"
            );
        }
        assert_eq!(
            written["scopes"][2],
            "https://www.googleapis.com/auth/gmail.modify"
        );
        let mode = std::fs::metadata(dir.join(LIEER_CREDENTIALS))
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_insert_says_whether_it_may_have_gone() {
        let event = NewEvent::new(
            "Dentist",
            suzatary_core::calendar::When::parse("2026-10-20/2026-10-21").expect("a span"),
            "",
        )
        .expect("an event");
        let answering = |second: Result<Resp, HttpError>| {
            Google::with_http(
                Endpoints::google(),
                Arc::new(Scripted::answering(vec![Ok(token()), second])),
            )
        };
        assert_eq!(
            answering(Ok(Resp::new(200, "{}"))).insert(&grant(), &event),
            Inserted::Done
        );
        assert!(matches!(
            answering(Ok(Resp::new(403, "{}"))).insert(&grant(), &event),
            Inserted::NotSent(_)
        ));
        assert!(matches!(
            answering(Err(HttpError::Lost("reset".to_owned()))).insert(&grant(), &event),
            Inserted::Unknown(_)
        ));
    }
}
