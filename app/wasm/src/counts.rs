//! How much the module's costly steps have run, for the browser scenes that show what a reading
//! costs.

use suzatary_lines::secretary::held_lines;
use wasm_bindgen::prelude::*;

#[expect(clippy::cast_precision_loss, reason = "a count far below 2^53")]
fn count(n: usize) -> f64 {
    n as f64
}

#[wasm_bindgen]
#[must_use]
pub fn scrubs() -> f64 {
    count(suzatary_held::redact::scrubs())
}

#[wasm_bindgen]
#[must_use]
pub fn decodes() -> f64 {
    count(held_lines::decodes())
}

/// How many of those prints were hashed against their names first.
#[wasm_bindgen]
#[must_use]
pub fn hashes() -> f64 {
    count(held_lines::hashes())
}

/// How many sealed slots this module has decrypted since it was loaded.
#[wasm_bindgen]
#[must_use]
pub fn opens() -> f64 {
    count(suzatary_memory::secretary::locked_text::opens())
}
