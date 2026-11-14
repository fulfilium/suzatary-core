//! Suzatary's browser module: the Prodrome's exports at this application's schemas, beside the
//! evaluator and the lifecycle events a replica spells. A class per schema reads its objects
//! once; every method parses its arguments, calls the core once and prints the answer, so the
//! tab and the box are one reading. A refusal is the JS exception, never a panic, which would
//! poison the module. No clock: every instant is an argument. A print that does not parse at the
//! schema is left out with what rests on it (SPEC law 44), and `tips` says what was.
#![forbid(unsafe_code)]

mod asks;
mod chains;
mod counts;
mod evaluator;
mod grow;
mod lifecycle;
mod line_rows;
mod memo;
mod object_rows;
mod prose;
mod rendering;
mod secretary;
mod signatures;
mod todos;
mod topics;
mod tripwire;
mod unrendered;

use wasm_bindgen::JsError;

pub use chains::{Chains, Linker};

/// The todo schema at this application's record.
type Event = suzatary_record::record::TodoEvent;

prodrome_wasm_exports::schema!(Todos = Event, priced);
object_rows::objects_store!(Proposals = suzatary_core::secretary::inbox_event::InboxEvent);

fn refused(reason: impl AsRef<str>) -> JsError {
    JsError::new(reason.as_ref())
}

/// The box's wire type, printed as its route prints it.
fn json(value: &impl serde::Serialize) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|e| refused(format!("could not print the answer: {e}")))
}

fn printed(value: &serde_json::Value) -> Result<String, JsError> {
    prodrome_wasm_exports::printed(value).map_err(refused)
}
