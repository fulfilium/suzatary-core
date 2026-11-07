//! FCM v1: a service-account key buys a bearer, and one POST per device carries a DATA message
//! with no `notification` block, so Google draws nothing this app did not compose.

use serde::{Deserialize, Serialize};
use suzatary_core::capability::{Printed, Secret};

use crate::http::{Http, Req};
use crate::secrets::{Resolves, SecretError};
use crate::tokens::{Bearer, Fresh, Grant};
use suzatary_core::mail::shorten as truncate;

const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// A parsed FCM v1 service-account key.
#[derive(Debug, Clone)]
pub struct ServiceAccount {
    /// The Firebase project the `messages:send` URL names.
    pub project_id: String,
    /// The JWT's `iss`.
    pub client_email: String,
    /// The RSA key that signs the JWT.
    pub private_key_pem: Secret,
    /// Where the JWT is exchanged for a bearer.
    pub token_uri: String,
}

#[derive(Deserialize)]
struct RawServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    #[serde(default = "default_token_uri")]
    token_uri: String,
}

fn default_token_uri() -> String {
    DEFAULT_TOKEN_URI.to_owned()
}

impl ServiceAccount {
    /// A key file's JSON, or `None`.
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        let raw: RawServiceAccount = serde_json::from_str(text).ok()?;
        Some(Self {
            project_id: raw.project_id,
            client_email: raw.client_email,
            private_key_pem: Secret::new(raw.private_key),
            token_uri: raw.token_uri,
        })
    }

    /// The base64 of a key file's JSON, or `None`.
    #[must_use]
    pub fn from_base64(raw: &str) -> Option<Self> {
        use base64::Engine as _;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(raw.trim())
            .ok()?;
        let text = String::from_utf8(decoded).ok()?;
        Self::from_json(&text)
    }
}

/// `FCM_SERVICE_ACCOUNT_B64`, parsed, or `None` when it is unset or does not parse.
///
/// # Errors
/// [`SecretError`] when the store could not answer.
pub fn credentials(
    secrets: &dyn Resolves,
    reason: &str,
) -> Result<Option<ServiceAccount>, SecretError> {
    Ok(secrets
        .get("FCM_SERVICE_ACCOUNT_B64", reason)?
        .and_then(|secret| ServiceAccount::from_base64(secret.expose())))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    iat: i64,
    exp: i64,
}

/// The RS256 JWT Google exchanges for a bearer.
fn assertion(account: &ServiceAccount, now: i64, lifetime: i64) -> Result<String, String> {
    let claims = Claims {
        iss: account.client_email.clone(),
        scope: FCM_SCOPE.to_owned(),
        aud: account.token_uri.clone(),
        iat: now,
        exp: now + lifetime,
    };
    let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key_pem.expose().as_bytes())
        .map_err(|error| error.to_string())?;
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
        &claims,
        &key,
    )
    .map_err(|error| error.to_string())
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    #[serde(default = "default_expires_in")]
    expires_in: u64,
}

fn default_expires_in() -> u64 {
    3600
}

/// Google's push service: one data message per registered phone.
pub const API: &str = "https://fcm.googleapis.com";

fn message(token: &str, push: &Printed) -> serde_json::Value {
    serde_json::json!({
        "message": {
            "token": token,
            "data": {"title": push.title, "body": push.body, "click": push.click},
            "android": {"priority": "HIGH"},
        }
    })
}

/// What one device's send came to.
pub(super) enum Sent {
    Accepted,
    /// FCM says the device token is gone for good.
    Gone,
    Failed(String),
}

impl Grant for ServiceAccount {
    type Token = Secret;
    type Error = String;

    fn mint(&self, http: &dyn Http, now: u64) -> Result<Fresh<Secret>, String> {
        let jwt = assertion(self, i64::try_from(now).unwrap_or(i64::MAX), 3600)?;
        let resp = http
            .send(Req::form(
                &self.token_uri,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                    ("assertion", &jwt),
                ],
            ))
            .map_err(|error| error.to_string())?;
        if resp.status != 200 {
            return Err(format!(
                "token exchange answered {}: {}",
                resp.status,
                truncate(&resp.text(), 300)
            ));
        }
        let answer: TokenAnswer =
            serde_json::from_slice(&resp.body).map_err(|error| error.to_string())?;
        Ok(Fresh {
            token: Secret::new(answer.access_token),
            lifetime: answer.expires_in,
        })
    }
}

