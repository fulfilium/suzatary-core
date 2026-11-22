use prodrome_core::event::Hash;
use prodrome_core::sign::{PublicKey, Signature, Signed};
use serde::{Deserialize, Serialize};

/// Which device asked for what the box wrote: one performed request, as provenance only.
///
/// It is pushed with the data, so it holds no body, answer, body digest or nonce: a name
/// recomputed from a guessed body would test a secret the body carried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requested {
    /// [`super::request_name`] of what it asked and the key.
    pub name: String,
    pub method: String,
    /// The path and query.
    pub target: String,
    /// The instant its device stamped on it.
    pub at: String,
    /// The device key that signed it.
    pub key: String,
    /// The signature of the name.
    pub signature: String,
    /// The actor the gate wrote it as.
    pub actor: String,
    /// The HTTP status the box answered.
    pub status: u16,
    /// The chain objects the box appended performing it, in the order written.
    pub wrote: Vec<String>,
}

/// What a replay is answered with: the status and the body's digest, never its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub status: u16,
    /// `{"type":"NotKept","sha256":…}`.
    pub body: String,
}

impl Answer {
    #[must_use]
    pub fn of(status: u16, sent: &[u8]) -> Self {
        use sha2::Digest as _;
        Self {
            status,
            body: serde_json::json!({
                "type": "NotKept",
                "sha256": hex::encode(sha2::Sha256::digest(sent)),
            })
            .to_string(),
        }
    }
}

impl Requested {
    #[must_use]
    pub fn verifies(&self) -> bool {
        match (
            Hash::new(self.name.clone()),
            PublicKey::new(self.key.clone()),
            Signature::new(self.signature.clone()),
        ) {
            (Ok(object), Ok(key), Ok(signature)) => Signed {
                object,
                key,
                signature,
            }
            .verifies(),
            _ => false,
        }
    }
}
