//! What the application reads as an email address, and an attendee's name, which holds none. The
//! calendar plugin links this crate, so both sides of a plugin's door apply one rule.

#![forbid(unsafe_code)]

mod attendee_name;
mod email;

pub use attendee_name::{AttendeeName, MAX_NAME, NotAName};
pub use email::{holds_address, replace_addresses};
