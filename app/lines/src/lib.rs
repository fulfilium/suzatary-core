//! The conversations' store: its lines as written, held, opened, read and replicated.

#![forbid(unsafe_code)]

pub mod first_read;
pub mod parts;
pub mod secretary {
    pub mod act_lines;
    pub(crate) mod again;
    pub(crate) mod claim;
    pub mod compare;
    #[cfg(feature = "wasm")]
    mod conversation_json;
    pub mod conversations;
    pub mod derived;
    pub mod held_lines;
    pub mod held_replica;
    pub mod husks;
    pub mod line_event;
    pub mod line_form;
    pub mod line_shape;
    pub mod lines_rule;
    pub mod object_stores;
    pub mod opened;
    pub mod outline;
    pub mod outlines;
    pub mod own_line;
    pub mod prune;
    pub mod rescrub;
    pub mod row;
    pub mod run;
    pub mod saying;
    pub mod sequence;
    pub mod skew;
    pub mod step;
    pub mod targets;
    pub mod transcript;
}
pub mod timings;
pub mod wire {
    pub mod lines;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
pub mod nothing;
