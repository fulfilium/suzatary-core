//! The names every crate of Suzatary speaks: ids, references, chains, zones, refusals, secrets,
//! the markup escape, and the ports for the clock, entropy, typesetting and whois.

#![forbid(unsafe_code)]

pub mod answer;
pub mod capability;
pub mod chains;
pub mod conversation_id;
pub mod drive_path;
pub mod escape;
pub mod finite;
mod literals;
// The file is hashed into every kept render's name (`rendering.rs`), so its link to the
// renderer, a crate above this one, stays as written.
#[allow(rustdoc::broken_intra_doc_links)]
pub mod narrowing;
#[cfg(any(test, feature = "nothing"))]
pub mod own;
pub mod persona;
pub mod reference;
pub mod refusal;
pub mod repo_ids;
pub mod rule_version;
pub mod spelling;
pub mod table;
pub mod term_json;
pub mod topic_target;
pub mod tree;
pub mod tripwire;
pub mod wire {
    //! The web's payloads that every crate names.
    pub mod base;
}
pub mod words;
pub mod writer;
pub mod zone;
