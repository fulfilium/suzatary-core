//! Microsoft `OAuth2` as a sans-I/O state machine; `suzatary_io::oauth` is the transport.
//!
//! # The three rules, each a type
//!
//! 1. [`Minted<P>`] carries `P::Token`, so [`ImapToken`] and [`SmtpToken`] cannot be confused.
//! 2. [`Protocol::SLOT`] keeps grants apart, so a refresh for one scope never yields the other.
//! 3. [`device_request`] needs a [`Consent`], which nothing unattended holds or can build.
//!
//! The msal token cache is deliberately not read; the owner re-authorises once instead.

use crate::capability::{ImapToken, Secret, SmtpToken};
use crate::mail::account::Authority;

/// `FairEmail`'s public client id, kept because the existing consents were granted to it.
pub const CLIENT_ID: &str = "3514cf2c-e7a3-45a2-80d4-6a3c3498eca0";

/// Requested beside the protocol scope so the grant is refreshable without a prompt.
const OFFLINE_ACCESS: &str = "offline_access";

mod sealed {
    /// Closed so [`super::Protocol`] has exactly the two implementors declared here.
    pub trait Sealed {}
}

/// Which protocol a token is for: its scope, its token type and its refresh grant's slot.
pub trait Protocol: sealed::Sealed {
    /// What a successful mint yields; the bounds let [`Minted`] and [`Polled`] derive.
    type Token: std::fmt::Debug + Clone + PartialEq + Eq;

    /// The Microsoft scope this protocol's tokens are minted under.
    const SCOPE: &'static str;

    /// The token store's slot for this protocol's refresh grant, one per consent.
    const SLOT: &'static str;

    /// Wrap a minted secret in this protocol's token type.
    fn token(secret: Secret) -> Self::Token;
}

/// Reading a mailbox over IMAP.
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

/// A claim that a human is watching, which the device-code flow requires.
///
/// Unattended paths cannot make one, so an expiry there is a refusal naming the verb, not a stall.
#[derive(Debug, Clone, Copy)]
pub struct Consent(());

impl Consent {
    /// Claim that an operator is at a terminal waiting to type a code.
    ///
    /// Called only by `suzatary auth` and by tests.
    #[must_use]
    pub fn given_at_a_terminal() -> Self {
        Self(())
    }

    /// Claim that the owner tapped a proposal, witnessed by the gate-minted [`Tap`].
    ///
    /// [`Tap`]: crate::secretary::tap::Tap
    #[must_use]
    pub fn given_by_tap(_tap: &crate::secretary::tap::Tap) -> Self {
        Self(())
    }
}

/// One form POST the flow needs performed, as a value rather than a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormPost {
    /// Where to post it.
    pub url: String,
    /// `application/x-www-form-urlencoded` fields, in a fixed order a test can compare.
    pub form: Vec<(String, String)>,
}

/// What a transport read back: the status and the parsed JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    /// The HTTP status.
    pub status: u16,
    /// The body, parsed; a non-JSON body is [`OauthError::Unreadable`] before it gets here.
    pub body: serde_json::Value,
}

/// The flow refused, or the answer could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OauthError {
    /// The endpoint answered a body that is not JSON or lacks the step's field.
    #[error("the {step} response could not be read: {why}")]
    Unreadable {
        /// Which step: "device code", "token".
        step: &'static str,
        /// What was missing or wrong.
        why: String,
    },
    /// The endpoint said no, carried whole because operators search `AADSTS` codes.
    #[error("{code}: {description}")]
    Refused {
        /// Microsoft's `error` field.
        code: String,
        /// Its `error_description`, or the body when it named none.
        description: String,
    },
    /// A device flow asked for a non-Microsoft account.
    #[error("{email} is not a Microsoft account, so it has no sign-in to run")]
    NotMicrosoft {
        /// The account.
        email: String,
    },
}

