//! Words a reading may no longer have: kept, or gone and why.

use serde::{Deserialize, Serialize};

use crate::secretary::erasure::Erasure;

/// Words of type `T`, or why they are not here; kept words serialise as themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words<T> {
    /// Here.
    Kept { kept: T },
    Gone {
        /// Why.
        gone: Gone,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "lowercase")]
pub enum Gone {
    /// Their key was destroyed.
    Erased { erasure: Erasure },
    /// Their key is not here: lost in a restore, or never given to this replica.
    Missing,
    /// Locked under older redaction rules, they fail the current ones and are not shown.
    Withheld,
}

impl<T> Words<T> {
    pub fn kept(kept: T) -> Self {
        Words::Kept { kept }
    }

    pub fn get(&self) -> Option<&T> {
        match self {
            Words::Kept { kept } => Some(kept),
            Words::Gone { .. } => None,
        }
    }

    pub fn into_kept(self) -> Option<T> {
        match self {
            Words::Kept { kept } => Some(kept),
            Words::Gone { .. } => None,
        }
    }

    pub fn gone(&self) -> Option<&Gone> {
        match self {
            Words::Kept { .. } => None,
            Words::Gone { gone } => Some(gone),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Words<U> {
        match self {
            Words::Kept { kept } => Words::Kept { kept: f(kept) },
            Words::Gone { gone } => Words::Gone { gone },
        }
    }

    pub fn as_ref(&self) -> Words<&T> {
        match self {
            Words::Kept { kept } => Words::Kept { kept },
            Words::Gone { gone } => Words::Gone { gone: gone.clone() },
        }
    }
}

/// Gone words print as the box's sentence in brackets, so it never reads as somebody's words.
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
    /// The sentence a model or a page sees in the words' place, composed from the reason and never
    /// stored.
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
