//! §7, the evaluator a page prices and draws with.

use prodrome_core::breaks::series_knots as knots;
use prodrome_core::fpl::{self, iso};
use prodrome_wasm_exports::json;
use prodrome_wasm_exports::wire::{
    History, object, parse_closed, parse_env, parse_instant, parse_specs, parse_term,
};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::{printed, refused};

/// What a closed term is worth at `now` under `env`; `undefined` is `∅`, and a number, not a
/// print, since a caller does arithmetic on it.
#[wasm_bindgen]
pub fn fulfillment(term: &str, now: &str, env: &str) -> Result<Option<f64>, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let now = parse_instant("now", now).map_err(refused)?;
    let env = parse_env(env).map_err(refused)?;
    Ok(fpl::fulfillment(&term, now, &env))
}

/// A stored term's canonical print, read with §2's own parser.
#[wasm_bindgen]
pub fn term_json(literal: &str) -> Result<String, JsError> {
    let term = fpl::parse_term(literal).map_err(|e| refused(format!("term: {}", e.0)))?;
    printed(&json::to_json(&term))
}

/// Every `ref` in `term` bound to the todo it names in `specs` (`{todo: term}`), recursively;
/// an unknown todo or a loop is the refusal.
#[wasm_bindgen]
pub fn link(term: &str, specs: &str) -> Result<String, JsError> {
    let term = parse_term("term", term).map_err(refused)?;
    let specs = parse_specs(specs).map_err(refused)?;
    let linked = fpl::link(&term, &specs).map_err(|e| refused(format!("link: {e}")))?;
    printed(&json::to_json(linked.term()))
}

/// The knots of a closed term over `[from, to]`, each priced against `history` as of its own
/// instant, and whether the knots are the curve.
#[wasm_bindgen]
pub fn series_knots(term: &str, from: &str, to: &str, history: &str) -> Result<String, JsError> {
    let term = parse_closed("term", term).map_err(refused)?;
    let from = parse_instant("from", from).map_err(refused)?;
    let to = parse_instant("to", to).map_err(refused)?;
    let past = History::parse(history).map_err(refused)?;
    let series = knots(&term, from, to, |at| past.at(at));
    printed(&object(vec![
        (
            "knots",
            Value::Array(
                series
                    .knots
                    .iter()
                    .map(|knot| {
                        Value::Array(vec![Value::String(iso(knot.at)), Value::from(knot.value)])
                    })
                    .collect(),
            ),
        ),
        ("exact", Value::Bool(series.exact)),
    ]))
}