/// What a silent access answers: a token, or that a person must sign in, never a prompt.
///
/// A sum rather than an error so a caller cannot `?` past a needed login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Silent<T> {
    /// Minted without asking anybody.
    Token(T),
    /// Only a person can fix this.
    NeedsLogin(NeedsLogin),
}

impl<T> Silent<T> {
    /// The token, or the needed login as an error, for paths that cannot proceed without one.
    ///
    /// # Errors
    /// [`NeedsLogin`].
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
    /// This store holds no grant for the protocol.
    #[error(
        "no {slot} authorisation for {email} — run `suzatary auth {email} --scope {slot}` at a terminal"
    )]
    NeverGranted {
        /// The account.
        email: String,
        /// Which protocol's grant is missing.
        slot: &'static str,
    },
    /// Microsoft refused the stored grant in words only a sign-in answers.
    #[error(
        "the {slot} authorisation for {email} has lapsed ({code}: {description}) — run `suzatary auth {email} --scope {slot}` at a terminal"
    )]
    Lapsed {
        /// The account.
        email: String,
        /// Which protocol's grant lapsed.
        slot: &'static str,
        /// Microsoft's `error` field.
        code: String,
        /// Its `error_description`.
        description: String,
    },
    /// A Google account holds no grant, or Google refused the one it holds: revoked, or ended
    /// after 7 days by a client left in Testing (`crate::google`).
    #[error(
        "the Google sign-in for {email} is missing or has lapsed ({why}) — sign it in again from \
         its card in the Inbox; a Google client left in Testing ends each sign-in after 7 days, \
         so publish it to In production"
    )]
    Google {
        /// The account.
        email: String,
        /// What the store or Google said.
        why: String,
    },
}

/// Microsoft's `error` codes for a grant only a person can renew.
const LAPSED: [&str; 4] = [
    "invalid_grant",
    "interaction_required",
    "consent_required",
    "login_required",
];

/// The tenant's device-code endpoint.
fn devicecode_url(authority: Authority) -> String {
    format!("https://login.microsoftonline.com/{authority}/oauth2/v2.0/devicecode")
}

/// The tenant's token endpoint, shared by the device-flow poll and the refresh grant.
fn token_url(authority: Authority) -> String {
    format!("https://login.microsoftonline.com/{authority}/oauth2/v2.0/token")
}

/// The protocol's own scope plus `offline_access`; one protocol per consent, never two.
#[must_use]
pub fn scope<P: Protocol>() -> String {
    format!("{} {OFFLINE_ACCESS}", P::SCOPE)
}

/// Step one: ask for a device code, which requires a [`Consent`].
#[must_use]
pub fn device_request<P: Protocol>(_consent: &Consent, authority: Authority) -> FormPost {
    FormPost {
        url: devicecode_url(authority),
        form: vec![
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            ("scope".to_owned(), scope::<P>()),
        ],
    }
}

/// The code an operator types, and what to tell them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    /// The opaque code this machine polls with. Never printed.
    pub device_code: Secret,
    /// The short code the operator types into the browser.
    pub user_code: String,
    /// Where they type it.
    pub verification_uri: String,
    /// Microsoft's own sentence, printed verbatim.
    pub message: String,
    /// Seconds to wait between polls, as the endpoint asked.
    pub interval_seconds: u64,
    /// Seconds until the code is dead.
    pub expires_in_seconds: u64,
}

/// Read step one's answer.
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

/// Step two: ask whether they have finished yet.
#[must_use]
pub fn poll_request(authority: Authority, code: &DeviceCode) -> FormPost {
    FormPost {
        url: token_url(authority),
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

/// What a poll answered; `Pending` carries the server's requested wait so `slow_down` is obeyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Polled<P: Protocol> {
    /// Nobody has typed the code yet; wait this many seconds and poll again.
    Pending {
        /// The interval to use for the next poll.
        wait_seconds: u64,
    },
    /// Done: the grant, for this protocol only.
    Granted(Minted<P>),
}

/// A grant for one protocol, whose `token` cannot become the other kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Minted<P: Protocol> {
    /// The access token, as this protocol's type.
    pub token: P::Token,
    /// The refresh token to store, when issued; a rotated one replaces the old.
    pub refresh: Option<Secret>,
    /// How long the access token is good for.
    pub expires_in_seconds: u64,
}

