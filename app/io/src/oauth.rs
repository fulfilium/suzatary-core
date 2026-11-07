//! The transport and token store for `suzatary_core::mail::oauth`'s sans-I/O state machine.
//!
//! Each account's file keeps one slot per protocol keyed only by `Protocol::SLOT`, so a
//! refresh granted under IMAP consent can never be read back for an SMTP request.
//! A secret's bytes go only onto the wire or into the `0600` grant file, never into a log
//! or an error.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::http::{Http, Req, Ureq};
use crate::present::Present as _;
use crate::tokens::{Bearer, Fresh, Grant, unix_now};
use serde::{Deserialize, Serialize};
use suzatary_core::capability::Secret;
use suzatary_core::mail::account::{Authority, MailAccount};
use suzatary_core::mail::oauth::{
    Answered, Consent, DeviceCode, FormPost, Imap, NeedsLogin, OauthError, Polled, Protocol,
    Silent, Smtp, device_request, poll_request, read_device_code, read_poll, read_silent,
    refresh_request,
};

/// Long enough for a slow tenant, short enough that a hung endpoint cannot stall the caller.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Bumped only if the file's shape changes; never read back as a decision.
const STORE_VERSION: u32 = 1;

/// A 4xx is an answer, not an error: the state machine reads 400 `authorization_pending`.
pub(crate) fn post(http: &dyn Http, request: &FormPost) -> Result<Answered, OauthError> {
    let unreadable = |why: String| OauthError::Unreadable { step: "http", why };
    let fields: Vec<(&str, &str)> = request
        .form
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let resp = http
        .send(Req::form(&request.url, &fields).timeout(REQUEST_TIMEOUT))
        .map_err(|error| unreadable(error.to_string()))?;
    let body: serde_json::Value = serde_json::from_slice(&resp.body)
        .map_err(|error| unreadable(format!("not JSON: {error}")))?;
    Ok(Answered {
        status: resp.status,
        body,
    })
}

/// One protocol's refresh grant; `granted` is for a human reading the file, never a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredGrant {
    refresh_token: String,
    granted: String,
}

/// One account's `<slug>.json`, its grants keyed by `Protocol::SLOT` and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredFile {
    version: u32,
    email: String,
    authority: Authority,
    #[serde(flatten)]
    grants: BTreeMap<String, StoredGrant>,
}

/// One account's stored refresh grant for protocol `P`.
struct Stored<'a, P: Protocol> {
    store: &'a TokenStore,
    account: &'a MailAccount,
    protocol: PhantomData<P>,
}

/// Why a stored grant minted nothing.
enum Unminted {
    NeedsLogin(NeedsLogin),
    Failed(OauthError),
}

impl From<OauthError> for Unminted {
    fn from(failed: OauthError) -> Self {
        Self::Failed(failed)
    }
}

impl<P: Protocol> Grant for Stored<'_, P> {
    type Token = P::Token;
    type Error = Unminted;

    fn mint(&self, http: &dyn Http, _now: u64) -> Result<Fresh<P::Token>, Unminted> {
        let (store, account) = (self.store, self.account);
        let stored = store.read_grants(account)?;
        let Some(grant) = stored.as_ref().and_then(|file| file.grants.get(P::SLOT)) else {
            return Err(Unminted::NeedsLogin(NeedsLogin::NeverGranted {
                email: account.email.to_string(),
                slot: P::SLOT,
            }));
        };
        let Some(ms) = account.microsoft() else {
            return Err(Unminted::Failed(OauthError::NotMicrosoft {
                email: account.email.to_string(),
            }));
        };
        let refresh = Secret::new(grant.refresh_token.clone());
        let answered = post(http, &refresh_request::<P>(ms.authority, &refresh))?;
        let minted = match read_silent::<P>(&answered, account.email.as_str())? {
            Silent::Token(minted) => minted,
            Silent::NeedsLogin(needs) => return Err(Unminted::NeedsLogin(needs)),
        };
        // The rotated refresh token is seen only here, so failing to store it fails the call.
        if let Some(new_refresh) = &minted.refresh {
            store.write_grant::<P>(account, ms.authority, new_refresh, Writing::Rotated)?;
        }
        Ok(Fresh {
            token: minted.token,
            lifetime: minted.expires_in_seconds,
        })
    }
}

