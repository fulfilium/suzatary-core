//! The two values that bind one consent to the request that began it: the state Google hands
//! back unchanged (RFC 6749 §10.12) and the PKCE verifier only the box holds (RFC 7636).

use base64::Engine as _;
use sha2::Digest as _;

use suzatary_base::capability::{Entropy, Secret};

/// How many random bytes each value is drawn from: 256 bits, as RFC 7636 §7.1 recommends.
const DRAWN: usize = 32;

/// The length of a value drawn from [`DRAWN`] bytes, base64url without padding.
const SPELT: usize = 43;

/// `DRAWN` bytes nobody can guess, spelt base64url without padding.
fn drawn(entropy: &dyn Entropy) -> Result<String, String> {
    let mut bytes = [0u8; DRAWN];
    entropy.fill(&mut bytes)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Whether `text` is a value as [`drawn`] spells one.
fn spelt(text: &str) -> bool {
    text.len() == SPELT
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// The PKCE code verifier: sent once, to the token endpoint with the code it binds, and never
/// in the consent's URL, a log or a store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verifier(Secret);

impl Verifier {
    /// A fresh one.
    ///
    /// # Errors
    /// The generator's failure.
    pub fn draw(entropy: &dyn Entropy) -> Result<Self, String> {
        drawn(entropy).map(|text| Self(Secret::new(text)))
    }

    /// Its S256 challenge, the only form of it the consent's URL carries.
    #[must_use]
    pub fn challenge(&self) -> Challenge {
        Challenge::s256(self.0.expose())
    }

    /// The verifier itself, for the token request alone.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

/// `BASE64URL(SHA256(verifier))`: method S256, the only one this box sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge(String);

impl Challenge {
    /// `BASE64URL(SHA256(verifier))`, as the token endpoint checks a verifier against the
    /// challenge it was shown (RFC 7636 §4.6).
    #[must_use]
    pub fn s256(verifier: &str) -> Self {
        Self(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(sha2::Sha256::digest(verifier.as_bytes())),
        )
    }

    /// As the consent's URL carries it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The value that ties a callback to the consent the box began: drawn per consent, compared
/// whole, and dropped by the box as it reads a callback that names it, so a callback is read
/// once.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct State(String);

impl State {
    /// A fresh one.
    ///
    /// # Errors
    /// The generator's failure.
    pub fn draw(entropy: &dyn Entropy) -> Result<Self, String> {
        drawn(entropy).map(Self)
    }

    /// One a callback names, when it is spelt as the box draws them; anything else is no state
    /// of the box's.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        spelt(text).then(|| Self(text.to_owned()))
    }

    /// As the consent's URL carries it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// Never printed: a state still open is half of what a forged callback would need.
impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("State(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counting(std::sync::atomic::AtomicU8);

    impl Entropy for Counting {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            for byte in bytes {
                *byte = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(())
        }
    }

    /// RFC 7636 appendix B's example: this verifier's S256 challenge.
    #[test]
    fn the_challenge_is_s256_of_the_verifier() {
        let verifier = Verifier(Secret::new("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"));
        assert_eq!(
            verifier.challenge().as_str(),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    /// Each draw is 256 bits, spelt in RFC 7636's alphabet at 43 characters, and two draws
    /// differ.
    #[test]
    fn a_draw_is_43_characters_of_the_unreserved_alphabet() {
        let entropy = Counting(std::sync::atomic::AtomicU8::new(0));
        let first = State::draw(&entropy).expect("a state");
        let second = State::draw(&entropy).expect("a state");
        let verifier = Verifier::draw(&entropy).expect("a verifier");
        assert_ne!(first, second);
        for text in [first.as_str(), second.as_str(), verifier.expose()] {
            assert_eq!(text.len(), 43, "{text}");
            assert!(spelt(text), "{text}");
        }
        assert_eq!(State::parse(first.as_str()), Some(first.clone()));
        assert_eq!(format!("{first:?}"), "State(<redacted>)");
        assert!(!format!("{verifier:?}").contains(verifier.expose()));
    }

    /// A state no draw could spell is no state of the box's.
    #[test]
    fn a_state_of_another_shape_is_refused() {
        for text in [
            "",
            "short",
            &"a".repeat(44),
            "dBjftJeZ4CVP-mJ92K27uhbUJU1p1r_wW1gFWFOEj/k",
        ] {
            assert_eq!(State::parse(text), None, "{text}");
        }
    }
}
