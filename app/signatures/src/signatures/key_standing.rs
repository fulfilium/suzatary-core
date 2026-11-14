use serde::Serialize;

/// What the history says of one key for one actor in one prodrome, its key objects counted as
/// the reader's registrar counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStanding {
    /// No `KeyAdded` that counts registers it: what it signs proves nothing.
    Untrusted,
    /// Registered, and no revocation that counts names it.
    Trusted,
    /// Registered and revoked: it proves only the signatures its revocations rest on, and a
    /// key revoked stays revoked (adding it again is the same object).
    Revoked,
}

impl KeyStanding {
    /// May this key sign a new object for its actor and be believed? Only a trusted one: a
    /// new signature is beneath no revocation.
    #[must_use]
    pub fn signs(self) -> bool {
        matches!(self, KeyStanding::Trusted)
    }

    /// Its one spelling, the one it serialises as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStanding::Untrusted => "untrusted",
            KeyStanding::Trusted => "trusted",
            KeyStanding::Revoked => "revoked",
        }
    }
}
