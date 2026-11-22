use serde::Serialize;

/// One key's standing for one actor in one prodrome, as the reader's registrar counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStanding {
    /// No `KeyAdded` that counts registers it: what it signs proves nothing.
    Untrusted,
    /// Registered, and no revocation that counts names it.
    Trusted,
    /// Proves only the signatures its revocations rest on; adding it again is the same object.
    Revoked,
}

impl KeyStanding {
    /// Whether a new signature by this key is believed: a new signature is beneath no revocation.
    #[must_use]
    pub fn signs(self) -> bool {
        matches!(self, KeyStanding::Trusted)
    }

    /// The spelling it serialises as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStanding::Untrusted => "untrusted",
            KeyStanding::Trusted => "trusted",
            KeyStanding::Revoked => "revoked",
        }
    }
}
