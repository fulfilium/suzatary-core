//! The consent, the callback, the exchange and the refresh: each request as a value and each
//! answer read into a parsed one.

use std::collections::BTreeSet;

use base64::Engine as _;

use super::{Challenge, Client, ClientId, RedirectUri, Scope, Scopes, State, Verifier};
use crate::mail::address::EmailAddress;
use crate::mail::oauth::{Answered, FormPost, NeedsLogin, OauthError, Silent};
use suzatary_base::capability::Secret;

/// Where the box asks Google: its consent page, its token endpoint and its Calendar API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    authorize: String,
    token: String,
    calendar: String,
    mail: String,
}

impl Endpoints {
    /// Google's own.
    #[must_use]
    pub fn google() -> Self {
        Self {
            authorize: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
            token: "https://oauth2.googleapis.com/token".to_owned(),
            calendar: "https://www.googleapis.com/calendar/v3".to_owned(),
            mail: "https://gmail.googleapis.com/gmail/v1".to_owned(),
        }
    }

    /// The same paths under `base`, a fake Google a test serves; the deployment's wiring names
    /// [`Endpoints::google`] alone.
    #[must_use]
    pub fn at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            authorize: format!("{base}/o/oauth2/v2/auth"),
            token: format!("{base}/token"),
            calendar: format!("{base}/calendar/v3"),
            mail: format!("{base}/gmail/v1"),
        }
    }

    /// The token endpoint, which lieer is told to refresh its grant at.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The Calendar API's root.
    #[must_use]
    pub fn calendar(&self) -> &str {
        &self.calendar
    }

    /// The Gmail API's root.
    #[must_use]
    pub fn mail(&self) -> &str {
        &self.mail
    }
}

/// The consent page's URL for `account`, asking for `scopes`, bound to `state` and to the
/// verifier whose `challenge` it carries.
///
/// `access_type=offline` and `prompt=consent` make Google issue a refresh token every time;
/// `include_granted_scopes=true` makes a later consent add to what the account already granted
/// rather than replace it, so a part asked for again is asked for alone.
#[must_use]
pub fn authorize(
    endpoints: &Endpoints,
    client: &ClientId,
    redirect: &RedirectUri,
    (state, challenge): (&State, &Challenge),
    scopes: &Scopes,
    account: &EmailAddress,
) -> String {
    format!(
        "{}?{}",
        endpoints.authorize,
        crate::url::encode_form(&[
            ("client_id", client.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", &scopes.requested()),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("include_granted_scopes", "true"),
            ("login_hint", account.as_str()),
        ])
    )
}

/// An authorization code, as a callback carries it: sent once, to the token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code(Secret);

impl Code {
    /// The code itself, for the token request alone.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

/// What a callback says, read once from its query: the state it names, and either a code or
/// Google's refusal. A query naming no state the box could have drawn is no callback at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callback {
    /// The owner consented.
    Code {
        /// The state Google handed back.
        state: State,
        /// The code to exchange.
        code: Code,
    },
    /// The owner declined, or Google refused the request.
    Refused {
        /// The state Google handed back.
        state: State,
        /// Google's `error`, which is no secret.
        error: String,
    },
}

impl Callback {
    /// The callback a query's `state`, `code` and `error` say.
    ///
    /// # Errors
    /// Why the query is no callback: no state the box could have drawn, or neither a code nor
    /// an error. Never quotes a code.
    pub fn of_query(
        state: Option<&str>,
        code: Option<&str>,
        error: Option<&str>,
    ) -> Result<Self, String> {
        let state = state
            .and_then(State::parse)
            .ok_or("the callback names no state this box draws")?;
        match (code, error) {
            (_, Some(error)) => Ok(Callback::Refused {
                state,
                error: error.chars().take(100).collect(),
            }),
            (Some(code), None)
                if !code.is_empty()
                    && code.len() <= 1024
                    && code.bytes().all(|b| b.is_ascii_graphic()) =>
            {
                Ok(Callback::Code {
                    state,
                    code: Code(Secret::new(code)),
                })
            }
            (Some(_), None) => Err("the callback's code is not one Google issues".to_owned()),
            (None, None) => Err("the callback carries neither a code nor an error".to_owned()),
        }
    }

    /// The state it names.
    #[must_use]
    pub fn state(&self) -> &State {
        match self {
            Callback::Code { state, .. } | Callback::Refused { state, .. } => state,
        }
    }
}

/// The token request that trades `code` for a grant: the client, the verifier the consent's
/// challenge was made from and the redirect the consent named (RFC 6749 §4.1.3).
#[must_use]
pub fn exchange(
    endpoints: &Endpoints,
    client: &Client,
    code: &Code,
    verifier: &Verifier,
    redirect: &RedirectUri,
) -> FormPost {
    FormPost {
        url: endpoints.token.clone(),
        form: vec![
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), code.expose().to_owned()),
            ("client_id".to_owned(), client.id.as_str().to_owned()),
            (
                "client_secret".to_owned(),
                client.secret.expose().to_owned(),
            ),
            ("redirect_uri".to_owned(), redirect.as_str().to_owned()),
            ("code_verifier".to_owned(), verifier.expose().to_owned()),
        ],
    }
}