/// How a refresh token is written: a fresh grant may make the account's file, a rotated one
/// only replaces what the file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Writing {
    Granted,
    Rotated,
}

/// Where the grants live: one directory, one `<slug>.json` file per account.
#[derive(Clone)]
pub struct TokenStore {
    dir: PathBuf,
    http: Arc<dyn Http>,
}

impl std::fmt::Debug for TokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenStore")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

impl TokenStore {
    /// A store rooted at `dir`; nothing is read until a grant is asked for.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_http(dir, Arc::new(Ureq::new()))
    }

    #[must_use]
    pub fn with_http(dir: impl Into<PathBuf>, http: Arc<dyn Http>) -> Self {
        Self {
            dir: dir.into(),
            http,
        }
    }

    fn path(&self, account: &MailAccount) -> PathBuf {
        self.dir.join(format!("{}.json", account.slug()))
    }

    /// Held across every rewrite of the account's file.
    fn hold(&self, account: &MailAccount) -> Result<crate::durable::Held, OauthError> {
        crate::durable::hold(&self.dir.join(format!("{}.lock", account.slug()))).map_err(|error| {
            OauthError::Unreadable {
                step: "stored grant",
                why: error.to_string(),
            }
        })
    }

    /// `None` when never written; a file that will not parse is an error, never "no grant".
    fn read_grants(&self, account: &MailAccount) -> Result<Option<StoredFile>, OauthError> {
        let unreadable = |why: String| OauthError::Unreadable {
            step: "stored grant",
            why,
        };
        std::fs::read_to_string(self.path(account))
            .present()
            .map_err(|error| unreadable(error.to_string()))?
            .map(|text| serde_json::from_str(&text).map_err(|error| unreadable(error.to_string())))
            .transpose()
    }

    /// Store `refresh` under `P::SLOT`, leaving every other slot as it was. A `Rotated` token
    /// replaces one the file holds and never makes the file, so a refresh that ends after the
    /// account's sign-in was revoked writes nothing back.
    fn write_grant<P: Protocol>(
        &self,
        account: &MailAccount,
        authority: Authority,
        refresh: &Secret,
        writing: Writing,
    ) -> Result<(), OauthError> {
        let _held = self.hold(account)?;
        let held = self.read_grants(account)?;
        if held.is_none() && writing == Writing::Rotated {
            return Err(OauthError::Unreadable {
                step: "stored grant",
                why: format!(
                    "the sign-in of {} was removed while it refreshed",
                    account.email
                ),
            });
        }
        let mut stored = held.unwrap_or_else(|| StoredFile {
            version: STORE_VERSION,
            email: account.email.to_string(),
            authority,
            grants: BTreeMap::new(),
        });
        stored.version = STORE_VERSION;
        stored.email = account.email.to_string();
        stored.authority = authority;
        stored.grants.insert(
            P::SLOT.to_owned(),
            StoredGrant {
                refresh_token: refresh.expose().to_owned(),
                granted: chrono::Utc::now().to_rfc3339(),
            },
        );

        let unreadable = |why: String| OauthError::Unreadable {
            step: "stored grant",
            why,
        };
        std::fs::create_dir_all(&self.dir).map_err(|error| unreadable(error.to_string()))?;
        std::fs::set_permissions(&self.dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| unreadable(error.to_string()))?;

        let json =
            serde_json::to_string_pretty(&stored).map_err(|error| unreadable(error.to_string()))?;
        crate::durable::replace(
            &self.path(account),
            json.as_bytes(),
            crate::durable::Mode::Private,
        )
        .map_err(|error| unreadable(error.to_string()))
    }

    /// A fresh access token for `P` from its own stored grant, or the login it needs.
    ///
    /// Silent only: it holds no `Consent`, so it can never start a sign-in.
    ///
    /// # Errors
    /// [`OauthError`] for an unreadable grant file, a transport failure, a
    /// refusal no sign-in fixes, or an account with no tenant.
    pub fn access<P: Protocol>(
        &self,
        account: &MailAccount,
    ) -> Result<Silent<P::Token>, OauthError> {
        let stored = Stored::<P> {
            store: self,
            account,
            protocol: PhantomData,
        };
        match Bearer::new(stored).token(self.http.as_ref(), unix_now()) {
            Ok(token) => Ok(Silent::Token(token)),
            Err(Unminted::NeedsLogin(needs)) => Ok(Silent::NeedsLogin(needs)),
            Err(Unminted::Failed(failed)) => Err(failed),
        }
    }

    /// Request a device code; `Pending::finish` is the blocking rest.
    ///
    /// # Errors
    /// [`OauthError::NotMicrosoft`] for an account with no Microsoft
    /// strategy, or the endpoint's own refusal to issue a code.
    pub fn begin_device_flow<P: Protocol>(
        &self,
        consent: &Consent,
        account: &MailAccount,
    ) -> Result<(DeviceCode, Pending<P>), OauthError> {
        let Some(ms) = account.microsoft() else {
            return Err(OauthError::NotMicrosoft {
                email: account.email.to_string(),
            });
        };
        let authority = ms.authority;
        let request = device_request::<P>(consent, authority);
        let answered = post(self.http.as_ref(), &request)?;
        let code = read_device_code(&answered)?;
        Ok((
            code.clone(),
            Pending {
                store: self.clone(),
                account: account.clone(),
                authority,
                code,
                protocol: std::marker::PhantomData,
            },
        ))
    }

    /// Run the device-code flow for this protocol and store the grant.
    ///
    /// # Errors
    /// Any error of `begin_device_flow` or `Pending::finish`.
    pub fn authorise<P: Protocol>(
        &self,
        consent: &Consent,
        account: &MailAccount,
        announce: &mut dyn FnMut(&DeviceCode),
        wait: &dyn Fn(Duration),
    ) -> Result<(), OauthError> {
        let (code, pending) = self.begin_device_flow::<P>(consent, account)?;
        announce(&code);
        pending.finish(wait)
    }

    /// Delete every grant this account holds, reading and sending alike.
    ///
    /// # Errors
    /// [`OauthError::Unreadable`] when the file is there and could not be deleted.
    pub fn revoke(&self, account: &MailAccount) -> Result<bool, OauthError> {
        let unreadable = |error: std::io::Error| OauthError::Unreadable {
            step: "stored grant",
            why: error.to_string(),
        };
        let _held = self.hold(account)?;
        crate::durable::remove(&self.path(account)).map_err(unreadable)
    }

    /// Which protocols this account holds a grant for, sorted, read offline.
    ///
    /// # Errors
    /// [`OauthError::Unreadable`] for a grant file that will not read.
    pub fn granted(&self, account: &MailAccount) -> Result<Vec<&'static str>, OauthError> {
        let Some(stored) = self.read_grants(account)? else {
            return Ok(Vec::new());
        };
        let mut slots = Vec::new();
        if stored.grants.contains_key(Imap::SLOT) {
            slots.push(Imap::SLOT);
        }
        if stored.grants.contains_key(Smtp::SLOT) {
            slots.push(Smtp::SLOT);
        }
        slots.sort_unstable();
        Ok(slots)
    }
}

