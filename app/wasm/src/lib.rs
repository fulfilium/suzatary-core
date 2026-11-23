//! Suzatary's browser module: the Prodrome's exports at this application's schemas; every method
//! calls the core once, so the tab and the box are one reading. A refusal is the JS exception,
//! never a panic, which would poison the module. No clock: every instant is an argument. A print
//! that does not parse at the schema is left out with what rests on it (SPEC law 44).
#![forbid(unsafe_code)]

mod asks;
mod chain;
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
mod seal;
mod secretary;
mod signatures;
mod todos;
mod topics;
mod tripwire;
mod unrendered;

use wasm_bindgen::JsError;

pub use chain::Chain;
pub use chains::{Chains, Linker};

type Event = suzatary_record::record::TodoEvent;

prodrome_wasm_exports::schema!(Todos = Event, priced);
object_rows::objects_store!(Proposals = suzatary_inbox::secretary::inbox_event::InboxEvent);

fn refused(reason: impl AsRef<str>) -> JsError {
    JsError::new(reason.as_ref())
}

/// The box's wire type, printed as its route prints it.
fn json(value: &impl serde::Serialize) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|e| refused(format!("could not print the answer: {e}")))
}

/// A reading printed whole in UTF-8, which the page parses once (`web/src/wire/text.ts`), and
/// beside it, as JSON, the little of it the worker reads itself.
#[wasm_bindgen::prelude::wasm_bindgen(getter_with_clone)]
pub struct Read {
    pub text: Vec<u8>,
    pub said: String,
}

fn read(value: &impl serde::Serialize, said: &impl serde::Serialize) -> Result<Read, JsError> {
    Ok(Read {
        text: json(value)?.into_bytes(),
        said: json(said)?,
    })
}

fn printed(value: &serde_json::Value) -> Result<String, JsError> {
    prodrome_wasm_exports::printed(value).map_err(refused)
}
