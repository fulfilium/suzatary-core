//! The machine that only climbs: proposals, the box's acts, taps and their phases.

#![forbid(unsafe_code)]

pub mod coding;
pub mod content_id;
pub mod priced;
pub mod secretary {
    pub mod act_vocabulary;
    pub mod acts;
    pub mod card;
    pub mod changes;
    pub mod decide;
    pub mod diff;
    pub mod fix;
    pub mod inbox;
    pub mod inbox_event;
    #[cfg(feature = "wasm")]
    mod inbox_json;
    pub mod inbox_reading;
    pub mod inbox_registers;
    pub mod need;
    pub mod own_round;
    pub mod phase;
    pub mod proposal;
    pub mod proposer;
    pub mod request;
    pub mod responded;
    pub mod sending;
    pub mod sign_in;
    pub mod stamp;
    pub mod tapped;
}
pub mod verb {
    mod arguments;
    mod caught;

    pub use arguments::parse;
    pub use caught::Caught;
}
pub mod wire {
    pub mod inbox;
}

// A proposal and every response to one, drawn for the schema's laws: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/drawn.rs"]
pub mod drawn;