/// A service account and the bearer it last bought.
pub(super) struct Sender(Bearer<ServiceAccount>);

impl Sender {
    pub(super) fn new(account: ServiceAccount) -> Self {
        Self(Bearer::new(account))
    }

    /// One device's send: one device's failure is data for the fan-out.
    pub(super) fn send(&self, http: &dyn Http, now: u64, token: &str, push: &Printed) -> Sent {
        let bearer = match self.0.token(http, now) {
            Ok(bearer) => bearer,
            Err(error) => return Sent::Failed(format!("FCM sign-in: {}", truncate(&error, 300))),
        };
        let url = format!(
            "{API}/v1/projects/{}/messages:send",
            self.0.grant().project_id
        );
        match http.send(
            Req::json(url, &message(token, push))
                .header("Authorization", format!("Bearer {}", bearer.expose())),
        ) {
            Ok(resp) if resp.status == 200 => Sent::Accepted,
            Ok(resp) if resp.status == 404 && resp.text().contains("UNREGISTERED") => Sent::Gone,
            Ok(resp) => Sent::Failed(format!(
                "FCM answered {}: {}",
                resp.status,
                truncate(&resp.text(), 300)
            )),
            Err(error) => Sent::Failed(format!("FCM: {}", truncate(&error.to_string(), 300))),
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn the_fcm_body_is_a_data_message_with_no_notification_block() {
        let push = suzatary_core::capability::Push::Agent {
            authored: 2,
            changes: 0,
        }
        .printed("https://box.example.ts.net");
        let body = message("a-device-token", &push);
        assert_eq!(
            body,
            serde_json::json!({
                "message": {
                    "token": "a-device-token",
                    "data": {
                        "title": "Suzatary",
                        "body": "2 new or updated todos from the agent",
                        "click": "https://box.example.ts.net/#/",
                    },
                    "android": {"priority": "HIGH"},
                }
            })
        );
        assert!(
            body["message"].get("notification").is_none(),
            "an FCM `notification` block would let Google's push service draw a picture this app did not compose"
        );
    }

    /// A throwaway RSA key that only signs a test JWT; it backs nothing and is no credential.
    pub(crate) const TEST_PRIVATE_KEY_PEM: &str = include_str!("../../tests/fixtures/fcm-key.pem");

    const TEST_PUBLIC_KEY_PEM: &str = include_str!("../../tests/fixtures/fcm-key.pub.pem");

    pub(crate) fn test_account() -> ServiceAccount {
        ServiceAccount {
            project_id: "suzatary-test".to_owned(),
            client_email: "suzatary-push@example.iam.gserviceaccount.com".to_owned(),
            private_key_pem: Secret::new(TEST_PRIVATE_KEY_PEM),
            token_uri: DEFAULT_TOKEN_URI.to_owned(),
        }
    }

    #[test]
    fn the_assertion_carries_exactly_the_five_claims_google_wants() {
        let account = test_account();
        let jwt = assertion(&account, 1_757_000_000, 3600).expect("the throwaway key signs");

        let key = jsonwebtoken::DecodingKey::from_rsa_pem(TEST_PUBLIC_KEY_PEM.as_bytes())
            .expect("a valid test key");
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.validate_exp = false;
        // Only the token endpoint checks `aud`; this reads the claims.
        validation.validate_aud = false;
        validation.required_spec_claims.clear();
        let decoded = jsonwebtoken::decode::<Claims>(&jwt, &key, &validation)
            .expect("the signature verifies");

        assert_eq!(decoded.header.alg, jsonwebtoken::Algorithm::RS256);
        assert_eq!(decoded.claims.iss, account.client_email);
        assert_eq!(decoded.claims.scope, FCM_SCOPE);
        assert_eq!(decoded.claims.aud, account.token_uri);
        assert_eq!(decoded.claims.iat, 1_757_000_000);
        assert_eq!(decoded.claims.exp, 1_757_003_600);
    }
}