/// A refresh token: what the box keeps of a grant, in the secret store alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshToken(Secret);

impl RefreshToken {
    /// Read one, as the store gives it back.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        (!text.is_empty() && text.len() <= 2048 && text.bytes().all(|b| b.is_ascii_graphic()))
            .then(|| Self(Secret::new(text)))
    }

    /// The token itself, for the token endpoint and the store alone.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }

    /// As a secret, for the store.
    #[must_use]
    pub fn secret(&self) -> &Secret {
        &self.0
    }
}

/// A grant, read: what to keep, what Google says it granted, and the account it was given for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchanged {
    /// The refresh token to keep.
    pub refresh: RefreshToken,
    /// The parts it holds, which may be fewer than were asked for.
    pub granted: BTreeSet<Scope>,
    /// The account its ID token names.
    pub account: EmailAddress,
}

/// Read the exchange's answer, for a consent given for `asked`: a grant holding a refresh token,
/// naming `asked` as its verified address in its `id_token`.
///
/// The `id_token` came straight from the token endpoint over TLS, so its claims are read without
/// checking its signature (OIDC Core, section 3.1.3.7).
///
/// # Errors
/// Google's refusal (an `invalid_grant` for a code used twice or past its time), or a grant that
/// holds no refresh token, names no verified account, or names another account than `asked`.
pub fn read_exchange(answered: &Answered, asked: &EmailAddress) -> Result<Exchanged, OauthError> {
    if let Some(refused) = refusal(answered) {
        return Err(refused);
    }
    let field = |name: &str| answered.body.get(name).and_then(serde_json::Value::as_str);
    let unreadable = |why: &str| OauthError::Unreadable {
        step: "token",
        why: why.to_owned(),
    };
    field("access_token").ok_or_else(|| unreadable("no access_token"))?;
    let refresh = field("refresh_token")
        .and_then(RefreshToken::parse)
        .ok_or_else(|| unreadable("the grant carried no refresh token to keep"))?;
    let account = field("id_token")
        .and_then(verified_email)
        .ok_or_else(|| unreadable("the grant names no verified account"))?;
    if !account.as_str().eq_ignore_ascii_case(asked.as_str()) {
        return Err(OauthError::Refused {
            code: "wrong_account".to_owned(),
            description: format!(
                "Google signed in {account}, and this sign-in was for {asked}: sign in again and \
                 choose {asked} on Google's page"
            ),
        });
    }
    Ok(Exchanged {
        refresh,
        granted: Scopes::granted(field("scope").unwrap_or_default()),
        account,
    })
}

/// The `email` an ID token's claims name, when they also say it is verified.
fn verified_email(id_token: &str) -> Option<EmailAddress> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let verified = match claims.get("email_verified") {
        Some(serde_json::Value::Bool(verified)) => *verified,
        Some(serde_json::Value::String(verified)) => verified == "true",
        _ => false,
    };
    verified
        .then(|| claims.get("email").and_then(serde_json::Value::as_str))
        .flatten()
        .and_then(|email| EmailAddress::new(email).ok())
}

/// The request for a fresh access token from a kept grant.
#[must_use]
pub fn refresh(endpoints: &Endpoints, client: &Client, token: &RefreshToken) -> FormPost {
    FormPost {
        url: endpoints.token.clone(),
        form: vec![
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".to_owned(), client.id.as_str().to_owned()),
            (
                "client_secret".to_owned(),
                client.secret.expose().to_owned(),
            ),
            ("refresh_token".to_owned(), token.expose().to_owned()),
        ],
    }
}

