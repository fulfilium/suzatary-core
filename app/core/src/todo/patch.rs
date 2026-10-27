//! A change to one field.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Leave the field as it is, or set it. Its serde form is an `Option`: `null` or absent keeps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Patch<A> {
    /// Untouched, which is not empty.
    #[default]
    Keep,
    /// The new value.
    Set(A),
}

impl<A> Patch<A> {
    /// Whether this leaves the field alone.
    #[must_use]
    pub fn is_keep(&self) -> bool {
        matches!(self, Patch::Keep)
    }

    /// The new value, if any.
    #[must_use]
    pub fn set(&self) -> Option<&A> {
        match self {
            Patch::Keep => None,
            Patch::Set(value) => Some(value),
        }
    }
}

impl<A> From<Option<A>> for Patch<A> {
    fn from(value: Option<A>) -> Self {
        value.map_or(Patch::Keep, Patch::Set)
    }
}

impl<A: Serialize> Serialize for Patch<A> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.set().serialize(serializer)
    }
}

impl<'de, A: Deserialize<'de>> Deserialize<'de> for Patch<A> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<A>::deserialize(deserializer).map(Patch::from)
    }
}

/// A kept field is `None`, a set one its value.
impl<A: prodrome_core::literal::codec::Literal> prodrome_core::literal::codec::Literal
    for Patch<A>
{
    fn to_value(&self) -> prodrome_core::literal::Value {
        self.set().map_or(
            prodrome_core::literal::Value::None,
            prodrome_core::literal::codec::Literal::to_value,
        )
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        Option::<A>::from_value(value).map(Patch::from)
    }
}
