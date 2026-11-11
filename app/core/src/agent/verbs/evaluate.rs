use schemars::{JsonSchema, Schema};
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Call, TriageSurface, reads};
use crate::verb::{Chained, Verb};
use suzatary_base::refusal::Refusal;

/// A priority spec evaluated against confirmed history.
pub struct Evaluate;

#[derive(Deserialize, JsonSchema)]
#[schemars(transform = spec_required)]
pub struct EvaluateArgs {
    #[serde(default, deserialize_with = "given")]
    #[schemars(with = "serde_json::Map<String, Value>")]
    spec_json: Option<Value>,
    at_iso: Option<String>,
}

/// A spec given as null is evaluated as one, which says why it is not a term.
fn given<'de, D: serde::Deserializer<'de>>(spec: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(spec).map(Some)
}

/// The model is told a spec is required; a call without one is answered with why, not refused.
fn spec_required(schema: &mut Schema) {
    if let Some(spec) = schema
        .get_mut("properties")
        .and_then(|properties| properties.get_mut("spec_json"))
        .and_then(Value::as_object_mut)
    {
        spec.remove("default");
    }
    if let Some(required) = schema
        .ensure_object()
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
    {
        required.insert(0, Value::from("spec_json"));
    }
}

impl Verb for Evaluate {
    type Surface = TriageSurface;
    type Args = Chained<EvaluateArgs>;
    type Answer = Value;
    const NAME: &'static str = "evaluate";
    const DESCRIPTION: &'static str = "Evaluate a priority spec at a moment against recorded history — your \
        read-only window into the temporal logic. spec_json is a spec in the fpl \
        JSON grammar (the same shape a todo's prioritySpec carries); it is parsed \
        through the fpl JSON boundary (bad specs are rejected there). Returns the \
        scalar `value` and a decomposable `explain` tree (every node its own \
        fulfillment, Conj nodes their certified floor). Evaluated against \
        trusted-only history as of `at` (defaults to now); provisional triage \
        events do NOT count, so this shows the CONFIRMED belief. `chain` picks \
        whose history it is evaluated against.";

    fn run(call: &Call<'_>, Chained(args): Chained<EvaluateArgs>) -> Result<Value, Refusal> {
        reads::evaluate(
            call.caps,
            call.chain,
            args.spec_json.as_ref(),
            args.at_iso.as_deref(),
        )
    }
}
