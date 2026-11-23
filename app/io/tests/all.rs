//! Every integration test of this crate as one binary, one link instead of one per file; a file
//! under `tests/` that is neither a module here nor its own target does not run, and the cli's
//! `suites.rs` fails then.

#[path = "drive_port.rs"]
mod drive_port;
#[path = "inbox_readings.rs"]
mod inbox_readings;
#[path = "legacy_stores.rs"]
mod legacy_stores;
#[path = "model_port.rs"]
mod model_port;
#[path = "puller_port.rs"]
mod puller_port;
#[path = "round_trip.rs"]
mod round_trip;
#[path = "script_law.rs"]
mod script_law;
#[path = "script_law_layers.rs"]
mod script_law_layers;
#[path = "store_laws.rs"]
mod store_laws;
#[path = "stores.rs"]
mod stores;
#[path = "verify_law.rs"]
mod verify_law;
