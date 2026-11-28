//! Microsoft `OAuth2` as a sans-I/O state machine; `suzatary_io_mail::oauth` is the transport.
//!
//! 1. [`Minted<P>`] carries `P::Token`, so [`ImapToken`], [`SmtpToken`] and [`CalendarToken`]
//!    cannot be confused.
//! 2. [`Protocol::SLOT`] keeps grants apart, so a refresh for one scope never yields another.
//! 3. [`device_request`] needs a [`Consent`], which nothing unattended holds or can build.
//!
//! The msal token cache is deliberately not read; the owner re-authorises once instead.

use crate::mail::account::Authority;
use suzatary_base::capability::{CalendarToken, ImapToken, Secret, SmtpToken};

/// `FairEmail`'s public client id, kept because the existing consents were granted to it.
pub const CLIENT_ID: &str = "3514cf2c-e7a3-45a2-80d4-6a3c3498eca0";

/// Requested beside the protocol scope so the grant is refreshable without a prompt.
const OFFLINE_ACCESS: &str = "offline_access";

mod sealed {
    /// Closed so [`super::Protocol`] has exactly the implementors declared here.
    pub trait Sealed {}
}

pub trait Protocol: sealed::Sealed {
    type Token: std::fmt::Debug + Clone + PartialEq + Eq;

    const SCOPE: &'static str;

    /// The token store's slot for this protocol's refresh grant, one per consent.
    const SLOT: &'static str;

    fn token(secret: Secret) -> Self::Token;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Imap;

impl sealed::Sealed for Imap {}
impl Protocol for Imap {
    type Token = ImapToken;
    const SCOPE: &'static str = "https://outlook.office.com/IMAP.AccessAsUser.All";
    const SLOT: &'static str = "imap";
    fn token(secret: Secret) -> ImapToken {
        ImapToken::from_refresh(secret)
    }
}

/// Sending over SMTP, reachable from nothing the agent is generic over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Smtp;

impl sealed::Sealed for Smtp {}
impl Protocol for Smtp {
    type Token = SmtpToken;
    const SCOPE: &'static str = "https://outlook.office.com/SMTP.Send";
    const SLOT: &'static str = "smtp";
    fn token(secret: Secret) -> SmtpToken {
        SmtpToken::from_refresh(secret)
    }
}

/// Reading and editing the account's calendars through Graph, a consent of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calendar;

impl sealed::Sealed for Calendar {}
impl Protocol for Calendar {
    type Token = CalendarToken;
    const SCOPE: &'static str = "https://graph.microsoft.com/Calendars.ReadWrite";
    const SLOT: &'static str = "calendar";
    fn token(secret: Secret) -> CalendarToken {
        CalendarToken::from_refresh(secret)
    }
}

/// Where Microsoft's sign-in and Graph are; a test serves both on loopback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    login: String,
    graph: String,
}

impl Endpoints {
    #[must_use]
    pub fn microsoft() -> Self {
        Self {
            login: "https://login.microsoftonline.com".to_owned(),
            graph: "https://graph.microsoft.com/v1.0".to_owned(),
        }
    }

    /// The same paths under `base`, a fake Microsoft a test serves.
    #[must_use]
    pub fn at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            login: format!("{base}/login"),
            graph: format!("{base}/v1.0"),
        }
    }

    #[must_use]
    pub fn graph(&self) -> &str {
        &self.graph
    }

    fn devicecode(&self, authority: Authority) -> String {
        format!("{}/{authority}/oauth2/v2.0/devicecode", self.login)
    }

    fn token(&self, authority: Authority) -> String {
        format!("{}/{authority}/oauth2/v2.0/token", self.login)
    }
}

/// A claim that a human is watching, which the device-code flow requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Consent(());

impl Consent {
    /// Claim that an operator is at a terminal waiting to type a code.
    #[must_use]
    pub fn given_at_a_terminal() -> Self {
        Self(())
    }

