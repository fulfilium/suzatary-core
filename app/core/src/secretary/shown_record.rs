//! The record of the conversations as one reader may read it: the digest's reading behind a
//! private field, reached only through what shows the parts its [`Shows`] reads, every other
//! part by its kinds. The triage agent reads mail anyone can send, and the secretary's answers
//! and review rounds can quote mail or a batch the agent once wrote, so its record is an
//! [`OwnersRecord`], which shows the owner's own lines and the summaries of nothing but those
//! ([`Label::is_owners`]), and never a [`Reading`], whose lines a new verb could reach. A
//! reader's [`Shows`] is a type, so no value the agent holds can open more.
//!
//! ```compile_fail,E0599
//! fn lines(record: &suzatary_core::secretary::shown_record::OwnersRecord) {
//!     let _ = record.entries();
//! }
//! ```

use std::marker::PhantomData;

use prodrome_core::digest::{Node, Part, Tree};
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::compaction::Reading;
use crate::secretary::digest_label::Label;
use crate::secretary::digest_view;
use crate::secretary::memory::Memory;
use crate::secretary::search::{self, Needle};
use crate::secretary::told::{self, Told};
use crate::secretary::topics::reading::Reading as Topics;

/// The record, shown as `S` reads it.
pub struct ShownRecord<S: Shows> {
    reading: Reading,
    shows: PhantomData<S>,
}

/// Which parts of the record a reader reads in words; it reads every other part by its kinds.
pub trait Shows {
    /// Whether a part of `label` is read in words.
    fn shows(label: &Label) -> bool;
}

/// The owner's own words, and the summaries of nothing but those: what the triage agent reads.
pub enum OwnersWords {}

impl Shows for OwnersWords {
    fn shows(label: &Label) -> bool {
        label.is_owners()
    }
}

/// Every clean part: what the compactor is shown beside a node not of the owner's words alone.
pub enum CleanWords {}

impl Shows for CleanWords {
    fn shows(label: &Label) -> bool {
        label.is_clean()
    }
}

/// Every part: what the secretary opens, each answer carrying the taint of what it shows.
pub enum Everything {}

impl Shows for Everything {
    fn shows(_: &Label) -> bool {
        true
    }
}

/// The record in the owner's words alone.
pub type OwnersRecord = ShownRecord<OwnersWords>;

impl<S: Shows> ShownRecord<S> {
    /// `reading`, from now on shown as `S` reads it.
    #[must_use]
    pub fn of(reading: Reading) -> Self {
        Self {
            reading,
            shows: PhantomData,
        }
    }

    /// Opening `handle`, as [`told::zoom`] answers it to this reader.
    ///
    /// # Errors
    /// [`told::zoom`]'s.
    pub fn zoom(&self, handle: &str) -> Result<Value, Refusal> {
        told::zoom(&self.reading, handle, S::shows)
    }

    /// The lines and summaries this reader reads in words that hold `needle`, the newest `limit`
    /// of them, each by its handle and with its words around the match.
    #[must_use]
    pub fn find(&self, needle: &Needle, limit: usize) -> Vec<Value> {
        let digest = self.reading.digest();
        let entries = self.reading.entries();
        let mut found: Vec<(usize, Value)> = Vec::new();
        let mut stack: Vec<&Node<Label>> = digest.tree().map(Tree::root).into_iter().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.children());
            if !S::shows(node.measure()) {
                continue;
            }
            let (kind, text) = if node.is_leaf() {
                let Some(entry) = entries.get(digest.line(node.name())) else {
                    continue;
                };
                ("said", digest_view::whole(entry))
            } else {
                let Some(summary) = digest.summary(node.name()) else {
                    continue;
                };
                ("derived", summary.as_str().to_owned())
            };
            if let Some(excerpt) = needle.excerpt(&text, search::RECALL_CONTEXT) {
                let part = Part {
                    node: node.name().clone(),
                    span: node.span(),
                };
                found.push((
                    node.span().start,
                    json!({ "handle": part.handle(), kind: excerpt }),
                ));
            }
        }
        found.sort_by_key(|(first, _)| std::cmp::Reverse(*first));
        found.into_iter().take(limit).map(|(_, hit)| hit).collect()
    }
}

/// What a session opening over `memory` and `record` (or why it was not read) is told, its view
/// within `budget` bytes, as [`told::context_of`] tells it and with no topic, since a topic is
/// the secretary's own word for a conversation.
#[must_use]
pub fn told(memory: &Memory, record: Result<&OwnersRecord, &Refusal>, budget: usize) -> Told {
    told::context_of(
        memory,
        Ok(&Topics::default()),
        record.map(|record| &record.reading),
        budget,
        None,
    )
}
