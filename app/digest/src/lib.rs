//! The digest: a conversation compacted into summaries within its budget, and the memories the
//! secretary recalls beside it.

#![forbid(unsafe_code)]

pub mod decisions {
    pub mod about;
    pub mod brief;
    pub mod decision_log;
    pub mod summarizing;
}
pub mod log;
pub mod model_name;
pub mod secretary {
    pub mod caused;
    pub mod compact;
    pub mod compact_ask;
    pub mod compact_budget;
    pub mod compact_eval;
    pub mod compacting;
    pub mod compaction;
    pub mod compactor;
    pub mod conversations_log;
    pub mod digest;
    pub mod digest_census;
    pub mod digest_event;
    pub mod digest_label;
    pub mod digest_line;
    pub mod digest_policy;
    pub mod digest_spent;
    pub mod digest_view;
    pub mod digests;
    pub mod indexed_tree;
    pub mod memories;
    pub mod recollection;
    pub mod record_steps;
    pub mod reserve;
    pub mod shown_record;
    pub mod told;
}

// The inert memory and the fake compactor: test support.
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/joining.rs"]
pub mod joining;
#[cfg(any(test, feature = "nothing"))]
#[path = "../tests/support/nothing.rs"]
mod nothing;
