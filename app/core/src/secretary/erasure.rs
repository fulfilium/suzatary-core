//! Why a line's words went: the owner asked, or a forgetting took the copies it caused.

use prodrome_core::event::Actor;
use serde::{Deserialize, Serialize};

use crate::secretary::memory::FactId;

/// Why words were erased, as the history records it, so a page and a model are told which in the
/// box's words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Spelled", into = "Spelled")]
pub enum Erasure {
    /// The owner, or another actor the gate admitted, erased them on a tap.
    Asked {
        /// Who.
        by: Actor,
    },
    /// Forgetting a fact erased the copies its remembering wrote.
    Forgot {
        /// The fact.
        fact: FactId,
    },
}

impl Erasure {
    /// What an erasure does, as every page that offers one says it before the owner confirms.
    pub const FOR_GOOD: &'static str = "The box and every device forget the words for good; who said them, when, and what they referred to stay.";

    /// The sentence a reader is told in the words' place.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Erasure::Asked { by } => format!(
                "{} erased these words",
                crate::spelling::capitalised(by.as_str())
            ),
            Erasure::Forgot { fact } => {
                format!("these words were erased when fact {fact} was forgotten")
            }
        }
    }
}

/// The serde spelling: an actor is a checked name, so it is read back through its constructor.
#[derive(Serialize, Deserialize)]
#[serde(tag = "erasure", rename_all = "lowercase")]
enum Spelled {
    Asked { by: String },
    Forgot { fact: FactId },
}

impl From<Erasure> for Spelled {
    fn from(erasure: Erasure) -> Self {
        match erasure {
            Erasure::Asked { by } => Spelled::Asked {
                by: by.into_string(),
            },
            Erasure::Forgot { fact } => Spelled::Forgot { fact },
        }
    }
}

impl TryFrom<Spelled> for Erasure {
    type Error = String;
    fn try_from(spelled: Spelled) -> Result<Self, String> {
        Ok(match spelled {
            Spelled::Asked { by } => Erasure::Asked {
                by: Actor::new(by).map_err(|failed| failed.to_string())?,
            },
            Spelled::Forgot { fact } => Erasure::Forgot { fact },
        })
    }
}
