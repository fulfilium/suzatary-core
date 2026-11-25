//! A conversation's entries and their fold, and the model's port.

#![forbid(unsafe_code)]

pub mod nonempty;
pub mod secretary {
    pub mod about;
    pub mod found;
    pub mod model;
    pub mod model_sessions;
    pub mod outcome;
    pub mod queue;
    pub mod search;
    pub mod sentence;
    pub mod store;
    pub mod taint;
    pub mod tool_name;
    pub mod turn_shape;
    pub mod verb_name;
}

#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
