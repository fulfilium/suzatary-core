use prodrome_core::sign::PublicKey;

/// Why the history does not prove an object's actor wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unproven {
    Unsigned,
    /// Signed, and no signature verifies over its name under its key.
    Forged,
    /// Signed by a key that verifies and that no registration the root signed gives its actor.
    Untrusted {
        key: PublicKey,
    },
    /// Signed by its actor's key after, beside or unseen by that key's revocation.
    Revoked {
        key: PublicKey,
    },
}

impl Unproven {
    /// The box's own sentence, shown verbatim.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Unproven::Unsigned => "it carries no signature".to_owned(),
            Unproven::Forged => "its signature does not verify".to_owned(),
            Unproven::Untrusted { key } => format!(
                "it is signed by the device key {}, which is not trusted: tap its card in the \
                 Inbox on a trusted device, or type the code `suzatary device pair` prints at \
                 the box in Settings, This device",
                short(key)
            ),
            Unproven::Revoked { key } => {
                format!(
                    "it is signed by the device key {}, which was revoked",
                    short(key)
                )
            }
        }
    }
}

pub(super) fn short(key: &PublicKey) -> String {
    key.as_str().chars().take(12).collect::<String>() + "…"
}
