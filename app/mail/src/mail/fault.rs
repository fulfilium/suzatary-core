//! What went wrong with one account's mail, as the box classifies it where it saw it.

use serde::{Deserialize, Serialize};

/// The class of one account's failed pull or send; its stored form is the same word as the
/// health's class of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fault {
    /// The account's sign-in is missing or lapsed.
    NeedsLogin,
    /// A secret it reads is stored nowhere on this box.
    Unresolved,
    /// Its server could not be reached.
    Unreachable,
    /// Anything else.
    Unclassified,
}