/// A device flow whose code has been issued, waiting to be polled to completion.
#[derive(Debug)]
pub struct Pending<P: Protocol> {
    store: TokenStore,
    account: MailAccount,
    authority: Authority,
    code: DeviceCode,
    protocol: std::marker::PhantomData<P>,
}

impl<P: Protocol> Pending<P> {
    /// Poll until the flow grants, expires, or is refused, storing the grant when it lands.
    ///
    /// # Errors
    /// The endpoint's own refusal, an unreadable answer, or the code
    /// expiring before it was used (`OauthError::Refused { code:
    /// "expired_token", .. }`).
    pub fn finish(self, wait: &dyn Fn(Duration)) -> Result<(), OauthError> {
        let mut interval = self.code.interval_seconds;
        let mut elapsed = 0u64;
        loop {
            let poll = poll_request(self.authority, &self.code);
            let answered = post(self.store.http.as_ref(), &poll)?;
            match read_poll::<P>(&answered, interval)? {
                Polled::Pending { wait_seconds } => {
                    interval = wait_seconds;
                    elapsed += wait_seconds;
                    if elapsed > self.code.expires_in_seconds {
                        return Err(OauthError::Refused {
                            code: "expired_token".to_owned(),
                            description: format!(
                                "the device code for {} expired before it was used",
                                self.account.email
                            ),
                        });
                    }
                    wait(Duration::from_secs(wait_seconds));
                }
                Polled::Granted(minted) => {
                    // Without a refresh token the next sync needs a human anyway, so refuse.
                    let Some(refresh) = minted.refresh else {
                        return Err(OauthError::Unreadable {
                            step: "token",
                            why: "the grant carried no refresh token to store".to_owned(),
                        });
                    };
                    self.store.write_grant::<P>(
                        &self.account,
                        self.authority,
                        &refresh,
                        Writing::Granted,
                    )?;
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Resp, Scripted};
    use suzatary_core::mail::account::{Authority, MsImap, Sync};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-io-oauth-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn ms_account(email: &str) -> MailAccount {
        MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new(email)
                .expect("a lower-case address"),
            subdir: "study".to_owned(),
            sync: Sync::MsImap(MsImap {
                authority: Authority::Organizations,
                inbox_folders: suzatary_core::mail::folders::Folders::inbox(),
                sent_folder: "Sent Items".to_owned(),
            }),
        }
    }

    /// Any tenant but the two the login URL is built from is an unreadable file.
    #[test]
    fn a_stored_grant_names_a_tenant_this_program_knows() {
        let dir = scratch("tenant");
        let store = TokenStore::new(&dir);
        let account = ms_account("her@study.test");
        std::fs::create_dir_all(&dir).expect("a directory");
        for (tenant, reads) in [
            ("organizations", true),
            ("consumers", true),
            ("common", false),
            ("", false),
        ] {
            std::fs::write(
                store.path(&account),
                format!(
                    r#"{{"version": {STORE_VERSION}, "email": "her@study.test", "authority": "{tenant}"}}"#
                ),
            )
            .expect("writes");
            assert_eq!(store.read_grants(&account).is_ok(), reads, "{tenant:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_revoked_grant_is_never_recreated_by_a_refresh() {
        let store = TokenStore::new(scratch("revoke"));
        let account = ms_account("her@study.test");
        assert_eq!(store.revoke(&account).ok(), Some(false), "nothing was kept");
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-1"),
                Writing::Granted,
            )
            .expect("writes");
        store
            .write_grant::<Smtp>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-2"),
                Writing::Granted,
            )
            .expect("writes");
        assert_eq!(store.revoke(&account).ok(), Some(true));
        assert!(store.granted(&account).expect("reads").is_empty());
        assert!(
            store
                .write_grant::<Imap>(
                    &account,
                    Authority::Organizations,
                    &Secret::new("rotated"),
                    Writing::Rotated
                )
                .is_err()
        );
        assert!(store.read_grants(&account).expect("reads").is_none());
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("again"),
                Writing::Granted,
            )
            .expect("a new sign-in writes");
        assert_eq!(store.granted(&account).expect("reads"), ["imap"]);
    }

    #[test]
    fn a_written_grant_round_trips() {
        let store = TokenStore::new(scratch("round-trip"));
        let account = ms_account("her@study.test");
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-1"),
                Writing::Granted,
            )
            .expect("writes");

        let stored = store
            .read_grants(&account)
            .expect("reads")
            .expect("a file exists");
        assert_eq!(stored.email, "her@study.test");
        assert_eq!(stored.authority, Authority::Organizations);
        assert_eq!(stored.version, STORE_VERSION);
        let grant = stored.grants.get("imap").expect("the imap slot");
        assert_eq!(grant.refresh_token, "refresh-1");
        assert!(!grant.granted.is_empty());
    }

