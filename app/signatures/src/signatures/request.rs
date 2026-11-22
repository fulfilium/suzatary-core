use prodrome_core::event::Hash;
use sha2::{Digest, Sha256};

/// A device's request of the box, named by its content under a prefix of its own, so a request's
/// name is never an object's.
/// The same request copied is the same name; two asks made on purpose differ by nonce and instant.
///
/// The box never records the nonce, so a guessed body cannot recompute the name from a record.
///
/// # Panics
/// Never: a SHA-256 in hex is a name.
#[must_use]
pub fn request_name(
    method: &str,
    target: &str,
    at: &str,
    nonce: &str,
    body: &str,
    key: &str,
) -> Hash {
    let mut digest = Sha256::new();
    digest.update(b"suzatary request\n");
    digest.update(method.as_bytes());
    digest.update(b" ");
    digest.update(target.as_bytes());
    digest.update(b"\n");
    digest.update(at.as_bytes());
    digest.update(b"\n");
    digest.update(nonce.as_bytes());
    digest.update(b"\n");
    digest.update(body.as_bytes());
    digest.update(b"\n");
    digest.update(key.as_bytes());
    Hash::new(hex::encode(digest.finalize())).expect("a SHA-256 in hex is a name")
}

/// A request's nonce, as a device draws it: 128 random bits in lowercase hex.
#[must_use]
pub fn is_nonce(nonce: &str) -> bool {
    nonce.len() == 32
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// How far a request's instant may be from the box's clock, in seconds; an answer is kept twice
/// this.
pub const WINDOW_SECONDS: i64 = 600;

/// A body's digest as a request's name takes it: lowercase hex SHA-256.
#[must_use]
pub fn body_digest(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    /// The name the tab computes for the same request (`web/test/request.test.ts` pins its side).
    #[test]
    fn a_request_is_named_by_its_method_target_instant_nonce_body_and_asker() {
        let at = "2026-10-06T09:00:00.000Z";
        let body = body_digest(br#"{"objects":[]}"#);
        let named = |method, target, at, body: &str, key| {
            request_name(method, target, at, NONCE, body, key)
        };
        let name = named("POST", "/api/proposals/accept", at, &body, KEY);
        assert_eq!(name, named("POST", "/api/proposals/accept", at, &body, KEY));
        assert_ne!(name, named("POST", "/api/proposals/reject", at, &body, KEY));
        assert_ne!(
            name,
            named(
                "POST",
                "/api/proposals/accept",
                "2026-10-06T09:00:01.000Z",
                &body,
                KEY
            ),
            "two asks made on purpose differ by their instants"
        );
        assert_ne!(
            name,
            named(
                "POST",
                "/api/proposals/accept",
                at,
                &body_digest(br#"{"objects":["a"]}"#),
                KEY
            )
        );
        assert_ne!(
            name,
            named("POST", "/api/proposals/accept", at, &body, &"2".repeat(64)),
            "two devices asking the same at once ask twice"
        );
        assert_ne!(
            name,
            request_name(
                "POST",
                "/api/proposals/accept",
                at,
                "fedcba9876543210fedcba9876543210",
                &body,
                KEY
            ),
            "two asks drawn apart differ by their nonces"
        );
        assert_eq!(
            name.as_str(),
            "e6998a65333b75210595be2aed16bbcd715904c094b8a05199c1e515ad096a88"
        );
        assert!(is_nonce(NONCE));
        for not in [
            "",
            "0123",
            &NONCE.to_uppercase(),
            &format!("{NONCE}0"),
            &"g".repeat(32),
        ] {
            assert!(!is_nonce(not), "{not:?}");
        }
    }
}