    /// Claim that the owner tapped a proposal, witnessed by a [`suzatary_signatures::signatures::Tap`].
    #[must_use]
    pub fn given_by_tap(_tap: &suzatary_signatures::signatures::Tap) -> Self {
        Self(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormPost {
    pub url: String,
    /// `application/x-www-form-urlencoded` fields.
    pub form: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    pub status: u16,
    /// The body, parsed; a non-JSON body is [`OauthError::Unreadable`] before it gets here.
    pub body: serde_json::Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OauthError {
    /// The endpoint answered a body that is not JSON or lacks the step's field.
    #[error("the {step} response could not be read: {why}")]
    Unreadable {
        /// Which step: "device code", "token".
        step: &'static str,
        why: String,
    },
    /// The endpoint said no, carried whole because operators search `AADSTS` codes.
    #[error("{code}: {description}")]
    Refused {
        code: String,
        /// Its `error_description`, or the body when it named none.
        description: String,
    },
    /// The organisation's administrator must approve the app before anyone in it signs in, so
    /// no new sign-in or retry mends it. Parsed once, where a refusal is read.
    #[error(
        "an administrator of the account's organisation must approve this app before it can \
         sign in ({code}: {description})"
    )]
    AdminApproval { code: String, description: String },
    #[error("{email} is not a Microsoft account, so it has no sign-in to run")]
    NotMicrosoft { email: String },
}

/// What a silent access answers: a token, or that a person must sign in, never a prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Silent<T> {
    Token(T),
    NeedsLogin(NeedsLogin),
}

impl<T> Silent<T> {
    pub fn required(self) -> Result<T, NeedsLogin> {
        match self {
            Self::Token(token) => Ok(token),
            Self::NeedsLogin(needs) => Err(needs),
        }
    }
}

/// Why a person has to sign in; the message names the verb to run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NeedsLogin {
    #[error(
        "no {slot} authorisation for {email} — run `suzatary auth {email} --scope {slot}` at a terminal"
    )]
    NeverGranted { email: String, slot: &'static str },
    /// Microsoft refused the stored grant in words only a sign-in answers.
    #[error(
        "the {slot} authorisation for {email} has lapsed ({code}: {description}) — run `suzatary auth {email} --scope {slot}` at a terminal"
    )]
    Lapsed {
        email: String,
        slot: &'static str,
        code: Lapse,
        description: String,
    },
    /// An administrator of the account's organisation must approve the app: a new sign-in does
    /// not mend it, so no re-authorisation is asked.
    #[error(
        "an administrator of the organisation of {email} must approve this app before its {slot} \
         authorisation works ({code}: {description})"
    )]
    AdminApproval {
        email: String,
        slot: &'static str,
        code: String,
        description: String,
    },
    #[error(
        "the Google sign-in for {email} is missing or has lapsed ({why}) — sign it in again from \
         its card in the Inbox; a Google client left in Testing ends each sign-in after 7 days, \
         so publish it to In production"
    )]
    Google { email: String, why: String },
}

/// An OAuth 2.0 `error` that only a person can answer: the provider ended the grant or the
/// client, and no retry mends it. The one list every puller reads a refusal against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lapse {
    InvalidGrant,
    InvalidClient,
    UnauthorizedClient,
    /// The OAuth client was deleted at the provider.
    DeletedClient,
    InteractionRequired,
    ConsentRequired,
    LoginRequired,
}

impl Lapse {
    pub const ALL: [Lapse; 7] = [
        Lapse::InvalidGrant,
        Lapse::InvalidClient,
        Lapse::UnauthorizedClient,
        Lapse::DeletedClient,
        Lapse::InteractionRequired,
        Lapse::ConsentRequired,
        Lapse::LoginRequired,
    ];

