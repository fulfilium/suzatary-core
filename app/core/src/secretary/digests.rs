//! The digest's stores as one value: its history, a prodrome at the [`DigestEvent`] schema that
//! names each summary's words, its texts, the words themselves, and the conversations whose lines
//! are the tree's leaves.
//!
//! Reading takes no lock. Writing a summary asks a question that is not monotone, "is this node
//! still pending?", and files a naming into a text store the sweep deletes from, so it is made
//! under the memory's hold ([`Held`]), the one the sweep asks under.

use prodrome_core::event::{Actor, Hash};
use prodrome_core::literal::Datetime;

use crate::model_name::ModelName;
use crate::refusal::Refusal;
use crate::secretary::compact_ask::Beside;
use crate::secretary::conversations::{Conversations, ReadConversations};
use crate::secretary::digest::{self, Digest};
use crate::secretary::digest_event::{self, Attempt, DigestEvent};
use crate::secretary::digest_line::Line;
use crate::secretary::line_event::LineEvent;
use crate::secretary::memories::Held;
use crate::secretary::salted_text::{Text, TextName};
use crate::secretary::texts::Naming;
use crate::store::{Doc, Gets, Prodrome, Reads};

/// Who every attempt is written as: the compactor, whose word binds about summaries alone.
pub const COMPACTOR: &str = "compactor";

/// [`COMPACTOR`] as an actor.
///
/// # Panics
/// Never: the constant is an actor's name.
#[must_use]
pub fn compactor() -> Actor {
    Actor::new(COMPACTOR).expect("a constant actor name")
}

/// The digest: its history, its words and the lines it summarises.
#[derive(Clone, Copy)]
pub struct Digests<'a> {
    /// What was said of each node, by the node's name; never a summary's words.
    pub history: &'a dyn Prodrome<DigestEvent>,
    /// Each summary's words, by the name its event gives them.
    pub texts: &'a dyn Doc<TextName, Text<Line>>,
    /// The conversations, every line a leaf.
    pub lines: Conversations<'a>,
    /// Whose retraction or erasure is believed: the deployment's writers
    /// ([`crate::deployment::Deployment::writers`]).
    pub owners: &'a std::collections::BTreeSet<Actor>,
    /// Whether the compactor runs and within what, so a reading can say what holds it back.
    pub compacting: &'a crate::secretary::compacting::Compacting,
}

/// The digest as a reader holds it: no field reaches a write of an event, a summary's words or
/// a line.
#[derive(Clone, Copy)]
pub struct ReadDigests<'a> {
    /// What was said of each node, by the node's name.
    pub history: &'a dyn Reads<DigestEvent>,
    /// Each summary's words, by the name its event gives them.
    pub texts: &'a dyn Gets<TextName, Text<Line>>,
    /// The conversations, every line a leaf.
    pub lines: ReadConversations<'a>,
    /// Whose retraction or erasure is believed.
    pub owners: &'a std::collections::BTreeSet<Actor>,
    /// Whether the compactor runs and within what.
    pub compacting: &'a crate::secretary::compacting::Compacting,
}

impl<'a> Digests<'a> {
    /// The stores as a reader holds them.
    #[must_use]
    pub fn reads(&self) -> ReadDigests<'a> {
        ReadDigests {
            history: self.history,
            texts: self.texts,
            lines: self.lines.reads(),
            owners: self.owners,
            compacting: self.compacting,
        }
    }
}