/// An access token, minted from a grant, good for `expires_in_seconds`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    /// The token, for an API's `Authorization` header alone.
    pub token: Secret,
    /// How long it is good for.
    pub expires_in_seconds: u64,
}

/// Google's `error` for a grant only a person can renew: revoked, expired (a client in Testing
/// ends each grant after 7 days), or its password changed.
const LAPSED: &str = "invalid_grant";

/// Read a refresh's answer for `account`: a token, or the sign-in a lapsed grant needs.
///
/// # Errors
/// A refusal no sign-in mends (an outage, a client Google no longer knows), or an answer with no
/// access token.
pub fn read_refresh(
    answered: &Answered,
    account: &EmailAddress,
) -> Result<Silent<AccessToken>, OauthError> {
    match refusal(answered) {
        Some(OauthError::Refused { code, description }) if code == LAPSED => {
            Ok(Silent::NeedsLogin(NeedsLogin::Google {
                email: account.as_str().to_owned(),
                why: format!("{code}: {description}"),
            }))
        }
        Some(refused) => Err(refused),
        None => {
            let token = answered
                .body
                .get("access_token")
                .and_then(serde_json::Value::as_str)
                .ok_or(OauthError::Unreadable {
                    step: "token",
                    why: "no access_token".to_owned(),
                })?;
            Ok(Silent::Token(AccessToken {
                token: Secret::new(token),
                expires_in_seconds: answered
                    .body
                    .get("expires_in")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(3600),
            }))
        }
    }
}

/// The refusal an answer is, or `None`: Google's `error` and `error_description`, or a status
/// outside 2xx named by itself.
fn refusal(answered: &Answered) -> Option<OauthError> {
    let field = |name: &str| answered.body.get(name).and_then(serde_json::Value::as_str);
    match field("error") {
        Some(code) => Some(OauthError::Refused {
            code: code.to_owned(),
            description: field("error_description").unwrap_or_default().to_owned(),
        }),
        None if !(200..300).contains(&answered.status) => Some(OauthError::Refused {
            code: format!("HTTP {}", answered.status),
            description: String::new(),
        }),
        None => None,
    }
}

/// A grant as the secret store keeps it: its refresh token, and the parts Google said it holds,
/// so the box can ask for a part it lacks alone. Printed on one line, as a secret store keeps a
/// value: the parts, by comma, a space, and the token, which holds no space; read back only
/// whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptGrant {
    /// The parts it holds; the mail always, since a grant without it is never kept.
    pub parts: Scopes,
    /// Its refresh token.
    pub refresh: RefreshToken,
}

impl KeptGrant {
    /// What the store keeps.
    #[must_use]
    pub fn print(&self) -> Secret {
        let parts: Vec<&str> = self.parts.iter().map(Scope::as_str).collect();
        Secret::new(format!("{} {}", parts.join(","), self.refresh.expose()))
    }

    /// What `text`, as the store gives it back, keeps; `None` for text the box did not print.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (parts, token) = text.split_once(' ')?;
        let parts = parts
            .split(',')
            .map(Scope::parse)
            .collect::<Option<Vec<Scope>>>()
            .and_then(Scopes::of)?;
        Some(Self {
            parts,
            refresh: RefreshToken::parse(token)?,
        })
    }
}

/// Whether an account is signed in with Google, and the parts its grant holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// No grant is kept for it.
    NotSignedIn,
    /// A grant is kept, holding these parts.
    Signed(Scopes),
}

