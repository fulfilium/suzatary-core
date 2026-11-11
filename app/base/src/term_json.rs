//! An FPL term as the web's JSON, read and printed through the Prodrome's term
//! schema: lowercase kind tags, ISO instants, spans in hours. The same shape
//! `prodrome-wasm` renders, frozen by its `conformance/term-json.json`; the
//! wasm crate is a cdylib, so this crate reads the schema itself.

use prodrome_core::fpl::{
    Closed, Delta, Env, Explanation, FplError, Instant, Note, Scalar, delta_from_hours, explained,
    parse_iso, scalar, wire_key,
};
use prodrome_core::term::Term;
use prodrome_core::term::schema::{Field, FieldSource, Fields, Slot, build, fields};
use serde_json::{Map, Value};

/// One JSON object's fields, `null` or left out being absent.
struct JsonSource<'a>(&'a Value);

/// A term's fields, printed the wire's way.
#[must_use]
pub fn to_json(term: &Term) -> Value {
    let mut out = object(fields(term.out()).1);
    out.insert("kind".into(), Value::String(term.out().kind().to_owned()));
    Value::Object(out)
}

/// A term back from [`to_json`]'s shape, through the core's own `mk_*`.
///
/// # Errors
/// A malformed or unknown shape, as the core's [`FplError`].
pub fn from_json(json: &Value) -> Result<Term, FplError> {
    let tag = json.get("kind").and_then(Value::as_str).unwrap_or("");
    let mut chars = tag.chars();
    match chars.next() {
        Some(initial) if initial.is_ascii_lowercase() => build(
            &format!("{}{}", initial.to_ascii_uppercase(), chars.as_str()),
            &mut JsonSource(json),
        ),
        _ => Err(FplError(format!("unknown term kind: {tag:?}"))),
    }
}

/// The priced explanation tree: [`to_json`]'s shape with each node's `value`
/// (`null` for `∅`) and its notes, each part explained in place.
#[must_use]
pub fn explain(term: &Closed, now: Instant, env: &Env) -> Value {
    explanation_json(&explained(term, now, env))
}

fn explanation_json(node: &Explanation) -> Value {
    let (kind, fields) = fields(&*node.node);
    let mut out = Map::new();
    out.insert("kind".into(), Value::String(node.node.kind().to_owned()));
    out.insert("value".into(), Value::from(node.value));
    for (key, note) in &node.notes {
        out.insert(key.clone(), note_json(note));
    }
    for (name, slot) in fields {
        // The piece in force rides in `Piecewise`'s `head`, printed as "term".
        let name = if kind == "Piecewise" { "term" } else { name };
        match slot {
            Slot::Child(child) => {
                out.insert(name.into(), explanation_json(child));
            }
            Slot::Children(children) => {
                out.insert(
                    name.into(),
                    children.into_iter().map(explanation_json).collect(),
                );
            }
            _ => {}
        }
    }
    Value::Object(out)
}

/// Every field, its leaves as scalars; an unset optional or empty text omitted.
fn object(fields: Fields<&Term>) -> Map<String, Value> {
    fields
        .into_iter()
        .filter_map(|(name, slot)| {
            let key = wire_key(name, matches!(slot, Slot::Span(_)));
            let value = match slot {
                Slot::Child(term) => to_json(term),
                Slot::Children(terms) => terms.into_iter().map(to_json).collect(),
                Slot::Rows(_, rows) => rows
                    .into_iter()
                    .map(|row| Value::Object(object(row)))
                    .collect(),
                leaf => scalar_json(&scalar(&leaf)?),
            };
            Some((key, value))
        })
        .collect()
}

fn scalar_json(scalar: &Scalar) -> Value {
    match scalar {
        Scalar::Text(text) => Value::String(text.clone()),
        Scalar::Float(float) => Value::from(*float),
        Scalar::Int(int) => Value::from(*int),
        Scalar::Bool(bool) => Value::Bool(*bool),
    }
}

fn note_json(note: &Note) -> Value {
    match note {
        Note::One(scalar) => scalar_json(scalar),
        Note::Many(scalars) => scalars.iter().map(scalar_json).collect(),
        Note::Maps(maps) => maps
            .iter()
            .map(|map| {
                Value::Object(
                    map.iter()
                        .map(|(key, value)| (key.clone(), scalar_json(value)))
                        .collect(),
                )
            })
            .collect(),
    }
}

impl<'a> JsonSource<'a> {
    fn read<T>(
        &self,
        name: &'static str,
        span: bool,
        decode: impl FnOnce(&'a Value) -> Option<T>,
    ) -> Field<T> {
        let key = wire_key(name, span);
        Field::new(
            name,
            match self.0.get(&key) {
                None | Some(Value::Null) => Ok(None),
                Some(value) => decode(value)
                    .map(Some)
                    .ok_or_else(|| FplError(format!("term json {key:?} is malformed"))),
            },
        )
    }
}

impl FieldSource for JsonSource<'_> {
    fn real(&mut self, name: &'static str) -> Field<f64> {
        self.read(name, false, Value::as_f64)
    }

    fn at(&mut self, name: &'static str) -> Field<Instant> {
        self.read(name, false, Value::as_str).and_then(parse_iso)
    }

    fn span(&mut self, name: &'static str) -> Field<Delta> {
        self.read(name, true, |value| value.as_f64().map(delta_from_hours))
    }

    fn text(&mut self, name: &'static str) -> Field<String> {
        self.read(name, false, |value| value.as_str().map(str::to_owned))
    }

    fn child(&mut self, name: &'static str) -> Field<Term> {
        self.read(name, false, Some).and_then(from_json)
    }

    fn children(&mut self, name: &'static str) -> Field<Vec<Term>> {
        self.read(name, false, Value::as_array)
            .and_then(|items| items.iter().map(from_json).collect())
    }

    fn rows<T>(
        &mut self,
        name: &'static str,
        _row: &'static str,
        mut read: impl FnMut(&mut Self) -> Result<T, FplError>,
    ) -> Field<Vec<T>> {
        self.read(name, false, Value::as_array).and_then(|items| {
            items
                .iter()
                .map(|item| read(&mut JsonSource(item)))
                .collect()
        })
    }
}
