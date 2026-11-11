//! An argument whose fault is answered where the verb reads it, not where the call is parsed.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use suzatary_base::refusal::Refusal;

/// An argument parsed as `T`, or the refusal it earned, held until the verb reads it.
///
/// For the parts of a price and a source, which a verb reads only when the rest of the call
/// makes them matter and whose faults it answers rather than refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caught<T>(Result<T, Refusal>);

impl<T> Caught<T> {
    /// The argument, or its fault.
    ///
    /// # Errors
    /// The refusal parsing it earned.
    pub fn read(&self) -> Result<&T, Refusal> {
        self.0.as_ref().map_err(Clone::clone)
    }

    /// The argument made into another, its fault kept.
    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Caught<U> {
        Caught(self.0.map(f))
    }
}

impl<T> From<T> for Caught<T> {
    fn from(value: T) -> Self {
        Self(Ok(value))
    }
}

impl<T: Default> Default for Caught<T> {
    fn default() -> Self {
        Self(Ok(T::default()))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Caught<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(
            T::deserialize(deserializer).map_err(|fault| Refusal::bad_request(fault.to_string())),
        ))
    }
}

// Only so a schema can print an argument's default.
impl<T: Serialize> Serialize for Caught<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Ok(value) => value.serialize(serializer),
            Err(_) => serializer.serialize_none(),
        }
    }
}

impl<T: JsonSchema> JsonSchema for Caught<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}
