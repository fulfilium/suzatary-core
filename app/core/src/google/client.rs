//! The one Google Cloud OAuth client this box signs in with, as the owner made it.

use suzatary_base::capability::Secret;

use super::RedirectUri;

/// The name the client's id is stored under.
pub const CLIENT_ID: &str = "GOOGLE_CLIENT_ID";

/// The name the client's secret is stored under.
pub const CLIENT_SECRET: &str = "GOOGLE_CLIENT_SECRET";

/// The longest id or secret the box keeps; Google's are well under it.
const LONGEST: usize = 256;

/// A Google OAuth client's id: `<number>-<letters>.apps.googleusercontent.com`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId(String);

impl ClientId {
    /// Read one, or `None` for text Google does not issue as a client id.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let head = text.strip_suffix(".apps.googleusercontent.com")?;
        (text.len() <= LONGEST
            && !head.is_empty()
            && head
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
        .then(|| Self(text.to_owned()))
    }

    /// As the consent and the token request send it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Google OAuth client's secret: a value only the token endpoint is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSecret(Secret);

impl ClientSecret {
    /// Read one: printable, with no space, and not too long.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        (!text.is_empty() && text.len() <= LONGEST && text.bytes().all(|b| b.is_ascii_graphic()))
            .then(|| Self(Secret::new(text)))
    }

    /// The secret itself, for the token request alone.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

/// The client: its id, and its secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Its id.
    pub id: ClientId,
    /// Its secret.
    pub secret: ClientSecret,
}

/// Why a pasted client file was not kept. Each says what to do, and none quotes the secret.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientRefused {
    /// Not the JSON Google's Download JSON gives.
    #[error(
        "that is not the file Google's Download JSON gives: paste the whole file, from its first \
         {{ to its last }}"
    )]
    NotTheFile,
    /// A client of another type than Web application.
    #[error(
        "that client is not of the type Web application: create a new client of that type, \
         since only a web client can send you back to this box"
    )]
    NotAWebClient,
    /// An id or secret Google would not have issued.
    #[error("the file's {0} is not one Google issues: download the file again")]
    Malformed(&'static str),
    /// The client does not list the box's callback, so Google would refuse to send the owner
    /// back to it.
    #[error(
        "the client does not list {wanted} as an authorized redirect URI (it lists {listed}): \
         add exactly that address under Authorized redirect URIs, save, and download the file \
         again"
    )]
    RedirectNotListed {
        /// The box's callback.
        wanted: String,
        /// What the client lists, which is no secret.
        listed: String,
    },
}

/// The fields of Google's client file this box reads.
#[derive(serde::Deserialize)]
struct Download {
    web: Option<Web>,
    installed: Option<serde::de::IgnoredAny>,
}

#[derive(serde::Deserialize)]
struct Web {
    client_id: String,
    client_secret: String,
    #[serde(default)]
    redirect_uris: Vec<String>,
}

impl Client {
    /// The client in the file Google's Download JSON gives, pasted whole into the card's field,
    /// kept only when it is a web client listing `redirect` exactly: Google compares the
    /// redirect byte for byte, so a client that does not list it could never finish a sign-in.
    ///
    /// # Errors
    /// `ClientRefused`, in words that never quote the secret.
    pub fn of_download(pasted: &Secret, redirect: &RedirectUri) -> Result<Self, ClientRefused> {
        let download: Download =
            serde_json::from_str(pasted.expose().trim()).map_err(|_| ClientRefused::NotTheFile)?;
        let web = match download {
            Download { web: Some(web), .. } => web,
            Download {
                installed: Some(_), ..
            } => return Err(ClientRefused::NotAWebClient),
            Download { .. } => return Err(ClientRefused::NotTheFile),
        };
        let id = ClientId::parse(&web.client_id).ok_or(ClientRefused::Malformed("client id"))?;
        let secret = ClientSecret::parse(&web.client_secret)
            .ok_or(ClientRefused::Malformed("client secret"))?;
        if !web.redirect_uris.iter().any(|uri| uri == redirect.as_str()) {
            return Err(ClientRefused::RedirectNotListed {
                wanted: redirect.to_string(),
                listed: if web.redirect_uris.is_empty() {
                    "none".to_owned()
                } else {
                    web.redirect_uris.join(", ")
                },
            });
        }
        Ok(Self { id, secret })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redirect() -> RedirectUri {
        RedirectUri::of_app("https://box.example.test").expect("a callback")
    }

    fn file(kind: &str, uris: &[&str]) -> Secret {
        let client = serde_json::json!({
            "client_id": "1234-abcd.apps.googleusercontent.com",
            "project_id": "suzatary-example",
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": "https://oauth2.googleapis.com/token",
            "client_secret": "GOCSPX-example-secret",
            "redirect_uris": uris,
        });
        let mut download = serde_json::Map::new();
        download.insert(kind.to_owned(), client);
        Secret::new(serde_json::Value::Object(download).to_string())
    }

    /// The downloaded file of a web client listing the box's callback is the client.
    #[test]
    fn a_web_client_listing_the_callback_is_kept() {
        let client = Client::of_download(
            &file(
                "web",
                &["https://box.example.test/api/oauth/google/callback"],
            ),
            &redirect(),
        )
        .expect("the client");
        assert_eq!(client.id.as_str(), "1234-abcd.apps.googleusercontent.com");
        assert_eq!(client.secret.expose(), "GOCSPX-example-secret");
        assert!(!format!("{client:?}").contains("GOCSPX"));
    }

    /// A redirect is compared exactly: a trailing slash, another host or none at all is refused,
    /// and the refusal names the address to add.
    #[test]
    fn a_client_without_the_exact_callback_is_refused_by_name() {
        for uris in [
            vec![],
            vec!["https://box.example.test/api/oauth/google/callback/"],
            vec!["https://other.example.test/api/oauth/google/callback"],
        ] {
            let refused =
                Client::of_download(&file("web", &uris), &redirect()).expect_err("refused");
            let said = refused.to_string();
            assert!(
                said.contains("https://box.example.test/api/oauth/google/callback "),
                "{said}"
            );
            assert!(!said.contains("GOCSPX"), "{said}");
        }
    }
}
