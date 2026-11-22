//! What the box decided about each message, and the rules it runs over its readings.

#![forbid(unsafe_code)]

pub mod mail {
    //! The modules beside `suzatary_mail::mail` that read what triage decided.
    pub mod corpus;
    pub mod gone;
    pub mod heartbeat;
    pub mod junk;
    pub mod junk_folder;
    pub mod known;
    pub mod local_state;
    pub mod older;
    pub mod older_mail;
    pub mod state;
    pub mod triage;
    pub mod verbs;
    pub mod watch;
}
pub mod rule;
pub mod wire {
    pub mod triage;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
