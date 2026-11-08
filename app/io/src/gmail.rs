//! The Gmail API on lieer's grant: the grant read and a token minted from it, for the send
//! transport, and the read-only search an older search makes.
//!
//! The grant file is read-only here because lieer owns and rewrites it; a caller mints its own
//! access token and persists nothing, so the two never race.

use std::path::Path;
use std::time::Duration;

use base64::Engine as _;
use serde::Deserialize;
use suzatary_core::capability::Secret;
use suzatary_core::mail::older::Matched;

use crate::http::{Http, Req};
use crate::tokens::{Fresh, Grant};

/// Characters of an error body kept, so a whole HTML page never lands in a refusal.
pub const ERROR_CAP: usize = 300;

const TOKEN_TIMEOUT: Duration = Duration::from_secs(30);

const READ_TIMEOUT: Duration = Duration::from_mins(1);

const MESSAGES_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";

/// The API's own page limit.
pub const PAGE: usize = 500;

/// lieer's grant could not be read or could not mint a token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// The credentials file is missing or not lieer's shape.
    #[error("{0}")]
    Credentials(String),
    /// Google refused the refresh, or could not be reached.
    #[error("{0}")]
    Auth(String),
}

impl GrantError {
    #[must_use]
    pub fn why(&self) -> &str {
        match self {
            Self::Credentials(why) | Self::Auth(why) => why,
        }
    }
}

/// lieer's grant, read from `.credentials.gmailieer.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lieer {
    client_id: String,
    client_secret: Secret,
    refresh_token: Secret,
    token_uri: String,
}

#[derive(Deserialize)]
struct RawLieer {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    token_uri: String,
}

impl Lieer {
    /// The grant in `text`, lieer's credentials file.
    pub fn parse(text: &str) -> Result<Self, GrantError> {
        let raw: RawLieer = serde_json::from_str(text)
            .map_err(|error| GrantError::Credentials(error.to_string()))?;
        Ok(Self {
            client_id: raw.client_id,
            client_secret: Secret::new(raw.client_secret),
            refresh_token: Secret::new(raw.refresh_token),
            token_uri: raw.token_uri,
        })
    }

    pub fn read(maildir: &Path) -> Result<Self, GrantError> {
        let path = maildir.join(".credentials.gmailieer.json");
        let text = std::fs::read_to_string(&path).map_err(|error| {
            GrantError::Credentials(format!("reading {}: {error}", path.display()))
        })?;
        Self::parse(&text)
    }
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: String,
    #[serde(default = "an_hour")]
    expires_in: u64,
}

fn an_hour() -> u64 {
    3600
}

impl Grant for Lieer {
    type Token = Secret;
    type Error = GrantError;

    fn mint(&self, http: &dyn Http, _now: u64) -> Result<Fresh<Secret>, GrantError> {
        let resp = http
            .send(
                Req::form(
                    &self.token_uri,
                    &[
                        ("client_id", self.client_id.as_str()),
                        ("client_secret", self.client_secret.expose()),
                        ("refresh_token", self.refresh_token.expose()),
                        ("grant_type", "refresh_token"),
                    ],
                )
                .timeout(TOKEN_TIMEOUT),
            )
            .map_err(|error| GrantError::Auth(error.to_string()))?;
        if resp.status != 200 {
            return Err(GrantError::Auth(format!(
                "HTTP {}: {}",
                resp.status,
                suzatary_core::spelling::shorten(&resp.text(), ERROR_CAP)
            )));
        }
        let answer: TokenAnswer = serde_json::from_slice(&resp.body)
            .map_err(|error| GrantError::Auth(error.to_string()))?;
        Ok(Fresh {
            token: Secret::new(answer.access_token),
            lifetime: answer.expires_in,
        })
    }
}

/// A Gmail message id: lowercase hex, lieer's file name for the message, so safe in a path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(String);

