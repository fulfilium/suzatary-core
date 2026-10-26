//! Suzatary's browser module: the Prodrome's exports, generic over a schema, instantiated at
//! this application's schemas, beside the evaluator and the lifecycle events a replica spells.
//!
//! A class per schema (`new Todos(objects)` reads the objects once; each method asks its
//! question of what was read, and `append` writes into it: a tab's act is the Prodrome's own
//! `Change`, its deps read off this replica's fold, so the tab and the box write the same bytes
//! holding the same objects). Everything here parses its arguments, calls the core once and
//! prints the answer: no arithmetic and no policy, so the tab and the box are one reading.
//! A refusal is the JS exception, never a panic, which would poison the module for the session.
//! No clock: every instant is an argument.
//!
//! Every reading is total over what a class holds (Prodrome SPEC law 44): a print that does not
//! parse at the schema is left out with what rests on it. The Prodrome's own readings answer
//! `excluded`; this module's are the box's answers over what was read, byte for byte, and leave
//! what was left out to `tips`, a function of the prints the tab asks once per reading.
#![forbid(unsafe_code)]

mod chains;
mod evaluator;
mod lifecycle;
mod memo;
mod prose;
mod redact;
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
type Event = suzatary_core::record::TodoEvent;

prodrome_wasm_exports::schema!(Todos = Event, priced);
prodrome_wasm_exports::schema!(Proposals = suzatary_core::secretary::inbox_event::InboxEvent);

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