/// The name an account's grant is kept under in the secret store: `GOOGLE_GRANT_` and the
/// account's maildir folder, in capitals.
#[must_use]
pub fn grant_name(account: &EmailAddress) -> String {
    format!(
        "GOOGLE_GRANT_{}",
        crate::mail::account::slug_of(account.as_str())
            .to_ascii_uppercase()
            .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::ClientSecret;

    /// A kept grant reads back as printed, and text the box did not print reads as nothing.
    #[test]
    fn a_kept_grant_reads_back_as_printed() {
        let kept = KeptGrant {
            parts: Scopes::of([Scope::Mail]).expect("a part"),
            refresh: RefreshToken::parse("1//r").expect("a token"),
        };
        assert_eq!(kept.print().expose(), "mail 1//r");
        assert_eq!(KeptGrant::parse(kept.print().expose()), Some(kept));
        let both = KeptGrant {
            parts: Scopes::all(),
            refresh: RefreshToken::parse("1//r").expect("a token"),
        };
        assert_eq!(KeptGrant::parse(both.print().expose()), Some(both));
        for text in [
            "1//r",
            " 1//r",
            "photos 1//r",
            "mail ",
            "mail,calendar 1// r",
            "mail\n1//r",
        ] {
            assert_eq!(KeptGrant::parse(text), None, "{text:?}");
        }
    }

    /// Bytes all of one value, so a draw is known.
    struct Fixed(u8);

    impl suzatary_base::capability::Entropy for Fixed {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(self.0);
            Ok(())
        }
    }

    fn ada() -> EmailAddress {
        EmailAddress::new("ada@example.test").expect("an address")
    }

    fn client() -> Client {
        Client {
            id: ClientId::parse("1234-abcd.apps.googleusercontent.com").expect("an id"),
            secret: ClientSecret::parse("GOCSPX-example").expect("a secret"),
        }
    }

    fn id_token(claims: &serde_json::Value) -> String {
        let part = |value: &serde_json::Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
        };
        format!(
            "{}.{}.signature",
            part(&serde_json::json!({"alg": "RS256"})),
            part(claims)
        )
    }

    fn granted(email: &str, scope: &str) -> Answered {
        Answered {
            status: 200,
            body: serde_json::json!({
                "access_token": "ya29.access",
                "refresh_token": "1//refresh",
                "expires_in": 3599,
                "scope": scope,
                "id_token": id_token(&serde_json::json!({"email": email, "email_verified": true})),
            }),
        }
    }

    fn field<'a>(post: &'a FormPost, name: &str) -> &'a str {
        post.form
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.as_str())
    }

    /// The consent carries the challenge and never the verifier, S256, the exact redirect, and
    /// asks for a refresh token and for scopes added to what was granted.
    #[test]
    fn the_consent_asks_with_the_challenge_and_never_the_verifier() {
        let verifier = Verifier::draw(&Fixed(7)).expect("a verifier");
        let state = State::draw(&Fixed(8)).expect("a state");
        let redirect = RedirectUri::of_app("https://box.example.test").expect("a callback");
        let url = authorize(
            &Endpoints::google(),
            &client().id,
            &redirect,
            (&state, &verifier.challenge()),
            &Scopes::all(),
            &ada(),
        );
        assert!(
            url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?client_id=1234-abcd"),
            "{url}"
        );
        for part in [
            "redirect_uri=https%3A%2F%2Fbox.example.test%2Fapi%2Foauth%2Fgoogle%2Fcallback&",
            "response_type=code&",
            &format!("state={}&", state.as_str()),
            &format!("code_challenge={}&", verifier.challenge().as_str()),
            "code_challenge_method=S256&",
            "access_type=offline&",
            "prompt=consent&",
            "include_granted_scopes=true&",
            "login_hint=ada%40example.test",
            "calendar.events.owned",
        ] {
            assert!(url.contains(part), "{part} in {url}");
        }
        assert!(!url.contains("drive"), "{url}");
        assert!(!url.contains(verifier.expose()), "{url}");
        assert!(!url.contains("GOCSPX"), "{url}");
    }

    /// The exchange sends the code, the verifier and the same redirect, to the token endpoint.
    #[test]
    fn the_exchange_sends_the_verifier_and_the_same_redirect() {
        let redirect = RedirectUri::of_app("https://box.example.test").expect("a callback");
        let Ok(Callback::Code { code, .. }) = Callback::of_query(
            Some("dBjftJeZ4CVP-mJ92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            Some("4/0Ab-code"),
            None,
        ) else {
            panic!("a code");
        };
        let verifier = Verifier::draw(&Fixed(9)).expect("a verifier");
        let post = exchange(&Endpoints::google(), &client(), &code, &verifier, &redirect);
        assert_eq!(post.url, "https://oauth2.googleapis.com/token");
        assert_eq!(field(&post, "grant_type"), "authorization_code");
        assert_eq!(field(&post, "code"), "4/0Ab-code");
        assert_eq!(field(&post, "code_verifier"), verifier.expose());
        assert_eq!(field(&post, "redirect_uri"), redirect.as_str());
        assert_eq!(field(&post, "client_secret"), "GOCSPX-example");
    }

    /// A callback is read once into a sum: a code, a refusal, or no callback.
    #[test]
    fn a_callback_is_a_code_a_refusal_or_nothing() {
        let state = "dBjftJeZ4CVP-mJ92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert!(matches!(
            Callback::of_query(Some(state), None, Some("access_denied")),
            Ok(Callback::Refused { error, .. }) if error == "access_denied"
        ));
        assert!(Callback::of_query(None, Some("4/code"), None).is_err());
        assert!(Callback::of_query(Some("forged"), Some("4/code"), None).is_err());
        assert!(Callback::of_query(Some(state), None, None).is_err());
        assert!(Callback::of_query(Some(state), Some("a code"), None).is_err());
        let said = Callback::of_query(Some(state), Some("4/secret code"), None).expect_err("no");
        assert!(!said.contains("secret"), "{said}");
    }

    /// A grant is read into what to keep, what it holds and whose it is.
    #[test]
    fn a_grant_is_read_into_its_token_its_parts_and_its_account() {
        let read = read_exchange(
            &granted("ada@example.test", &Scopes::all().requested()),
            &ada(),
        )
        .expect("a grant");
        assert_eq!(read.refresh.expose(), "1//refresh");
        assert_eq!(read.account, ada());
        assert_eq!(read.granted, Scope::ALL.into_iter().collect());
        assert!(!format!("{read:?}").contains("1//refresh"));
    }

    /// A grant for another account, one naming no verified account and one with no refresh token
    /// are refused; a code used twice is Google's `invalid_grant`, carried whole.
    #[test]
    fn a_grant_for_another_account_or_without_a_refresh_token_is_refused() {
        let wrong = read_exchange(&granted("eve@example.test", "openid"), &ada())
            .expect_err("another account");
        assert!(wrong.to_string().contains("eve@example.test"), "{wrong}");
        let mut unverified = granted("ada@example.test", "openid");
        unverified.body["id_token"] = serde_json::Value::String(id_token(
            &serde_json::json!({"email": "ada@example.test", "email_verified": false}),
        ));
        assert!(read_exchange(&unverified, &ada()).is_err());
        let mut no_refresh = granted("ada@example.test", "openid");
        no_refresh.body["refresh_token"] = serde_json::Value::Null;
        assert!(read_exchange(&no_refresh, &ada()).is_err());
        let replayed = Answered {
            status: 400,
            body: serde_json::json!({"error": "invalid_grant", "error_description": "Malformed auth code."}),
        };
        assert_eq!(
            read_exchange(&replayed, &ada()),
            Err(OauthError::Refused {
                code: "invalid_grant".to_owned(),
                description: "Malformed auth code.".to_owned(),
            })
        );
    }

    /// A revoked or expired grant needs the owner's sign-in; an outage stays an error.
    #[test]
    fn a_lapsed_grant_needs_a_sign_in_and_an_outage_does_not() {
        let lapsed = Answered {
            status: 400,
            body: serde_json::json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
        };
        assert!(matches!(
            read_refresh(&lapsed, &ada()),
            Ok(Silent::NeedsLogin(NeedsLogin::Google { .. }))
        ));
        let outage = Answered {
            status: 503,
            body: serde_json::json!({}),
        };
        assert!(read_refresh(&outage, &ada()).is_err());
        let fresh = Answered {
            status: 200,
            body: serde_json::json!({"access_token": "ya29.a", "expires_in": 100}),
        };
        assert!(matches!(
            read_refresh(&fresh, &ada()),
            Ok(Silent::Token(AccessToken {
                expires_in_seconds: 100,
                ..
            }))
        ));
        let post = refresh(
            &Endpoints::google(),
            &client(),
            &RefreshToken::parse("1//r").expect("a token"),
        );
        assert_eq!(field(&post, "grant_type"), "refresh_token");
    }

    #[test]
    fn a_grant_is_kept_under_the_accounts_name() {
        assert_eq!(grant_name(&ada()), "GOOGLE_GRANT_ADA_AT_EXAMPLE_TEST");
    }
}
