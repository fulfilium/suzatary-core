//! The arguments of a verb that reads or writes one chain, which the surface picks.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

use crate::refusal::Refusal;

/// A verb's own arguments `A`, plus `chain`, which the surface resolves before the verb runs.
#[derive(Debug)]
pub struct Chained<A>(pub A);

impl<'de, A: Deserialize<'de>> Deserialize<'de> for Chained<A> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        A::deserialize(deserializer).map(Self)
    }
}

impl<A: JsonSchema> JsonSchema for Chained<A> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        A::schema_name()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = A::json_schema(generator);
        let chain = json_schema!({"type": ["string", "null"]});
        if let Some(properties) = schema
            .ensure_object()
            .entry("properties")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
            .as_object_mut()
        {
            properties.insert("chain".to_owned(), chain.into());
        }
        schema
    }
}

/// A verb's own arguments `A`, as a surface spells them: bare, or with the `chain` it resolves.
pub trait Own<A> {
    /// The verb's own arguments.
    fn own(self) -> A;
}

impl<A> Own<A> for A {
    fn own(self) -> A {
        self
    }
}

impl<A> Own<A> for Chained<A> {
    fn own(self) -> A {
        self.0
    }
}

/// The chain a call names, whichever verb it calls; absent means the surface's first.
///
/// # Errors
/// A `chain` that is not a string.
pub(crate) fn chain_named(args: &Map<String, Value>) -> Result<Option<String>, Refusal> {
    #[derive(Deserialize, JsonSchema)]
    struct On {
        chain: Option<String>,
    }
    super::parse::<On>(args).map(|on| on.chain)
}

/// A call read whole where it arrives: the chain it names, absent meaning the surface's first,
/// and the verb with its arguments.
#[derive(Debug)]
pub struct Request<C> {
    /// The chain named, unresolved, since only the surface knows which it serves.
    pub chain: Option<String>,
    /// The verb and its arguments.
    pub call: C,
}
