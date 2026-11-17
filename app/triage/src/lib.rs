//! What the box decided about each message: the corpus, junk, older mail, watches, the triage
//! state, and the rules the box runs over its readings.

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
    //! The web's payloads of a mail watch.
    pub mod triage;
}

// The inert corpus, Junk folder, older mail and watch log: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
