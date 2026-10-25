//! The record of the conversations as a reader of the owner's words alone holds it: the digest's
//! reading behind a private field, reached only through what shows the owner's own lines and the
//! summaries of nothing but those ([`Label::is_owners`]), every other part by its kinds. The
//! triage agent reads mail anyone can send, and the secretary's answers and review rounds can
//! quote mail or a batch the agent once wrote, so its record is this type and never a
//! [`Reading`], whose lines a new verb could reach.
//!
//! ```compile_fail,E0599
//! fn lines(record: &suzatary_core::secretary::owners_record::OwnersRecord) {
//!     let _ = record.entries();
//! }
//! ```

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

/// The owner's part of the record.
pub struct OwnersRecord {
    reading: Reading,
}

impl OwnersRecord {
    /// `reading`, from now on shown in the owner's words alone.
    #[must_use]
    pub fn of(reading: Reading) -> Self {
        Self { reading }
    }

    /// Opening `handle`, as [`told::zoom`] answers it to a reader of the owner's words alone.
    ///
    /// # Errors
    /// [`told::zoom`]'s.
    pub fn zoom(&self, handle: &str) -> Result<Value, Refusal> {
        told::zoom(&self.reading, handle, Label::is_owners)
    }

    /// The owner's lines, and the summaries of nothing but such lines, that hold `needle`, the
    /// newest `limit` of them, each by its handle and with its words around the match.
    #[must_use]
    pub fn find(&self, needle: &Needle, limit: usize) -> Vec<Value> {
        let digest = self.reading.digest();
        let entries = self.reading.entries();
        let mut found: Vec<(usize, Value)> = Vec::new();
        let mut stack: Vec<&Node<Label>> = digest.tree().map(Tree::root).into_iter().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.children());
            if !node.measure().is_owners() {
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