impl ReadDigests<'_> {
    /// The reading.
    ///
    /// # Errors
    /// The stores' own; [`Refusal::internal`] for lines in a cycle, which no store can hold.
    pub fn read(&self) -> Result<Digest, Refusal> {
        let lines = self.lines.lines.objects()?;
        self.read_over(&lines, &self.opened(&lines))
    }

    /// Each of `lines` as read, by name: its words opened by the conversations' keys.
    #[must_use]
    pub fn opened(
        &self,
        lines: &[(Hash, prodrome_core::event::Envelope<LineEvent>)],
    ) -> std::collections::BTreeMap<Hash, crate::secretary::store::Entry> {
        crate::secretary::transcript::opened(lines, &self.lines.slots())
    }

    /// The reading over `lines`, the conversations' objects as already read, and `opened`, each
    /// of them read.
    ///
    /// # Errors
    /// As [`ReadDigests::read`].
    pub fn read_over(
        &self,
        lines: &[(Hash, prodrome_core::event::Envelope<LineEvent>)],
        opened: &std::collections::BTreeMap<Hash, crate::secretary::store::Entry>,
    ) -> Result<Digest, Refusal> {
        digest::digest(lines, opened, &self.history.nodes()?, self.owners, |name| {
            self.words(name)
        })
        .map_err(|cycle| {
            Refusal::internal(format!(
                "the conversations' lines rest on {} in a cycle",
                cycle.0.as_str()
            ))
        })
    }

    /// The texts the history names, each live or past ([`Digest::naming`]).
    ///
    /// # Errors
    /// As [`ReadDigests::read`].
    pub fn naming(&self) -> Result<Naming<Line>, Refusal> {
        Ok(self.read()?.naming())
    }

    /// Refuse unless the history and the lines both read whole, as every write that decides over
    /// the reading must.
    ///
    /// # Errors
    /// As [`crate::secretary::memories`]'s sweep: naming the store and what it left out.
    pub fn whole(&self) -> Result<(), Refusal> {
        crate::secretary::memories::whole(self.history, "digests")?;
        crate::secretary::memories::whole(self.lines.lines, "conversations")
    }

    /// The words kept under `name`; `None` once deleted or lost.
    #[must_use]
    pub fn words(&self, name: &TextName) -> Option<Line> {
        self.texts
            .get(name)
            .ok()
            .flatten()
            .map(|text| text.words().clone())
    }
}

impl Held<'_> {
    /// Record `line` as the compactor's summary of `node`, written by `model` from `over`: its
    /// words kept first, then the event naming them, and the words removed again if the event is
    /// refused. The event's taint, and the class whose asking it is recorded under, are the
    /// node's, read off the reading here and never an argument, so no caller can label a summary
    /// cleaner than its lines.
    ///
    /// # Errors
    /// [`Refusal::conflict`] for a node that is not pending (holding a current summary, holding
    /// the last line, or not in the tree); [`ReadDigests::whole`]'s; the salt's and the stores' own.
    pub fn summarize(
        &self,
        node: &Hash,
        line: &Line,
        over: Vec<Hash>,
        model: ModelName,
        at: Datetime,
    ) -> Result<Hash, Refusal> {
        let label = self.pending(node)?;
        let text = Text::new(line.clone(), self.salt()?);
        let name = text.name();
        let event = digest_event::summarized(
            node.clone(),
            at,
            compactor(),
            &text,
            Attempt {
                over,
                model,
                taint: label.taint,
                beside: Beside::of(&label),
            },
        )
        .map_err(|refused| Refusal::unprocessable(refused.to_string()))?;
        let digests = self.digests();
        digests.texts.set(&name, &text)?;
        match digests.history.append(event) {
            Ok(object) => Ok(object),
            Err(refused) => {
                // Its words never stood, so they are not kept.
                let _ = digests.texts.remove(&name);
                Err(refused.into())
            }
        }
    }

    /// Record that `model` found no line for `node`, its shortest answer `shortest` bytes: the
    /// node stays unsummarized, and the history says why.
    ///
    /// # Errors
    /// As [`Held::summarize`].
    pub fn unfit(
        &self,
        node: &Hash,
        model: ModelName,
        shortest: u32,
        at: Datetime,
    ) -> Result<Hash, Refusal> {
        self.pending(node)?;
        let event = digest_event::unfit(node.clone(), at, compactor(), model, shortest)
            .map_err(|refused| Refusal::internal(refused.to_string()))?;
        Ok(self.digests().history.append(event)?)
    }

    /// Refuse unless every store reads whole and `node` is waiting for a summary; its label
    /// when it is.
    fn pending(&self, node: &Hash) -> Result<crate::secretary::digest_label::Label, Refusal> {
        let digests = self.digests().reads();
        digests.whole()?;
        let read = digests.read()?;
        let waiting = || Refusal::conflict(format!("node {node} is not waiting for a summary"));
        if !read.pending().contains(node) {
            return Err(waiting());
        }
        let tree = read.tree().ok_or_else(waiting)?;
        Ok(*tree.node(node).ok_or_else(waiting)?.measure())
    }
}