    /// The `error` the provider sends.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Lapse::InvalidGrant => "invalid_grant",
            Lapse::InvalidClient => "invalid_client",
            Lapse::UnauthorizedClient => "unauthorized_client",
            Lapse::DeletedClient => "deleted_client",
            Lapse::InteractionRequired => "interaction_required",
            Lapse::ConsentRequired => "consent_required",
            Lapse::LoginRequired => "login_required",
        }
    }

    /// The lapse an `error` names; `None` for any other code, which is an outage or a fault of
    /// the request.
    #[must_use]
    pub fn of_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|lapse| lapse.code() == code)
    }

    /// The lapse the last `RefreshError` of a lieer run names. Python prints it as
    /// `RefreshError: ('<code>: <words>', {'error': '<code>', ...})`; this is the one place that
    /// reads that text.
    #[must_use]
    pub fn of_refresh_error(output: &str) -> Option<Self> {
        const KEY: &str = "'error': '";
        let line = output.lines().rfind(|line| line.contains("RefreshError"))?;
        let from = line.find(KEY)? + KEY.len();
        let code = line[from..].split('\'').next()?;
        Self::of_code(code)
    }
}

impl std::fmt::Display for Lapse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// The protocol's own scope plus `offline_access`; one protocol per consent, never two.
#[must_use]
pub fn scope<P: Protocol>() -> String {
    format!("{} {OFFLINE_ACCESS}", P::SCOPE)
}

/// Step one: ask for a device code, which requires a [`Consent`].
#[must_use]
pub fn device_request<P: Protocol>(
    _consent: &Consent,
    endpoints: &Endpoints,
    authority: Authority,
) -> FormPost {
    FormPost {
        url: endpoints.devicecode(authority),
        form: vec![
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            ("scope".to_owned(), scope::<P>()),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    /// The opaque code this machine polls with. Never printed.
    pub device_code: Secret,
    pub user_code: String,
    pub verification_uri: String,
    /// Microsoft's own sentence, printed verbatim.
    pub message: String,
    /// Seconds to wait between polls, as the endpoint asked.
    pub interval_seconds: u64,
    pub expires_in_seconds: u64,
}

pub fn read_device_code(answered: &Answered) -> Result<DeviceCode, OauthError> {
    if let Some(refused) = refusal(answered) {
        return Err(refused);
    }
    let field = |name: &'static str| -> Result<String, OauthError> {
        answered
            .body
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| OauthError::Unreadable {
                step: "device code",
                why: format!("no {name}"),
            })
    };
    let seconds = |name: &str, fallback: u64| -> u64 {
        answered
            .body
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(fallback)
    };
    let user_code = field("user_code")?;
    let verification_uri = field("verification_uri")?;
    Ok(DeviceCode {
        device_code: Secret::new(field("device_code")?),
        message: answered
            .body
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map_or_else(
                || {
                    format!(
                        "To sign in, open {verification_uri} and enter the code {user_code} to authenticate."
                    )
                },
                str::to_owned,
            ),
        user_code,
        verification_uri,
        interval_seconds: seconds("interval", 5),
        expires_in_seconds: seconds("expires_in", 900),
    })
}

