//! One account's mail as the protocols carry it: its address, IMAP, SMTP, `OAuth2` and the
//! message.

#![forbid(unsafe_code)]

pub mod mail;
pub mod outcome;

// The inert mailbox and sync: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
