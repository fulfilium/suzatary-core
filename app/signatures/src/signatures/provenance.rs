use prodrome_core::sign::PublicKey;

use super::Unproven;

/// Who wrote one object, as a reader is told: what a page beside the object says, and what the
/// box answers a refused post with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// A signature by a key registered to its actor proves it.
    Proven {
        /// The key whose signature proves it.
        key: PublicKey,
    },
    /// Unsigned and beneath an epoch: written before this deployment signed its objects, held
    /// by the box when it began, and as valid as it ever was.
    Predates,
    /// Not proven, and not earlier than signatures.
    Unproven(Unproven),
}

impl Provenance {
    /// The sentence a page shows.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Provenance::Proven { key } => {
                format!("signed by the key {}", super::unproven::short(key))
            }
            Provenance::Predates => {
                "written before signatures (SPEC law 42); the box attested it then".to_owned()
            }
            Provenance::Unproven(why) => format!("not proven: {}", why.said()),
        }
    }
}
