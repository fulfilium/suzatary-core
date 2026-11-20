//! Every integration test of this crate as one binary: one link of the whole
//! application instead of one per file. Each file stays where it was and reads
//! as it did on its own; a file under `tests/` that is not a module here, or a
//! target of its own in `Cargo.toml`, does not run, and `suites.rs` in the
//! cli's tests fails when one is missing.

#[path = "drive_port.rs"]
mod drive_port;
#[path = "husked.rs"]
mod husked;
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
#[path = "templates.rs"]
mod templates;
#[path = "verify_law.rs"]
mod verify_law;
