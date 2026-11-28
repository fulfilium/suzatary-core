//! The mail transports: IMAP and SMTP over rustls, the Gmail API and the OAuth grants, the
//! pullers and the sync that runs them, the junk sweep, the search of older mail, and the
//! notmuch corpus with its mailbox reads. `smtp` is the only path that sends mail, and only the
//! binary's `send` and the tap reach it.

#![forbid(unsafe_code)]

pub mod calendar;
pub mod corpus;
pub mod gmail;
pub mod google;
pub mod graph;
pub mod imap;
pub mod junk;
pub mod mail_sync;
pub mod mailbox;
pub mod oauth;
pub mod older;
pub mod pullers;
pub mod smtp;
pub mod sync;
