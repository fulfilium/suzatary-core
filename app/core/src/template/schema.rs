//! A Rust type's Typst schema, read off the JSON Schema serde's attributes give it: one module of
//! accessors per named type, and the values it is checked against.
//!
//! A template reads its value only through these accessors (`#import "/schema/Answer.typ" as
//! answer`, then `answer.source(d)`), so a field the type does not have is an unknown name and
//! fails the compile whatever value the template is called with. Nothing here names a Suzatary
//! type: it holds for any `JsonSchema`.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde_json::Value;

/// Where a type's module is kept in a template's world: `/schema/<its schema name>.typ`.
#[must_use]
pub fn path(name: &str) -> String {
    format!("/schema/{name}.typ")
}

/// The Typst schema of `T`: its modules, by path, and its examples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schema {
    /// Each named type's module of accessors, by its [`path`].
    pub(crate) modules: BTreeMap<String, String>,
    /// Values of `T` as serde prints them ([`super::example`]), which together take every arm
    /// of every sum and leave every option empty once. Typst asks for a name only on the branch
    /// it takes, so a template is checked against them all.
    pub(crate) examples: Vec<Value>,
}

/// `T`'s schema.
#[must_use]
pub fn of<T: JsonSchema>() -> Schema {
    let root = schemars::SchemaGenerator::default().into_root_schema_for::<T>();
    let mut defs = root
        .as_value()
        .get("$defs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut own = root.as_value().clone();
    if let Value::Object(fields) = &mut own {
        fields.remove("$defs");
    }
    defs.insert(T::schema_name().into_owned(), own);
    Schema {
        modules: defs
            .iter()
            .map(|(name, schema)| (path(name), module(schema)))
            .collect(),
        examples: super::example::all(&T::schema_name(), &defs),
    }
}

/// One accessor: a field read by its name, or a sum's arm asked about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Accessor {
    Field(String),
    /// An internally tagged arm: the tag field holds the arm's name.
    Tagged {
        field: String,
        arm: String,
    },
    /// A unit arm, printed as its name alone.
    Unit(String),
}

impl Accessor {
    fn print(&self) -> String {
        match self {
            Accessor::Field(name) => format!("#let {name}(d) = d.at(\"{name}\")\n"),
            Accessor::Tagged { field, arm } => {
                format!("#let is-{arm}(d) = d.at(\"{field}\") == \"{arm}\"\n")
            }
            Accessor::Unit(arm) => format!("#let is-{arm}(d) = d == \"{arm}\"\n"),
        }
    }
}

/// A named type's accessors, one line each, sorted, so the module is a function of the type.
fn module(schema: &Value) -> String {
    let mut accessors = BTreeSet::new();
    gather(schema, &mut accessors);
    accessors.iter().map(Accessor::print).collect()
}

/// The accessors a schema's own shape holds: its fields, and its arms'. A shape this reads no
/// accessor from gives none, so a template that reads it fails its compile.
fn gather(schema: &Value, out: &mut BTreeSet<Accessor>) {
    if let Some(Value::String(arm)) = schema.get("const") {
        out.insert(Accessor::Unit(arm.clone()));
    }
    if let Some(Value::Array(arms)) = schema.get("enum") {
        out.extend(
            arms.iter()
                .filter_map(Value::as_str)
                .map(|arm| Accessor::Unit(arm.to_owned())),
        );
    }
    if let Some(Value::Object(fields)) = schema.get("properties") {
        for (name, field) in fields {
            match field.get("const") {
                Some(Value::String(arm)) => out.insert(Accessor::Tagged {
                    field: name.clone(),
                    arm: arm.clone(),
                }),
                _ => out.insert(Accessor::Field(name.clone())),
            };
        }
    }
    for arms in ["oneOf", "anyOf", "allOf"] {
        if let Some(Value::Array(arms)) = schema.get(arms) {
            arms.iter()
                .filter(|arm| arm.get("$ref").is_none())
                .for_each(|arm| gather(arm, out));
        }
    }
}