/// Read a poll's answer.
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

/// Ask for a fresh access token for this protocol, from this protocol's grant.
#[must_use]
pub fn refresh_request<P: Protocol>(authority: Authority, refresh: &Secret) -> FormPost {
    FormPost {
        url: token_url(authority),
        form: vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            ("refresh_token".to_owned(), refresh.expose().to_owned()),
            ("scope".to_owned(), scope::<P>()),
        ],
    }
}

/// Read a refresh's answer as this protocol's token.
fn read_refresh<P: Protocol>(answered: &Answered) -> Result<Minted<P>, OauthError> {
    if let Some(refused) = refusal(answered) {
        return Err(refused);
    }
    read_grant::<P>(answered)
}

/// Read a silent refresh, turning a lapse into [`NeedsLogin::Lapsed`]; other refusals stay errors.
///
/// # Errors
/// [`OauthError`] for a refusal that is not a lapse, or an unreadable answer.
pub fn read_silent<P: Protocol>(
    answered: &Answered,
    email: &str,
) -> Result<Silent<Minted<P>>, OauthError> {
    match read_refresh::<P>(answered) {
        Ok(minted) => Ok(Silent::Token(minted)),
        Err(OauthError::Refused { code, description }) if LAPSED.contains(&code.as_str()) => {
            Ok(Silent::NeedsLogin(NeedsLogin::Lapsed {
                email: email.to_owned(),
                slot: P::SLOT,
                code,
                description,
            }))
        }
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

/// Microsoft's `error` field, when the body carries one.
fn error_code(answered: &Answered) -> Option<String> {
    answered
        .body
        .get("error")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// The refusal this answer is, or `None` when it is not one.
///
/// A non-2xx with no `error` field is still a refusal, named by its status.
fn refusal(answered: &Answered) -> Option<OauthError> {
    match error_code(answered) {
        Some(code) => Some(OauthError::Refused {
            description: answered
                .body
                .get("error_description")
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| answered.body.to_string(), str::to_owned),
            code,
        }),
        None if !(200..300).contains(&answered.status) => Some(OauthError::Refused {
            code: format!("HTTP {}", answered.status),
            description: answered.body.to_string(),
        }),
        None => None,
    }
}

#[cfg(test)]
mod tests {
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

    /// The two protocols ask for different scopes on the wire.
    #[test]
    fn a_refresh_names_exactly_one_protocols_scope() {
        let refresh = Secret::new("r-1");
        let imap = refresh_request::<Imap>(Authority::Organizations, &refresh);
        let smtp = refresh_request::<Smtp>(Authority::Consumers, &refresh);

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

    /// One response read as each protocol yields two different token types.
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

    /// The device request asks for the protocol's scope.
    #[test]
    fn the_device_request_asks_for_the_protocols_scope() {
        let consent = Consent::given_at_a_terminal();
        let post = device_request::<Smtp>(&consent, Authority::Consumers);
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

    /// A lapsed grant is a login to ask for; an outage stays an error.
    #[test]
    fn a_lapsed_grant_reads_as_needs_login_and_an_outage_does_not() {
        for code in LAPSED {
            let read = read_silent::<Imap>(
                &answered(
                    400,
                    serde_json::json!({"error": code, "error_description": "AADSTS70008: expired"}),
                ),
                "her@study.test",
            )
            .expect("a lapse is an answer");
            assert_eq!(
                read,
                Silent::NeedsLogin(NeedsLogin::Lapsed {
                    email: "her@study.test".to_owned(),
                    slot: "imap",
                    code: code.to_owned(),
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