    #[test]
    fn the_file_is_0600_and_the_directory_is_0700() {
        let dir = scratch("perms");
        let store = TokenStore::new(dir.clone());
        let account = ms_account("her@study.test");
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-1"),
                Writing::Granted,
            )
            .expect("writes");

        let dir_mode = std::fs::metadata(&dir)
            .expect("the directory exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);

        let file_mode = std::fs::metadata(store.path(&account))
            .expect("the file exists")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
    }

    #[test]
    fn a_grant_for_one_protocol_never_answers_for_the_other() {
        let store = TokenStore::new(scratch("slot-independence"));
        let account = ms_account("her@study.test");
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-imap"),
                Writing::Granted,
            )
            .expect("writes the imap grant");

        let stored = store
            .read_grants(&account)
            .expect("reads")
            .expect("a file exists");
        assert!(stored.grants.contains_key("imap"));
        assert!(!stored.grants.contains_key("smtp"));

        let needs = store
            .access::<Smtp>(&account)
            .expect("an answer")
            .required()
            .expect_err("no smtp grant was ever stored");
        assert_eq!(
            needs,
            NeedsLogin::NeverGranted {
                email: "her@study.test".to_owned(),
                slot: "smtp",
            }
        );
        assert!(needs.to_string().contains("--scope smtp"), "{needs}");
    }

    #[test]
    fn a_store_with_no_file_at_all_needs_a_login() {
        let store = TokenStore::with_http(scratch("empty"), never_called());
        let account = ms_account("her@study.test");
        let answered = store.access::<Imap>(&account).expect("an answer");
        assert_eq!(
            answered,
            Silent::NeedsLogin(NeedsLogin::NeverGranted {
                email: "her@study.test".to_owned(),
                slot: "imap",
            })
        );
    }

    #[test]
    fn a_lapsed_grant_needs_a_login_after_one_refresh_and_no_device_flow() {
        let dir = scratch("lapsed");
        let transport = scripted(vec![Answered {
            status: 400,
            body: serde_json::json!({
                "error": "invalid_grant",
                "error_description": "AADSTS700082: The refresh token has expired.",
            }),
        }]);
        let store = TokenStore::with_http(dir, transport.clone());
        let account = ms_account("her@study.test");
        store
            .write_grant::<Imap>(
                &account,
                Authority::Organizations,
                &Secret::new("refresh-imap"),
                Writing::Granted,
            )
            .expect("a stored grant");
        let needs = store
            .access::<Imap>(&account)
            .expect("an answer")
            .required()
            .expect_err("the grant lapsed");
        assert!(matches!(needs, NeedsLogin::Lapsed { .. }), "{needs:?}");
        assert!(
            needs
                .to_string()
                .contains("suzatary auth her@study.test --scope imap"),
            "{needs}"
        );
        let asked = transport.sent();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(
            asked[0].url.ends_with("/oauth2/v2.0/token"),
            "{}",
            asked[0].url
        );
        assert!(String::from_utf8_lossy(&asked[0].body).contains("grant_type=refresh_token"));
        assert!(!asked.iter().any(|post| post.url.contains("devicecode")));
    }

    // These tests are a permitted caller of `Consent::given_at_a_terminal`, pinned in
    // `app/cli/tests/writes.rs`.

    fn resp(answered: &Answered) -> Resp {
        Resp::new(answered.status, answered.body.to_string())
    }

    fn scripted(answers: Vec<Answered>) -> Arc<Scripted> {
        Arc::new(Scripted::answering(
            answers
                .into_iter()
                .map(|answered| Ok(resp(&answered)))
                .collect(),
        ))
    }

    fn never_called() -> Arc<Scripted> {
        Arc::new(Scripted::new(|req| {
            panic!("the transport must not be asked: {req:?}")
        }))
    }

    fn granted(refresh: &str) -> Answered {
        Answered {
            status: 200,
            body: serde_json::json!({
                "access_token": "access-1",
                "refresh_token": refresh,
                "expires_in": 3600,
            }),
        }
    }

    fn device_code_answer() -> Answered {
        Answered {
            status: 200,
            body: serde_json::json!({
                "device_code": "the-secret-device-code",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://microsoft.com/devicelogin",
                "message": "To sign in, use a web browser to open https://microsoft.com/devicelogin \
                             and enter the code ABCD-EFGH to authenticate.",
                "interval": 0,
                "expires_in": 900,
            }),
        }
    }

    #[test]
    fn begin_device_flow_refuses_a_non_microsoft_account_and_asks_the_transport_nothing() {
        let store = TokenStore::with_http(scratch("non-ms"), never_called());
        let account = MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new("her@gmail.test")
                .expect("a lower-case address"),
            subdir: "personal".to_owned(),
            sync: Sync::Gmail,
        };
        let consent = Consent::given_at_a_terminal();
        let error = store
            .begin_device_flow::<Imap>(&consent, &account)
            .expect_err("no Microsoft strategy, no flow");
        assert_eq!(
            error,
            OauthError::NotMicrosoft {
                email: "her@gmail.test".to_owned(),
            }
        );
    }

    #[test]
    fn begin_device_flow_and_pending_finish_grant_and_store_the_token() {
        let dir = scratch("begin-and-finish");
        let transport = scripted(vec![device_code_answer(), granted("refresh-from-the-flow")]);
        let store = TokenStore::with_http(dir, transport);
        let account = ms_account("her@study.test");
        let consent = Consent::given_at_a_terminal();

        let (code, pending) = store
            .begin_device_flow::<Imap>(&consent, &account)
            .expect("the code is issued");
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.verification_uri, "https://microsoft.com/devicelogin");
        assert!(code.message.contains("ABCD-EFGH"));

        let waited = std::sync::Mutex::new(Vec::new());
        pending
            .finish(&|how_long| waited.lock().unwrap().push(how_long))
            .expect("the flow grants on the first poll");
        assert!(
            waited.into_inner().unwrap().is_empty(),
            "granted on the first poll: no wait"
        );

        let stored = store
            .read_grants(&account)
            .expect("reads")
            .expect("a grant was written");
        assert_eq!(
            stored
                .grants
                .get("imap")
                .expect("the imap slot")
                .refresh_token,
            "refresh-from-the-flow"
        );
    }

    #[test]
    fn pending_finish_refuses_once_the_scripted_elapsed_time_passes_expiry() {
        fn pending_answer() -> Answered {
            Answered {
                status: 400,
                body: serde_json::json!({"error": "authorization_pending"}),
            }
        }
        let dir = scratch("expires");
        // Polls cost 300s each against a 900s expiry; 900 is not `>` 900, so the fourth
        // pending answer is the one that expires the code.
        let transport = scripted(vec![
            Answered {
                status: 200,
                body: serde_json::json!({
                    "device_code": "dc-expiring",
                    "user_code": "WXYZ",
                    "verification_uri": "https://microsoft.com/devicelogin",
                    "interval": 300,
                    "expires_in": 900,
                }),
            },
            pending_answer(),
            pending_answer(),
            pending_answer(),
            pending_answer(),
        ]);
        let store = TokenStore::with_http(dir, transport);
        let account = ms_account("her@study.test");
        let consent = Consent::given_at_a_terminal();

        let (_code, pending) = store
            .begin_device_flow::<Imap>(&consent, &account)
            .expect("the code is issued");

        let waited = std::sync::Mutex::new(Vec::new());
        let error = pending
            .finish(&|how_long| waited.lock().unwrap().push(how_long))
            .expect_err("the endpoint never grants before expiry");
        match error {
            OauthError::Refused { code, description } => {
                assert_eq!(code, "expired_token");
                assert!(description.contains("her@study.test"), "{description}");
                assert!(description.contains("expired"), "{description}");
            }
            other => panic!("expected Refused(expired_token), got {other:?}"),
        }
        assert!(
            !waited.into_inner().unwrap().is_empty(),
            "at least one interval was waited out"
        );
    }
}
