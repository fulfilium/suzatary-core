//! How much the redaction door has run in this module, for the browser scenes that show a reading
//! scrubs nothing: `suzatary_core::redact::scrubs`.

use wasm_bindgen::prelude::*;

/// How many values this module has scrubbed since it was loaded.
#[wasm_bindgen]
#[must_use]
pub fn scrubs() -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a count far below 2^53")]
    let count = suzatary_core::redact::scrubs() as f64;
    count
}
