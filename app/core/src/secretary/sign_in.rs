//! A model sign-in in progress: the link the box read off the deployment's sign-in and the
//! sessions the box holds open, which a code card's tap finishes.

use crate::capability::Secret;
use crate::secretary::inbox::ProposalId;

/// An `https://` link on a bare host, as a sign-in draws it. Which hosts a sign-in may name is
/// the deployment's to check where it reads the link, before it makes one.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct SignInUrl(String);

impl SignInUrl {
    /// Parse one.
    ///
    /// # Errors
    /// [`NotASignIn`] for another scheme, no host, userinfo, a port, or whitespace.
    pub fn parse(text: &str) -> Result<Self, NotASignIn> {
        let refused = || NotASignIn(text.chars().take(80).collect());
        let rest = text.strip_prefix("https://").ok_or_else(refused)?;
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        // A host is a bare name, so userinfo (`ada@`) and a port (`:8443`) are refused with it.
        let bare = !host.is_empty()
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if text.chars().any(char::is_whitespace) || !bare {
            return Err(refused());
        }
        Ok(Self(text.to_owned()))
    }

    /// The link.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The host it names, whole.
    #[must_use]
    pub fn host(&self) -> &str {
        self.0["https://".len()..]
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
    }
}

/// A link that is not a sign-in, named by its first characters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not an https link on a bare host")]
pub struct NotASignIn(pub String);

/// The sign-ins the box holds open, each waiting for its code; held in no
/// capability record, so only a tap reaches one.
pub trait Sessions: Sync {
    /// Every sign-in still open, by its link.
    fn open(&self) -> Vec<SignInUrl>;

    /// Type `code` into the sign-in at `url`, and answer the card that began it with the token
    /// the code is exchanged for, or why none came in words that never carry the code or a
    /// token; the session ends either way. `None` for a sign-in no longer open.
    fn finish(
        &self,
        url: &SignInUrl,
        code: &Secret,
    ) -> Option<(ProposalId, Result<Secret, String>)>;
}

crate::literal::text_literal!(
    SignInUrl => |url| url.as_str().to_owned(),
);

impl TryFrom<String> for SignInUrl {
    type Error = NotASignIn;
    fn try_from(text: String) -> Result<Self, NotASignIn> {
        Self::parse(&text)
    }
}

impl From<SignInUrl> for String {
    fn from(url: SignInUrl) -> String {
        url.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_https_link_on_a_bare_host_parses() {
        let url =
            SignInUrl::parse("https://sign-in.example.test/oauth/authorize?code=true&state=s")
                .expect("a link");
        assert_eq!(url.host(), "sign-in.example.test");
        for refused in [
            "http://sign-in.example.test/oauth/authorize",
            "https:///oauth",
            "https://sign-in.example.test:8443/oauth",
            "https://ada@sign-in.example.test/oauth",
            "https://sign-in.example.test/oauth?x=1 y",
        ] {
            assert!(SignInUrl::parse(refused).is_err(), "{refused}");
        }
    }
}
