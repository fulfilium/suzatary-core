//! The values a template is checked against, read off its type's JSON Schema: every field
//! present, and between them every arm of every sum taken and every option left empty.
//!
//! Typst asks for a name only on the branch it takes, so one example per arm is what makes a
//! template's every branch meet the schema. An example aimed at one arm of one sum takes it
//! wherever that sum is drawn, and at every other sum the first arm that leads to it.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// Which example is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Aim<'a> {
    /// The first arm of every sum, and every option's value.
    First,
    /// Arm `arm` of the sum named `def`, every other sum on the way to it.
    Arm { def: &'a str, arm: usize },
    /// Every option empty, the first arm of every sum.
    Empty,
}

/// Every example of the type named `root`, whose schema is among `defs`.
#[must_use]
pub fn all(root: &str, defs: &Map<String, Value>) -> Vec<Value> {
    let aims = defs.iter().flat_map(|(def, schema)| {
        (0..arms(schema).len()).map(move |arm| Aim::Arm {
            def: def.as_str(),
            arm,
        })
    });
    let reference = Value::from(format!("#/$defs/{root}"));
    let start = Map::from_iter([("$ref".to_owned(), reference)]);
    let mut seen = BTreeSet::new();
    [Aim::First, Aim::Empty]
        .into_iter()
        .chain(aims)
        .map(|aim| {
            Drawing {
                defs,
                aim,
                open: Vec::new(),
            }
            .draw(&Value::Object(start.clone()), None)
        })
        .filter(|example| seen.insert(example.to_string()))
        .collect()
}

/// A sum's arms but null: a tagged enum's variants, or a unit enum's names.
fn arms(schema: &Value) -> Vec<&Value> {
    ["oneOf", "anyOf", "enum"]
        .iter()
        .filter_map(|key| schema.get(*key).and_then(Value::as_array))
        .flatten()
        .filter(|arm| !nullish(arm))
        .collect()
}

fn nullish(schema: &Value) -> bool {
    schema.get("type").and_then(Value::as_str) == Some("null")
}

/// The name a `$ref` points at.
fn referred(schema: &Value) -> Option<&str> {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.rsplit('/').next())
}

struct Drawing<'a> {
    defs: &'a Map<String, Value>,
    aim: Aim<'a>,
    open: Vec<String>,
}

impl Drawing<'_> {
    /// Whether `schema` leads, through its references, to the type named `target`.
    fn reaches(&self, schema: &Value, target: &str, seen: &mut BTreeSet<String>) -> bool {
        if let Some(name) = referred(schema) {
            return name == target
                || (seen.insert(name.to_owned())
                    && self
                        .defs
                        .get(name)
                        .is_some_and(|def| self.reaches(def, target, seen)));
        }
        match schema {
            Value::Object(fields) => fields.values().any(|v| self.reaches(v, target, seen)),
            Value::Array(items) => items.iter().any(|v| self.reaches(v, target, seen)),
            _ => false,
        }
    }

    /// Which of a sum's arms to take: the aimed one where this is the aimed sum, else the first
    /// that leads to it.
    fn pick(&self, arms: &[&Value], sum: Option<&str>) -> usize {
        match self.aim {
            Aim::Arm { def, arm } if sum == Some(def) => arm,
            Aim::Arm { def, .. } => arms
                .iter()
                .position(|a| self.reaches(a, def, &mut BTreeSet::new()))
                .unwrap_or(0),
            Aim::First | Aim::Empty => 0,
        }
    }

    /// An example of `schema`; `sum` names the type whose own schema it is, if any.
    fn draw(&mut self, schema: &Value, sum: Option<&str>) -> Value {
        if let Some(name) = referred(schema).map(str::to_owned) {
            let Some(def) = self.defs.get(&name) else {
                return Value::Null;
            };
            // A type met again inside itself takes its first arms, so a recursive type ends.
            let aim = self.aim;
            match self.open.iter().filter(|d| **d == name).count() {
                0 => {}
                1 => self.aim = Aim::First,
                _ => return Value::Null,
            }
            self.open.push(name.clone());
            let drawn = self.draw(def, Some(&name));
            self.open.pop();
            self.aim = aim;
            return drawn;
        }
        if let Some(constant) = schema.get("const") {
            return constant.clone();
        }
        let nulls = ["oneOf", "anyOf"]
            .iter()
            .filter_map(|key| schema.get(*key).and_then(Value::as_array))
            .flatten()
            .any(nullish);
        if nulls && self.aim == Aim::Empty {
            return Value::Null;
        }
        let choices = arms(schema);
        if !choices.is_empty() {
            let arm = choices[self.pick(&choices, sum).min(choices.len() - 1)];
            return if schema.get("enum").is_some() {
                arm.clone()
            } else {
                self.draw(arm, None)
            };
        }
        if let Some(Value::Array(parts)) = schema.get("allOf") {
            let mut merged = Map::new();
            for part in parts {
                if let Value::Object(fields) = self.draw(part, None) {
                    merged.extend(fields);
                }
            }
            return Value::Object(merged);
        }
        if let Some(Value::Object(fields)) = schema.get("properties") {
            return Value::Object(
                fields
                    .iter()
                    .map(|(name, field)| (name.clone(), self.draw(field, None)))
                    .collect(),
            );
        }
        let (kinds, null) = kinds(schema);
        if null && self.aim == Aim::Empty {
            return Value::Null;
        }
        match kinds.first().copied() {
            Some("string") => Value::String("example".to_owned()),
            Some("integer") => Value::from(1),
            Some("number") => Value::from(0.5),
            Some("boolean") => Value::Bool(true),
            Some("array") => {
                let item = schema.get("items").unwrap_or(&Value::Null);
                let again = referred(item).is_some_and(|name| self.open.iter().any(|d| d == name));
                Value::Array(if again {
                    Vec::new()
                } else {
                    vec![self.draw(item, None)]
                })
            }
            _ => Value::Null,
        }
    }
}

/// The types a schema names but null, and whether it names null too.
fn kinds(schema: &Value) -> (Vec<&str>, bool) {
    let named: Vec<&str> = match schema.get("type") {
        Some(Value::String(kind)) => vec![kind.as_str()],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let null = named.contains(&"null");
    (
        named.into_iter().filter(|kind| *kind != "null").collect(),
        null,
    )
}