impl MessageId {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        (!text.is_empty()
            && text.len() <= 32
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        .then(|| Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One listing of the ids `q` matches, newest first, and how many matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// At most [`PAGE`] ids.
    pub ids: Vec<MessageId>,
    /// Exactly the listed count, or at least it when the server had another page.
    pub matched: Matched,
}

#[derive(Deserialize)]
struct ListAnswer {
    #[serde(default)]
    messages: Vec<ListedId>,
    #[serde(rename = "nextPageToken")]
    next: Option<String>,
}

#[derive(Deserialize)]
struct ListedId {
    id: String,
}

fn get(http: &dyn Http, token: &Secret, url: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
    let resp = http
        .send(
            Req::get(url)
                .header("Authorization", format!("Bearer {}", token.expose()))
                .timeout(READ_TIMEOUT)
                .limit(limit),
        )
        .map_err(|error| error.to_string())?;
    match resp.status {
        200 => Ok(Some(resp.body)),
        // A message deleted since the listing costs that message, not the search.
        404 => Ok(None),
        status => Err(format!(
            "HTTP {status}: {}",
            suzatary_core::spelling::shorten(&resp.text(), ERROR_CAP)
        )),
    }
}

/// The ids `q` matches, newest first, one page of at most [`PAGE`], spam and trash excluded.
pub fn list(http: &dyn Http, token: &Secret, q: &str) -> Result<Listed, String> {
    let url = format!(
        "{MESSAGES_URL}?maxResults={PAGE}&q={}",
        percent_encoding::utf8_percent_encode(q, percent_encoding::NON_ALPHANUMERIC)
    );
    let body = get(http, token, &url, 1024 * 1024)?
        .ok_or_else(|| "the messages listing answered 404".to_owned())?;
    let answer: ListAnswer =
        serde_json::from_slice(&body).map_err(|error| format!("the listing: {error}"))?;
    let listed = answer.messages.len();
    Ok(Listed {
        ids: answer
            .messages
            .iter()
            .filter_map(|listed| MessageId::parse(&listed.id))
            .collect(),
        matched: match answer.next {
            Some(_) => Matched::AtLeast(listed),
            None => Matched::Exactly(listed),
        },
    })
}

#[derive(Deserialize)]
struct Minimal {
    #[serde(rename = "sizeEstimate")]
    size: u64,
}

#[derive(Deserialize)]
struct Raw {
    raw: String,
}

/// One message as it was received, or `None` when it is gone or larger than `cap` bytes, asked
/// first.
pub fn message(
    http: &dyn Http,
    token: &Secret,
    id: &MessageId,
    cap: usize,
) -> Result<Fetched, String> {
    let url = format!("{MESSAGES_URL}/{}", id.as_str());
    let Some(body) = get(http, token, &format!("{url}?format=minimal"), 64 * 1024)? else {
        return Ok(Fetched::Gone);
    };
    let minimal: Minimal =
        serde_json::from_slice(&body).map_err(|error| format!("message {}: {error}", id.0))?;
    if minimal.size > cap as u64 {
        return Ok(Fetched::TooLarge);
    }
    let limit = (cap as u64) * 4 / 3 + 64 * 1024;
    let Some(body) = get(http, token, &format!("{url}?format=raw"), limit)? else {
        return Ok(Fetched::Gone);
    };
    let raw: Raw =
        serde_json::from_slice(&body).map_err(|error| format!("message {}: {error}", id.0))?;
    // URL-safe, padded or not: the API documents the alphabet and not the padding.
    let bytes = base64::engine::GeneralPurpose::new(
        &base64::alphabet::URL_SAFE,
        base64::engine::GeneralPurposeConfig::new()
            .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
    )
    .decode(&raw.raw)
    .map_err(|error| format!("message {}: {error}", id.0))?;
    Ok(if bytes.len() > cap {
        Fetched::TooLarge
    } else {
        Fetched::Message(bytes)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    Message(Vec<u8>),
    /// Larger than the cap, so not downloaded or not kept.
    TooLarge,
    /// Deleted since it was listed.
    Gone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{Resp, Scripted};

    #[test]
    fn a_message_id_is_lowercase_hex_and_nothing_else() {
        assert!(MessageId::parse("18c2f0a9b3d4e5f6").is_some());
        for bad in ["", "../x", "18C2", "a/b", &"a".repeat(33)] {
            assert!(MessageId::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_listing_sends_q_encoded_and_reads_more() {
        let http = Scripted::always(Ok(Resp::new(
            200,
            r#"{"messages":[{"id":"b2"},{"id":"../x"},{"id":"a1"}],"nextPageToken":"t"}"#,
        )));
        let listed = list(
            &http,
            &Secret::new("tok"),
            "subject:\"a b\" after:2023/01/01",
        )
        .expect("a listing");
        assert_eq!(
            listed.ids,
            [
                MessageId::parse("b2").expect("an id"),
                MessageId::parse("a1").expect("an id")
            ]
        );
        assert_eq!(listed.matched, Matched::AtLeast(3));
        let sent = http.sent();
        assert!(
            sent[0]
                .url
                .ends_with("?maxResults=500&q=subject%3A%22a%20b%22%20after%3A2023%2F01%2F01")
        );
        assert_eq!(
            crate::http::header(&sent[0].headers, "Authorization"),
            Some("Bearer tok")
        );
    }

    #[test]
    fn a_large_message_is_never_downloaded() {
        let http = Scripted::answering(vec![Ok(Resp::new(200, r#"{"sizeEstimate": 99}"#))]);
        let id = MessageId::parse("a1").expect("an id");
        assert_eq!(
            message(&http, &Secret::new("t"), &id, 10),
            Ok(Fetched::TooLarge)
        );
        assert_eq!(http.sent().len(), 1);
    }
}
