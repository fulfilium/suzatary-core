//! The box's tripwire, for the tab's door: one implementation, `suzatary_base::tripwire`.

use wasm_bindgen::prelude::*;

/// What is wrong with this compiled HTML, or `undefined`; the tab refuses exactly what the box refuses.
#[wasm_bindgen]
#[must_use]
pub fn suspicious(html: &str) -> Option<String> {
    suzatary_base::tripwire::suspicious(html).map(str::to_owned)
}
