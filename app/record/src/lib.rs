//! What one todo record is: its kinds, its fields and the two source languages of its markup.

#![forbid(unsafe_code)]

// The name `#[derive(Literal)]` gives the core.
extern crate prodrome_core as prodrome;

pub mod decoration;
pub mod record;
pub mod todo;
pub mod wire {
    //! The web's payloads of a record.
    pub mod record;
}
