//! Words a reading may no longer have: kept, or gone and why.
//!
//! A locked text reads as its words while its key is kept. Once the key is destroyed it reads as
//! [`Gone::Erased`], saying who or what erased it; where the key was never here (a box restored
//! from a vault older than the line, a tab not given it) it reads as [`Gone::Missing`]. Neither is
//! a sentinel string: a reader that shows words matches the sum, and a reader of the shape alone
//! never looks.

use serde::{Deserialize, Serialize};

use crate::secretary::erasure::Erasure;

/// Words of type `T`, or why they are not here. Kept words serialise as themselves, so a reader
/// of the JSON a line printed before its words could go reads it the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words<T> {
    /// Here.
    Kept {
        /// The words.
        kept: T,
    },
    /// Not here.
    Gone {
        /// Why.
        gone: Gone,
    },
}

/// Why words are not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "lowercase")]
pub enum Gone {
    /// Their key was destroyed, for the reason the history records.
    Erased {
        /// The erasure.
        erasure: Erasure,
    },
    /// Their key is not here: lost in a restore, or never given to this replica.
    Missing,
    /// They were locked under older redaction rules and no longer pass the door under the
    /// current ones, so they are not shown; the migration reports them.
    Withheld,
}

impl<T> Words<T> {
    /// Kept words.
    pub fn kept(kept: T) -> Self {
        Words::Kept { kept }
    }

    /// The words, when kept.
    pub fn get(&self) -> Option<&T> {
        match self {
            Words::Kept { kept } => Some(kept),
            Words::Gone { .. } => None,
        }
    }

    /// The words, owned, when kept.
    pub fn into_kept(self) -> Option<T> {
        match self {
            Words::Kept { kept } => Some(kept),
            Words::Gone { .. } => None,
        }
    }

    /// Why they are gone, when they are.
    pub fn gone(&self) -> Option<&Gone> {
        match self {
            Words::Kept { .. } => None,
            Words::Gone { gone } => Some(gone),
        }
    }

    /// The functor: `f` on kept words, a gone reason unchanged.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Words<U> {
        match self {
            Words::Kept { kept } => Words::Kept { kept: f(kept) },
            Words::Gone { gone } => Words::Gone { gone },
        }
    }

    /// The words borrowed.
    pub fn as_ref(&self) -> Words<&T> {
        match self {
            Words::Kept { kept } => Words::Kept { kept },
            Words::Gone { gone } => Words::Gone { gone: gone.clone() },
        }
    }
}

/// Kept words as themselves; gone ones as the box's sentence, bracketed so it never reads as
/// somebody's words.
impl<T: std::fmt::Display> std::fmt::Display for Words<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Words::Kept { kept } => kept.fmt(f),
            Words::Gone { gone } => write!(f, "[{}]", gone.sentence()),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Spelled<T> {
    Gone { gone: Gone },
    Kept(T),
}

impl<T: Serialize> Serialize for Words<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Words::Kept { kept } => kept.serialize(serializer),
            Words::Gone { gone } => {
                Spelled::<()>::Gone { gone: gone.clone() }.serialize(serializer)
            }
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Words<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Spelled::<T>::deserialize(deserializer)? {
            Spelled::Gone { gone } => Words::Gone { gone },
            Spelled::Kept(kept) => Words::Kept { kept },
        })
    }
}

impl<T> From<Gone> for Words<T> {
    fn from(gone: Gone) -> Self {
        Words::Gone { gone }
    }
}

impl Gone {
    /// The box's sentence for words that are not here, what a model or a page is told in their
    /// place: composed from the reason, never stored.
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Gone::Erased { erasure } => erasure.sentence(),
            Gone::Missing => "these words are not on this box: their key was lost".to_owned(),
            Gone::Withheld => {
                "these words are withheld: they do not pass the box's current redaction rules"
                    .to_owned()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn words() -> impl Strategy<Value = Words<u8>> {
        prop_oneof![
            any::<u8>().prop_map(Words::kept),
            Just(Words::from(Gone::Missing)),
        ]
    }

    proptest! {
        /// Kept words print as themselves, and every value reads back as itself.
        #[test]
        fn words_read_back_and_kept_ones_print_as_themselves(w in words()) {
            let printed = serde_json::to_string(&w).expect("prints");
            prop_assert_eq!(serde_json::from_str::<Words<u8>>(&printed).expect("reads"), w.clone());
            if let Some(kept) = w.get() {
                prop_assert_eq!(printed, kept.to_string());
            }
        }
    }
}
