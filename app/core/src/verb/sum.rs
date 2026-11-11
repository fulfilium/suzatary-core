//! A surface's verbs as a closed sum, declared once.

use schemars::JsonSchema;
use serde_json::{Map, Value, json};

use super::Verb;

/// Declare a surface's verbs as two sums: `$Verb`, one empty variant per verb, and `$Call`, one
/// variant per verb holding its parsed arguments.
///
/// Every function over a surface is a `match` the compiler checks is whole, so a verb joins the
/// sum with its class, its schema, its parser and its body, or does not compile. `classed by`
/// classes each verb by its own name's variant, and `of` maps back.
macro_rules! verbs {
    (
        $(#[$verb_doc:meta])* pub enum $Verb:ident;
        $(#[$call_doc:meta])* pub enum $Call:ident;
        on $Surface:ty, classed by $Class:ident;
        $($Name:ident),+ $(,)?
    ) => {
        verbs! {
            $(#[$verb_doc])* pub enum $Verb;
            $(#[$call_doc])* pub enum $Call;
            on $Surface;
            $($Name: $Class::$Name),+
        }

        impl $Verb {
            /// The verb of `class`.
            #[must_use]
            pub const fn of(class: $Class) -> Self {
                match class {
                    $($Class::$Name => Self::$Name,)+
                }
            }
        }
    };
    (
        $(#[$verb_doc:meta])* pub enum $Verb:ident;
        $(#[$call_doc:meta])* pub enum $Call:ident;
        on $Surface:ty;
        $($Name:ident: $class:expr),+ $(,)?
    ) => {
        $(#[$verb_doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $Verb {
            $(#[doc = concat!("`", stringify!($Name), "`.")] $Name,)+
        }

        impl $Verb {
            /// Every verb, in the order the model is shown them.
            pub const ALL: &'static [Self] = &[$(Self::$Name),+];

            /// The name the model calls it by.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$Name => <$Name as $crate::verb::Verb>::NAME,)+
                }
            }

            /// What the model is told it does.
            #[must_use]
            pub const fn description(self) -> &'static str {
                match self {
                    $(Self::$Name => <$Name as $crate::verb::Verb>::DESCRIPTION,)+
                }
            }

            /// What the surface knows of it beyond its name.
            #[must_use]
            pub const fn class(self) -> <$Surface as $crate::verb::Surface>::Class {
                match self {
                    $(Self::$Name => $class,)+
                }
            }

            /// The verb a model's string names, or `None`.
            #[must_use]
            pub fn named(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|verb| verb.name() == name)
            }

            /// The verb in the MCP `tools/list` shape, its schema derived from its arguments.
            #[must_use]
            pub fn tool(self) -> serde_json::Value {
                match self {
                    $(Self::$Name => $crate::verb::tool::<$Name>(),)+
                }
            }

            /// `args` read as this verb's arguments.
            ///
            /// # Errors
            /// An argument that is missing or of the wrong type.
            pub fn parse(
                self,
                args: &serde_json::Map<String, serde_json::Value>,
            ) -> Result<$Call, $crate::refusal::Refusal> {
                match self {
                    $(Self::$Name => $crate::verb::parse(args).map($Call::$Name),)+
                }
            }

            /// `args` read whole: the chain they name, then this verb's own arguments.
            ///
            /// # Errors
            /// A `chain` that is not a string, or an argument that is missing or of the wrong type.
            pub fn request(
                self,
                args: &serde_json::Map<String, serde_json::Value>,
            ) -> Result<$crate::verb::Request<$Call>, $crate::refusal::Refusal> {
                Ok($crate::verb::Request {
                    chain: $crate::verb::chain_named(args)?,
                    call: self.parse(args)?,
                })
            }
        }

        impl std::fmt::Display for $Verb {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.name())
            }
        }

        $(#[$call_doc])*
        pub enum $Call {
            $(
                #[doc = concat!("`", stringify!($Name), "` and its arguments.")]
                $Name(<$Name as $crate::verb::Verb>::Args),
            )+
        }

        impl $Call {
            /// Which verb it calls.
            #[must_use]
            pub const fn verb(&self) -> $Verb {
                match self {
                    $(Self::$Name(_) => $Verb::$Name,)+
                }
            }

            /// Run it.
            ///
            /// # Errors
            /// Whatever the verb refuses, or an effect's failure.
            pub fn run(
                self,
                on: &<$Surface as $crate::verb::Surface>::Call<'_>,
            ) -> Result<<$Surface as $crate::verb::Surface>::Answer, $crate::refusal::Refusal> {
                match self {
                    $(Self::$Name(args) => <$Name as $crate::verb::Verb>::run(on, args).map(Into::into),)+
                }
            }
        }
    };
}

pub(crate) use verbs;

/// `V` in the MCP `tools/list` shape.
#[must_use]
pub fn tool<V: Verb>() -> Value {
    json!({
        "name": V::NAME,
        "description": V::DESCRIPTION,
        "inputSchema": input_schema::<V::Args>(),
    })
}

/// `A`'s schema as a model is shown it: each argument's type and default, and which are required.
/// An argument that may be absent and names no default defaults to null.
fn input_schema<A: JsonSchema>() -> Value {
    let schema = schemars::SchemaGenerator::default().into_root_schema_for::<A>();
    let required = schema
        .get("required")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let properties: Map<String, Value> = schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, property)| {
            let mut shown = Map::new();
            shown.insert("type".to_owned(), property["type"].clone());
            if let Some(names) = property.get("enum") {
                shown.insert("enum".to_owned(), names.clone());
            }
            let optional = !required
                .as_array()
                .is_some_and(|names| names.iter().any(|named| named == name.as_str()));
            match property.get("default") {
                Some(default) => {
                    shown.insert("default".to_owned(), default.clone());
                }
                None if optional => {
                    shown.insert("default".to_owned(), Value::Null);
                }
                None => {}
            }
            (name.clone(), Value::Object(shown))
        })
        .collect();
    json!({"type": "object", "properties": properties, "required": required})
}