#[must_use]
pub fn poll_request(endpoints: &Endpoints, authority: Authority, code: &DeviceCode) -> FormPost {
    FormPost {
        url: endpoints.token(authority),
        form: vec![
            (
                "grant_type".to_owned(),
                "urn:ietf:params:oauth:grant-type:device_code".to_owned(),
            ),
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            (
                "device_code".to_owned(),
                code.device_code.expose().to_owned(),
            ),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Polled<P: Protocol> {
    /// Nobody has typed the code yet; wait this many seconds and poll again.
    Pending { wait_seconds: u64 },
    /// Done: the grant, for this protocol only.
    Granted(Minted<P>),
}

/// A grant for one protocol, whose `token` cannot become the other kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minted<P: Protocol> {
    pub token: P::Token,
    /// The refresh token to store, when issued; a rotated one replaces the old.
    pub refresh: Option<Secret>,
    pub expires_in_seconds: u64,
}

pub fn read_poll<P: Protocol>(
    answered: &Answered,
    interval_seconds: u64,
) -> Result<Polled<P>, OauthError> {
    if let Some(code) = error_code(answered) {
        return match code.as_str() {
            "authorization_pending" => Ok(Polled::Pending {
                wait_seconds: interval_seconds,
            }),
            // The endpoint's guidance is to add five seconds each time.
            "slow_down" => Ok(Polled::Pending {
                wait_seconds: interval_seconds + 5,
            }),
            _ => Err(refusal(answered).unwrap_or(OauthError::Unreadable {
                step: "token",
                why: code,
            })),
        };
    }
    Ok(Polled::Granted(read_grant::<P>(answered)?))
}

#[must_use]
pub fn refresh_request<P: Protocol>(
    endpoints: &Endpoints,
    authority: Authority,
    refresh: &Secret,
) -> FormPost {
    FormPost {
        url: endpoints.token(authority),
        form: vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            ("refresh_token".to_owned(), refresh.expose().to_owned()),
            ("scope".to_owned(), scope::<P>()),
        ],
    }
}

fn read_refresh<P: Protocol>(answered: &Answered) -> Result<Minted<P>, OauthError> {
    if let Some(refused) = refusal(answered) {
        return Err(refused);
    }
    read_grant::<P>(answered)
}

/// Read a silent refresh, turning a lapse into [`NeedsLogin::Lapsed`]; other refusals stay errors.
pub fn read_silent<P: Protocol>(
    answered: &Answered,
    email: &str,
) -> Result<Silent<Minted<P>>, OauthError> {
    match read_refresh::<P>(answered) {
        Ok(minted) => Ok(Silent::Token(minted)),
        Err(OauthError::AdminApproval { code, description }) => {
            Ok(Silent::NeedsLogin(NeedsLogin::AdminApproval {
                email: email.to_owned(),
                slot: P::SLOT,
                code,
                description,
            }))
        }
        Err(OauthError::Refused { code, description }) => match Lapse::of_code(&code) {
            Some(code) => Ok(Silent::NeedsLogin(NeedsLogin::Lapsed {
                email: email.to_owned(),
                slot: P::SLOT,
                code,
                description,
            })),
            None => Err(OauthError::Refused { code, description }),
        },
        Err(failed) => Err(failed),
    }
}

/// The half both grant paths share: `access_token` and any rotated refresh token.
fn read_grant<P: Protocol>(answered: &Answered) -> Result<Minted<P>, OauthError> {
    let access = answered
        .body
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| OauthError::Unreadable {
            step: "token",
            why: "no access_token".to_owned(),
        })?;
    Ok(Minted {
        token: P::token(Secret::new(access)),
        refresh: answered
            .body
            .get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .map(Secret::new),
        expires_in_seconds: answered
            .body
            .get("expires_in")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(3600),
    })
}

fn error_code(answered: &Answered) -> Option<String> {
    answered
        .body
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// The `AADSTS` codes that say an administrator must approve the app: consent not given by the
/// user or an administrator, an administrator required to consent, and an application that needs
/// an administrator's approval.
const ADMIN_APPROVAL: [u32; 3] = [65001, 90094, 90095];

/// Microsoft's words, which the box shows but does not trust: control characters removed and at
/// most [`MAX_WORDS`] characters kept.
fn outside_words(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_WORDS)
        .collect()
}

const MAX_WORDS: usize = 300;

/// The `AADSTS` code an answer names: the first of its `error_codes`, else the number its
/// description opens with (`AADSTS65001: …`).
fn aadsts(answered: &Answered) -> Option<u32> {
    answered
        .body
        .get("error_codes")
        .and_then(serde_json::Value::as_array)
        .and_then(|codes| codes.first())
        .and_then(serde_json::Value::as_u64)
        .and_then(|code| u32::try_from(code).ok())
        .or_else(|| {
            let description = answered.body.get("error_description")?.as_str()?;
            let digits = description.strip_prefix("AADSTS")?;
            let end = digits
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(digits.len());
            digits[..end].parse().ok()
        })
}

