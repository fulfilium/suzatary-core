//! The routes, the gate and the wire that the browser and the server share: the top of the
//! application, over `suzatary-core`.

#![forbid(unsafe_code)]

pub mod clienterror;
pub mod gate;
pub mod portal;
pub mod query;
pub mod routes;
pub mod secretary {
    //! The secretary's routes.
    pub mod routes;
}
pub mod server;
pub mod wire;

// The inert server: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
