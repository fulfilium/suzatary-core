//! Turns the Prodrome's conformance vectors into the JSON `web/test/wasm.test.ts` reads, parsed
//! with the crate's own `literal::parse_literal` so the grammar has one reader.
//!
//! A browser test has no literal parser (`prodrome-wasm` exposes typed operations only).
//! `term-json.json`'s `env`/`json` fields, `prodrome-wasm`'s own frozen codec evidence, are
//! copied through rather than re-derived; only `fpl.py`'s samples and `series.py` are converted.
//!
//! Run with `cargo run --example vector_json -- fpl <fpl.py> <term-json.json>` or
//! `cargo run --example vector_json -- series <series.py>`; both print JSON to stdout.
//! `flake.nix`'s `suzatary-vector-json` and `scripts/web-test.sh` run them.

use std::collections::HashMap;
use std::{env, fs};

use prodrome_core::fpl::{instant_of, iso};
use prodrome_core::literal::{Open, Value, parse_literal};
use serde_json::json;

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_call()
        .and_then(|call| call.field(name))
        .unwrap_or_else(|| panic!("no field {name:?} on {value:?}"))
}

fn each<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    field(value, name)
        .as_tuple()
        .unwrap_or_else(|| panic!("field {name:?} is not a tuple, on {value:?}"))
}

fn text_of(value: &Value) -> &str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("expected a string, got {value:?}"))
}

fn dt_iso(value: &Value) -> String {
    match value {
        Value::Datetime(at) => iso(instant_of(*at)),
        other => panic!("expected a datetime, got {other:?}"),
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a Sample/Knot value is a fulfillment or a count the vector grammar happened to \
              print without a decimal point; every one of them is well inside f64's exact \
              integer range, and this tool only re-spells JSON, it does not price anything"
)]
fn num(value: &Value) -> f64 {
    match value {
        Value::Float(f) => f.get(),
        Value::Int(i) => i.as_i64().expect("an integer that fits") as f64,
        other => panic!("expected a number, got {other:?}"),
    }
}

fn boolean(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        other => panic!("expected a bool, got {other:?}"),
    }
}

fn parse(path: &str) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    parse_literal(&text, &Open).unwrap_or_else(|e| panic!("{path} is not a vector literal: {e}"))
}

/// `fpl.py`'s 150 `FplCase`s, `env`/`json` copied from `term-json.json` by the
/// term print (the key the two files share), `samples` converted from the
/// grammar's `Sample(now=datetime(...), value=...)` tuples.
fn fpl_json(vector_path: &str, term_json_path: &str) {
    let root = parse(vector_path);
    let cases = each(&root, "terms");

    let term_json_text = fs::read_to_string(term_json_path)
        .unwrap_or_else(|e| panic!("cannot read {term_json_path}: {e}"));
    let term_json: serde_json::Value = serde_json::from_str(&term_json_text)
        .unwrap_or_else(|e| panic!("{term_json_path} is not JSON: {e}"));
    let entries = term_json["terms"]
        .as_array()
        .unwrap_or_else(|| panic!("{term_json_path} has no \"terms\" array"));
    let by_term: HashMap<&str, &serde_json::Value> = entries
        .iter()
        .map(|entry| {
            let term = entry["term"]
                .as_str()
                .unwrap_or_else(|| panic!("a {term_json_path} entry with no \"term\""));
            (term, entry)
        })
        .collect();

    let terms: Vec<serde_json::Value> = cases
        .iter()
        .map(|case| {
            let term = text_of(field(case, "term"));
            let entry = by_term
                .get(term)
                .unwrap_or_else(|| panic!("{term}: no matching entry in {term_json_path}"));
            let samples: Vec<serde_json::Value> = each(case, "samples")
                .iter()
                .map(|sample| {
                    json!({
                        "now": dt_iso(field(sample, "now")),
                        "value": num(field(sample, "value")),
                    })
                })
                .collect();
            json!({
                "term": term,
                "env": entry["env"],
                "json": entry["json"],
                "samples": samples,
            })
        })
        .collect();
    println!("{}", json!({ "terms": terms }));
}

/// `series.py`'s `SeriesCase`s, whole: `term-json.json` holds no series vector to copy.
fn series_json(vector_path: &str) {
    let root = parse(vector_path);
    let cases = each(&root, "series");
    let series: Vec<serde_json::Value> = cases
        .iter()
        .map(|case| {
            let knots: Vec<serde_json::Value> = each(case, "knots")
                .iter()
                .map(|knot| json!([dt_iso(field(knot, "at")), num(field(knot, "value"))]))
                .collect();
            json!({
                "term": text_of(field(case, "term")),
                "from": dt_iso(field(case, "from")),
                "to": dt_iso(field(case, "to")),
                "exact": boolean(field(case, "exact")),
                "knots": knots,
            })
        })
        .collect();
    println!("{}", json!({ "series": series }));
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match (args.get(1).map(String::as_str), args.get(2), args.get(3)) {
        (Some("fpl"), Some(vectors), Some(term_json)) => fpl_json(vectors, term_json),
        (Some("series"), Some(vectors), None) => series_json(vectors),
        _ => {
            eprintln!(
                "usage: vector_json fpl <fpl.py> <term-json.json>\n   or: vector_json series <series.py>"
            );
            std::process::exit(2);
        }
    }
}
