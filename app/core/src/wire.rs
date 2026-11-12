//! The web's payloads, one module per cut that names them; `suzatary_api::wire` holds the rest
//! and generates their TypeScript.

pub mod cards;
pub mod inbox;
pub mod lines;
pub mod signatures;
pub mod store;
pub mod todo;
pub mod topics;
pub mod triage;
