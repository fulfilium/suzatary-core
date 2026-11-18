//! The readings of a conversation that the browser shares: its topics, the cards it draws and
//! files, the coding sessions they start, and the stream.

#![forbid(unsafe_code)]

pub mod secretary {
    pub mod answers;
    pub mod build;
    pub mod coding;
    pub mod filed;
    pub mod readings;
    pub mod rules;
    pub mod set_up;
    pub mod stream;
    pub mod title;
    pub mod topics;
}
pub mod wire {
    pub mod topics;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
