//! The model port: the `claude` CLI run under a settings file (`model`), the prompt and the
//! blocks it is sent, the setup token's sign-in in a pseudo-terminal (`setup_token`) and the
//! usage the account reports (`usage`).

#![forbid(unsafe_code)]

pub mod blocks;
pub mod model;
pub mod prompt;
pub mod setup_token;
pub mod usage;
