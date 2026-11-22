//! The keys the box keeps rendered blocks under, named by the same core the box runs.

use suzatary_base::answer::Renderable;
use suzatary_todo::rendering::ANSWER;
use wasm_bindgen::prelude::*;

/// The answer template's key at `source`, as the box prints it beside the block's HTML.
#[wasm_bindgen]
pub fn answer_key(source: &str) -> Result<String, JsError> {
    ANSWER
        .key(&Renderable::of(source))
        .map_err(|e| JsError::new(&e.to_string()))
}

/// The names of the functions a device keeps blocks of, as JSON `[<name>, …]`: a block under any
/// other was named by another build, and goes.
#[wasm_bindgen]
pub fn kept_functions() -> Result<String, JsError> {
    crate::json(
        &suzatary_todo::rendering::kept()
            .map(prodrome_core::event::Hash::as_str)
            .to_vec(),
    )
}
