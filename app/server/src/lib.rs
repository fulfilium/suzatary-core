//! The box's half of the routes: the effects record the serving process supplies, the
//! secretary's routes over it, the door and the portal's answer, and `serve`, the axum shell a
//! running server is: its state, handlers and route table. The wire and the routes the tab shares
//! are `suzatary-api`'s.

#![forbid(unsafe_code)]

pub mod gate;
pub mod portal;
pub mod routes;
pub mod secretary {
    //! The secretary's routes.
    pub mod routes;
}
pub mod serve;
pub mod server;

// The inert server: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
