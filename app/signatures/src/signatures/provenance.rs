use prodrome_data::sign::PublicKey;

use super::Unproven;

/// Who wrote one object, as a page or a refused post's answer tells a reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// A signature by a key registered to its actor proves it.
    Proven {
        key: PublicKey,
    },
    /// Held as its husk (SPEC §3): the signature vouches for it and proves nothing, since its
    /// bytes no longer hash to its name.
    Vouched {
        key: PublicKey,
    },
    /// Unsigned and beneath an epoch: written before this deployment signed its objects.
    Predates,
    Unproven(Unproven),
}

impl Provenance {
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Provenance::Proven { key } => {
                format!("signed by the key {}", super::unproven::short(key))
            }
            Provenance::Vouched { key } => format!(
                "kept as its husk, its words let go; the key {} signed the whole it was",
                super::unproven::short(key)
            ),
            Provenance::Predates => {
                "written before signatures (SPEC law 42); the box attested it then".to_owned()
            }
            Provenance::Unproven(why) => format!("not proven: {}", why.said()),
        }
    }
}
