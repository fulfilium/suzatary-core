//! The box's dedup key: what `state.json` names a message by, so a pushed record of every
//! decision is not a record anyone holding a message can find it in.
//!
//! The canonical content hash is a public function of a message's bytes, so a file keyed by it
//! tells anyone who holds a message whether this box received it. The pushed file is keyed by
//! [`DedupKey::name`], an HMAC-SHA256 of that hash under a secret only the box holds; the hash
//! itself, and the rest of what the box knows of a message, stay in the box-local tier.

use hmac::{Hmac, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use suzatary_base::capability::Secret;

/// The secretspec name of the key, minted by `suzatary device init` beside the signing seeds.
pub const DEDUP_KEY: &str = "SUZATARY_DEDUP_KEY";

/// The key, 32 bytes. Never printed: `Debug` says only that it is one.
#[derive(Clone, PartialEq, Eq)]
pub struct DedupKey([u8; 32]);

/// A value that is not a key: by its name only, never its bytes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{DEDUP_KEY} is not a key (64 lowercase hex)")]
pub struct NotAKey;

impl DedupKey {
    /// The key whose 64 lowercase hex digits `secret` holds.
    ///
    /// # Errors
    /// [`NotAKey`].
    pub fn of(secret: &Secret) -> Result<Self, NotAKey> {
        let text = secret.expose();
        if text.len() != 64 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(NotAKey);
        }
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(text, &mut bytes).map_err(|_| NotAKey)?;
        Ok(Self(bytes))
    }

    /// What the pushed file names the message whose canonical hash is `hash` by.
    ///
    /// # Panics
    /// Never: HMAC takes a key of any length.
    #[must_use]
    pub fn name(&self, hash: &str) -> Keyed {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0).expect("HMAC takes any key length");
        mac.update(hash.as_bytes());
        Keyed(hex::encode(mac.finalize().into_bytes()))
    }
}

impl std::fmt::Debug for DedupKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DedupKey(..)")
    }
}

/// A message as the pushed file names it: 64 lowercase hex digits only [`DedupKey::name`]
/// produces, or a file reads.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Keyed(String);

impl Keyed {
    /// Its spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Keyed {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        if text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(text))
        } else {
            Err(format!("{text:?} is not a keyed name (64 lowercase hex)"))
        }
    }
}

impl From<Keyed> for String {
    fn from(keyed: Keyed) -> String {
        keyed.0
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn key(byte: u8) -> DedupKey {
        DedupKey::of(&Secret::new(hex::encode([byte; 32]))).expect("a key")
    }

    #[test]
    fn a_key_is_64_lowercase_hex_and_its_debug_holds_none_of_it() {
        assert_eq!(DedupKey::of(&Secret::new("ab")), Err(NotAKey));
        assert_eq!(DedupKey::of(&Secret::new("AB".repeat(32))), Err(NotAKey));
        assert_eq!(format!("{:?}", key(7)), "DedupKey(..)");
    }

    /// RFC 4231 test case 2, so the name is HMAC-SHA256 and nothing of this module's own.
    #[test]
    fn the_name_is_hmac_sha256() {
        let jefe = DedupKey({
            let mut bytes = [0u8; 32];
            bytes[..4].copy_from_slice(b"Jefe");
            bytes
        });
        // A 32-byte key padded with zeros is the 4-byte key, by HMAC's own padding.
        assert_eq!(
            jefe.name("what do ya want for nothing?").as_str(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    proptest! {
        /// One key names one hash one way; another key, or another hash, names it otherwise.
        #[test]
        fn names_are_a_function_of_the_key_and_the_hash(hash in "[0-9a-f]{64}", other in "[0-9a-f]{64}") {
            prop_assert_eq!(key(1).name(&hash), key(1).name(&hash));
            prop_assert_ne!(key(1).name(&hash), key(2).name(&hash));
            prop_assert_ne!(String::from(key(1).name(&hash)), hash.clone());
            if hash != other {
                prop_assert_ne!(key(1).name(&hash), key(1).name(&other));
            }
        }
    }
}
