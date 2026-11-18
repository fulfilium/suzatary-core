//! The cards the box files on its own: what setup owes, what fails, what a coding session
//! reports, what was withdrawn, and the pushes that say so.

#![forbid(unsafe_code)]

pub mod notify;
pub mod secretary {
    pub mod afresh;
    pub mod bootstrap;
    pub mod connections;
    pub mod done_elsewhere;
    pub mod failing;
    pub mod fixes;
    pub mod google_sign_in;
    pub mod lapsed;
    pub mod merged_patches;
    pub mod noticed;
    pub mod undo;
}
pub mod settings {
    //! The settings page's overview, beside `suzatary_connect::settings`.
    pub mod overview;
}
pub mod wire {
    pub mod cards;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
