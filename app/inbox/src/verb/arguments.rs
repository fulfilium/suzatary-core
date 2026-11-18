//! A call's arguments read into a verb's type, refused in the words the models have learned.

use std::collections::BTreeSet;
use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, DeserializeOwned, DeserializeSeed, IntoDeserializer, MapAccess, Visitor};
use serde_json::{Map, Value};

use suzatary_base::refusal::Refusal;

/// A call's arguments, as the model sent them.
struct Arguments<'a> {
    map: &'a Map<String, Value>,
    /// The arguments the schema lets be absent.
    optional: &'a BTreeSet<String>,
}

/// Read `map` as `A`. A null is absent where the schema lets the argument be absent or `A` holds
/// it in an `Option`, and given where neither; an argument `A` does not name is ignored.
///
/// # Errors
/// [`Refusal::bad_request`] naming the first argument that is missing or of the wrong type.
pub fn parse<A: DeserializeOwned + JsonSchema>(map: &Map<String, Value>) -> Result<A, Refusal> {
    let optional = optional::<A>();
    A::deserialize(Arguments {
        map,
        optional: &optional,
    })
    .map_err(|Fault(refusal)| refusal)
}

fn optional<A: JsonSchema>() -> BTreeSet<String> {
    let schema = schemars::SchemaGenerator::default().into_root_schema_for::<A>();
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::keys)
        .filter(|name| !required.contains(name.as_str()))
        .cloned()
        .collect()
}

/// Why an argument was refused.
#[derive(Debug)]
struct Fault(Refusal);

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.message())
    }
}

impl std::error::Error for Fault {}

impl de::Error for Fault {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Fault(Refusal::bad_request(message.to_string()))
    }

    fn missing_field(name: &'static str) -> Self {
        Fault(Refusal::bad_request(format!("{name} is required")))
    }
}

impl<'de> de::Deserializer<'de> for Arguments<'_> {
    type Error = Fault;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        let optional = self.optional;
        visitor.visit_map(Entries {
            given: self
                .map
                .iter()
                .filter(move |(name, value)| !(value.is_null() && optional.contains(*name))),
            value: None,
            optional: self.optional,
        })
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier
        ignored_any
    }
}

struct Entries<'a, I> {
    given: I,
    value: Option<(&'a str, &'a Value)>,
    optional: &'a BTreeSet<String>,
}

impl<'de, 'a, I: Iterator<Item = (&'a String, &'a Value)>> MapAccess<'de> for Entries<'a, I> {
    type Error = Fault;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Fault> {
        let Some((name, value)) = self.given.next() else {
            return Ok(None);
        };
        self.value = Some((name, value));
        seed.deserialize(name.as_str().into_deserializer())
            .map(Some)
    }

    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, Fault> {
        let (name, value) = self
            .value
            .take()
            .ok_or_else(|| de::Error::custom("a value asked for before its name"))?;
        seed.deserialize(Argument {
            name,
            value,
            nullable: self.optional.contains(name),
        })
    }
}

/// One argument, which names itself in every refusal, and says "or null" where it may be absent.
struct Argument<'a> {
    name: &'a str,
    value: &'a Value,
    nullable: bool,
}

impl Argument<'_> {
    fn must_be(&self, wanted: &str) -> Fault {
        Fault(Refusal::bad_request(format!(
            "{} must be {wanted}",
            self.name
        )))
    }

    fn missing(&self) -> Fault {
        Fault(Refusal::bad_request(format!("{} is required", self.name)))
    }

    fn string(&self) -> Fault {
        self.must_be(if self.nullable {
            "a string or null"
        } else {
            "a string"
        })
    }
}

impl<'de> de::Deserializer<'de> for Argument<'_> {
    type Error = Fault;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        self.value
            .clone()
            .deserialize_any(visitor)
            .map_err(|failed| Fault(Refusal::bad_request(format!("{}: {failed}", self.name))))
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        match self.value {
            Value::String(text) => visitor.visit_str(text),
            Value::Null => Err(self.missing()),
            _ => Err(self.string()),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        self.deserialize_str(visitor)
    }

    // Every integer argument has always been refused in these two sentences, required or not.
    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        match self.value {
            Value::Number(number) => match number.as_u64() {
                Some(whole) => visitor.visit_u64(whole),
                None => Err(self.must_be("a whole number that is not negative")),
            },
            Value::Null => Err(self.missing()),
            _ => Err(self.must_be("a whole number or null")),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        match self.value {
            Value::Bool(flag) => visitor.visit_bool(*flag),
            Value::Null => Err(self.missing()),
            _ => Err(self.must_be("true or false")),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Fault> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(Argument {
                nullable: true,
                ..self
            }),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Fault> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Fault> {
        match self.value {
            Value::Null => Err(self.missing()),
            value => value
                .clone()
                .deserialize_enum(name, variants, visitor)
                .map_err(|failed| Fault(Refusal::bad_request(format!("{}: {failed}", self.name)))),
        }
    }

    serde::forward_to_deserialize_any! {
        i8 i16 i32 i64 i128 u8 u16 u32 u128 f32 f64 char bytes byte_buf unit unit_struct seq
        tuple tuple_struct map struct identifier ignored_any
    }
}
