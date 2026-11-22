//! Every integration test of this crate as one binary; a file under `tests/` that is not a module
//! here does not run, and the cli's `suites.rs` fails when one is missing.

#[path = "connect.rs"]
mod connect;
#[path = "drive_port.rs"]
mod drive_port;
#[path = "puller_port.rs"]
mod puller_port;
#[path = "transport_port.rs"]
mod transport_port;
