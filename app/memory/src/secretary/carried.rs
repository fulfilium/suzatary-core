//! Where a line came from: written here, or carried from an object it replaced.
//!
//! A carried line keeps its earlier object's name, because the digest reads it as its leaf; it is
//! never read for meaning.

use prodrome_data::event::Hash;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Carried {
    Written,
    /// Carried from the object of the earlier store this names.
    From(Hash),
}

/// A line prints `None`, or the name of the object it carries.
impl prodrome_data::literal::codec::Literal for Carried {
    fn to_value(&self) -> prodrome_data::literal::Value {
        match self {
            Carried::Written => None::<Hash>.to_value(),
            Carried::From(object) => Some(object.clone()).to_value(),
        }
    }

    fn from_value(
        value: &prodrome_data::literal::Value,
    ) -> Result<Self, prodrome_data::literal::ProdromeError> {
        Ok(Option::<Hash>::from_value(value)?.map_or(Carried::Written, Carried::From))
    }
}
