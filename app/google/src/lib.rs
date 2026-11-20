//! Google's sign-in, and the calendar as a source.

#![forbid(unsafe_code)]

pub mod calendar;
pub mod google;
pub mod url;

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
