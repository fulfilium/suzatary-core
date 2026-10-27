//! How much the module's costly steps have run, for the browser scenes that show what a reading
//! costs: the redaction door (`suzatary_core::redact::scrubs`) and the lines' decoding of a print
//! (`suzatary_core::secretary::held_lines::decodes`).

use wasm_bindgen::prelude::*;

#[expect(clippy::cast_precision_loss, reason = "a count far below 2^53")]
fn count(n: usize) -> f64 {
    n as f64
}

/// How many values this module has scrubbed since it was loaded.
#[wasm_bindgen]
#[must_use]
pub fn scrubs() -> f64 {
    count(suzatary_core::redact::scrubs())
}

/// How many prints this module's lines have decoded since it was loaded.
#[wasm_bindgen]
#[must_use]
pub fn decodes() -> f64 {
    count(suzatary_core::secretary::held_lines::decodes())
}