/// The refusal this answer is; a non-2xx with no `error` field is one, named by its status. The
/// one place a refusal is read for the class an administrator alone mends.
fn refusal(answered: &Answered) -> Option<OauthError> {
    match error_code(answered) {
        Some(code) => {
            let description = answered
                .body
                .get("error_description")
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| answered.body.to_string(), str::to_owned);
            Some(
                if aadsts(answered).is_some_and(|n| ADMIN_APPROVAL.contains(&n)) {
                    OauthError::AdminApproval {
                        code,
                        description: outside_words(&description),
                    }
                } else {
                    OauthError::Refused { code, description }
                },
            )
        }
        None if !(200..300).contains(&answered.status) => Some(OauthError::Refused {
            code: format!("HTTP {}", answered.status),
            description: answered.body.to_string(),
        }),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn answered(status: u16, body: serde_json::Value) -> Answered {
        Answered { status, body }
    }

    fn field<'a>(post: &'a FormPost, name: &str) -> &'a str {
        post.form
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_str())
    }

    #[test]
    fn a_refresh_names_exactly_one_protocols_scope() {
        let refresh = Secret::new("r-1");
        let imap =
            refresh_request::<Imap>(&Endpoints::microsoft(), Authority::Organizations, &refresh);
        let smtp = refresh_request::<Smtp>(&Endpoints::microsoft(), Authority::Consumers, &refresh);

        assert_eq!(
            field(&imap, "scope"),
            "https://outlook.office.com/IMAP.AccessAsUser.All offline_access"
        );
        assert!(!field(&imap, "scope").contains("SMTP"));
        assert_eq!(
            field(&smtp, "scope"),
            "https://outlook.office.com/SMTP.Send offline_access"
        );
        assert!(!field(&smtp, "scope").contains("IMAP"));

        assert_eq!(
            imap.url,
            "https://login.microsoftonline.com/organizations/oauth2/v2.0/token"
        );
        assert_eq!(
            smtp.url,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
        );
        assert_eq!(field(&imap, "grant_type"), "refresh_token");
        assert_eq!(field(&imap, "client_id"), CLIENT_ID);
    }

    #[test]
    fn the_calendar_consent_asks_graph_for_calendars_and_nothing_else() {
        let post = device_request::<Calendar>(
            &Consent::given_at_a_terminal(),
            &Endpoints::microsoft(),
            Authority::Organizations,
        );
        assert_eq!(
            field(&post, "scope"),
            "https://graph.microsoft.com/Calendars.ReadWrite offline_access"
        );
        assert_eq!(field(&post, "client_id"), CLIENT_ID);
        let refresh = refresh_request::<Calendar>(
            &Endpoints::at("http://127.0.0.1:9"),
            Authority::Organizations,
            &Secret::new("r-1"),
        );
        assert_eq!(
            refresh.url,
            "http://127.0.0.1:9/login/organizations/oauth2/v2.0/token"
        );
        assert_eq!(field(&refresh, "scope"), scope::<Calendar>());
        assert_eq!(Calendar::SLOT, "calendar");
    }

    /// An administrator's approval is a class of its own whichever step meets it, so no step
    /// reads it as a lapse a new sign-in mends.
    #[test]
    fn a_refusal_that_needs_an_administrator_is_its_own_class() {
        for (code, error_codes) in [
            ("AADSTS65001", json!([65001])),
            ("AADSTS90094", json!([90094])),
            ("AADSTS90095", json!(null)),
        ] {
            let body = json!({
                "error": "consent_required",
                "error_description": format!("{code}: Need admin approval"),
                "error_codes": error_codes,
            });
            let refused = answered(400, body);
            assert!(
                matches!(
                    read_poll::<Calendar>(&refused, 5),
                    Err(OauthError::AdminApproval { .. })
                ),
                "{code}"
            );
            assert!(
                matches!(
                    read_silent::<Calendar>(&refused, "her@study.test"),
                    Ok(Silent::NeedsLogin(NeedsLogin::AdminApproval {
                        slot: "calendar",
                        ..
                    }))
                ),
                "{code}"
            );
            assert!(matches!(
                read_device_code(&refused),
                Err(OauthError::AdminApproval { .. })
            ));
        }
        let other = answered(
            400,
            json!({"error": "invalid_grant", "error_description": "AADSTS70008: expired"}),
        );
        assert!(matches!(
            read_poll::<Calendar>(&other, 5),
            Err(OauthError::Refused { .. })
        ));
    }

    #[test]
    fn one_response_yields_one_kind_of_token_per_protocol() {
        let body = serde_json::json!({
            "access_token": "at-1",
            "refresh_token": "rt-2",
            "expires_in": 3599,
        });
        let as_imap: Minted<Imap> =
            read_refresh::<Imap>(&answered(200, body.clone())).expect("a grant");
        let as_smtp: Minted<Smtp> = read_refresh::<Smtp>(&answered(200, body)).expect("a grant");

        assert_eq!(as_imap.token.secret().expose(), "at-1");
        assert_eq!(as_smtp.token.secret().expose(), "at-1");
        assert_eq!(as_imap.refresh.as_ref().map(Secret::expose), Some("rt-2"));
        assert_eq!(as_imap.expires_in_seconds, 3599);

        assert_eq!(Imap::SLOT, "imap");
        assert_eq!(Smtp::SLOT, "smtp");
        assert_ne!(Imap::SLOT, Smtp::SLOT);
    }

    /// A secret never prints itself in these records.
    #[test]
    fn nothing_here_debug_prints_a_secret() {
        let minted: Minted<Imap> = read_refresh::<Imap>(&answered(
            200,
            serde_json::json!({"access_token": "hunter2", "refresh_token": "hunter3"}),
        ))
        .expect("a grant");
        let shown = format!("{minted:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("hunter3"), "{shown}");
    }

    #[test]
    fn the_device_request_asks_for_the_protocols_scope() {
        let consent = Consent::given_at_a_terminal();
        let post = device_request::<Smtp>(&consent, &Endpoints::microsoft(), Authority::Consumers);
        assert_eq!(
            post.url,
            "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
        );
        assert_eq!(
            field(&post, "scope"),
            "https://outlook.office.com/SMTP.Send offline_access"
        );
    }

    #[test]
    fn a_device_code_answer_carries_the_operators_own_words() {
        let code = read_device_code(&answered(
            200,
            serde_json::json!({
                "device_code": "dc-1",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://microsoft.com/devicelogin",
                "message": "To sign in, use a web browser to open …",
                "interval": 7,
                "expires_in": 600,
            }),
        ))
        .expect("a device code");
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.verification_uri, "https://microsoft.com/devicelogin");
        assert!(code.message.starts_with("To sign in"));
        assert_eq!(code.interval_seconds, 7);
        assert_eq!(code.expires_in_seconds, 600);
        assert!(!format!("{code:?}").contains("dc-1"));
    }

    #[test]
    fn a_lapsed_grant_reads_as_needs_login_and_an_outage_does_not() {
        for lapse in Lapse::ALL {
            let read = read_silent::<Imap>(
                &answered(
                    400,
                    serde_json::json!({"error": lapse.code(), "error_description": "AADSTS70008: expired"}),
                ),
                "her@study.test",
            )
            .expect("a lapse is an answer");
            assert_eq!(
                read,
                Silent::NeedsLogin(NeedsLogin::Lapsed {
                    email: "her@study.test".to_owned(),
                    slot: "imap",
                    code: lapse,
                    description: "AADSTS70008: expired".to_owned(),
                })
            );
        }
        let outage = read_silent::<Imap>(
            &answered(503, serde_json::json!({"error": "temporarily_unavailable"})),
            "her@study.test",
        )
        .expect_err("not the owner's to fix");
        assert!(matches!(outage, OauthError::Refused { .. }));
        let minted = read_silent::<Imap>(
            &answered(200, serde_json::json!({"access_token": "at"})),
            "her@study.test",
        )
        .expect("a grant");
        assert!(matches!(minted, Silent::Token(_)));
    }
}
