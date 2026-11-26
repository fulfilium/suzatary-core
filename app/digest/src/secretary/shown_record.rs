//! The record of the conversations as one reader may read it: the digest's reading behind a
//! private field, reached only through what shows the parts its [`Shows`] reads, every other
//! part by its kinds. The triage agent reads mail anyone can send, and the secretary's answers
//! and review rounds can quote mail or a batch the agent once wrote, so its record is an
//! [`OwnersRecord`], which shows the owner's own lines and the summaries of nothing but those
//! ([`Label::is_owners`]), and never a [`Reading`], whose lines a new verb could reach. A
//! reader's [`Shows`] is a type, so no value the agent holds can open more.
//!
//! ```compile_fail,E0599
//! fn lines(record: &suzatary_digest::secretary::shown_record::OwnersRecord) {
//!     let _ = record.entries();
//! }
//! ```

use std::marker::PhantomData;

use prodrome_digest::{Node, Part, Tree};
use serde_json::{Value, json};

use crate::secretary::compaction::Reading;
use crate::secretary::conversations_log::Conversations;
use crate::secretary::digest_label::Label;
use crate::secretary::digest_view;
use crate::secretary::told::{self, Told};
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::search;
use suzatary_conversations::secretary::search::Needle;
use suzatary_memory::secretary::memory::Memory;
use suzatary_topics::secretary::topics::reading::Reading as Topics;

pub struct ShownRecord<S: Shows> {
    reading: Reading,
    shows: PhantomData<S>,
}

/// Which parts of the record a reader reads in words; it reads every other part by its kinds.
pub trait Shows {
    fn shows(label: &Label) -> bool;
}

/// The owner's own words, and the summaries of nothing but those: what the triage agent reads.
pub enum OwnersWords {}

impl Shows for OwnersWords {
    fn shows(label: &Label) -> bool {
        label.is_owners()
    }
}

/// Every clean part: what the compactor of a clean node is shown.
pub enum CleanWords {}

impl Shows for CleanWords {
    fn shows(label: &Label) -> bool {
        label.is_clean()
    }
}

/// The parts the secretary's call reads in words: clean, and nothing but the owner's turns, acts
/// and erasures and the secretary's own answers. A box's note can hold a connector's own words,
/// and a clean verb's answer an outside server's, so a part holding either is read by its kinds.
pub enum OwnersAndAnswers {}

impl Shows for OwnersAndAnswers {
    fn shows(label: &Label) -> bool {
        label.is_clean()
            && label.kinds.iter().all(|kind| {
                matches!(
                    kind,
                    crate::secretary::digest_label::Kind::Person
                        | crate::secretary::digest_label::Kind::Act
                        | crate::secretary::digest_label::Kind::Erased
                        | crate::secretary::digest_label::Kind::Model
                )
            })
    }
}

/// Every part, each answer carrying the taint of what it shows.
pub enum Everything {}

impl Shows for Everything {
    fn shows(_: &Label) -> bool {
        true
    }
}

pub type OwnersRecord = ShownRecord<OwnersWords>;

impl<S: Shows> ShownRecord<S> {
    #[must_use]
    pub fn of(reading: Reading) -> Self {
        Self {
            reading,
            shows: PhantomData,
        }
    }

    pub fn zoom(&self, handle: &str) -> Result<Value, Refusal> {
        told::zoom(&self.reading, handle, S::shows)
    }

    /// The view within `budget` bytes, one line per part in the log's order, before it is fitted.
    #[must_use]
    pub fn lines(&self, budget: usize) -> Vec<told::Spanned> {
        told::spanned_lines(&self.reading, budget.into(), None, S::shows).all()
    }

    /// The summaries this reader reads in words in the view within `budget` bytes.
    #[must_use]
    pub fn summaries(&self, budget: usize) -> Vec<(Part, crate::secretary::digest_line::Line)> {
        digest_view::view(
            &Conversations::new(self.reading.digest(), &self.reading.entries()),
            budget,
            S::shows,
        )
        .into_iter()
        .filter_map(|shown| match shown.body {
            digest_view::Body::Derived(line) => Some((shown.part, line)),
            digest_view::Body::Said(_)
            | digest_view::Body::Hidden(_)
            | digest_view::Body::Unsummarised(_) => None,
        })
        .collect()
    }

    /// The lines and summaries this reader reads in words that hold `needle`, the newest `limit`
    /// of them.
    #[must_use]
    pub fn find(&self, needle: &Needle, limit: usize) -> Vec<Value> {
        let digest = self.reading.digest();
        let entries = self.reading.entries();
        let mut found: Vec<(usize, Value)> = Vec::new();
        let mut stack: Vec<&Node<Label>> = digest.tree().map(Tree::root).into_iter().collect();
        while let Some(node) = stack.pop() {
            stack.extend(node.children());
            if !S::shows(&digest.label(node)) {
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

/// What the agent is told of `memory`, with the newest of `lines` that fit `budget` bytes, the
/// rest named by a handle `zoom` opens, and with no topic, since a topic is the secretary's own
/// word for a conversation.
#[must_use]
pub fn told(memory: &Memory, lines: Result<&[told::Spanned], &Refusal>, budget: usize) -> Told {
    told::of(
        told::Reader::Agent,
        Ok(memory),
        Ok(&Topics::default()),
        lines.map(|lines| told::fit_latest(lines.to_vec(), budget)),
    )
}
