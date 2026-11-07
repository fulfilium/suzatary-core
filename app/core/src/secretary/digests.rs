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
use crate::timings::{Phase, timed};

/// Who every attempt is written as: the compactor, whose word binds about summaries alone.
pub(crate) const COMPACTOR: &str = "compactor";

/// `COMPACTOR` as an actor.
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

/// The digest as a reader holds it: no field reaches a write.
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
        let graph = self.line_graph()?;
        let lines: Vec<_> = graph.objects().collect();
        self.read_over(&lines, &self.opened(&lines))
    }

    /// The conversations' lines as the store's graph, shared by every reader at the same heads.
    ///
    /// # Errors
    /// The store's own.
    pub(crate) fn line_graph(
        &self,
    ) -> Result<std::sync::Arc<crate::store::Graph<LineEvent>>, Refusal> {
        Ok(timed(Phase::Lines, || self.lines.lines.graph())?)
    }

    /// Each of `lines` as read, by name: its words opened by the conversations' keys.
    #[must_use]
    pub fn opened(
        &self,
        lines: &[(&Hash, &prodrome_core::event::Envelope<LineEvent>)],
    ) -> std::collections::BTreeMap<Hash, crate::secretary::store::Entry> {
        timed(Phase::Opened, || {
            crate::secretary::transcript::opened(lines, &self.lines.slots())
        })
    }

    /// The reading over `lines` and `opened`, each of them read.
    ///
    /// # Errors
    /// As [`ReadDigests::read`].
    pub(crate) fn read_over(
        &self,
        lines: &[(&Hash, &prodrome_core::event::Envelope<LineEvent>)],
        opened: &std::collections::BTreeMap<Hash, crate::secretary::store::Entry>,
    ) -> Result<Digest, Refusal> {
        let history = timed(Phase::History, || self.history.nodes())?;
        let unread = std::cell::RefCell::new(None);
        let read = timed(Phase::Fold, || {
            digest::digest(lines, opened, &history, self.owners, |name| {
                self.words(name).unwrap_or_else(|failed| {
                    unread.borrow_mut().get_or_insert(failed);
                    None
                })
            })
        });
        if let Some(failed) = unread.into_inner() {
            return Err(failed);
        }
        read.map_err(|cycle| {
            Refusal::internal(format!(
                "the conversations' lines rest on {} in a cycle",
                cycle.0.as_str()
            ))
        })
    }

    /// Every retraction an owner wrote, by object name, without reading the lines.
    ///
    /// # Errors
    /// The history's store's own.
    pub fn retractions(&self) -> Result<std::collections::BTreeSet<Hash>, Refusal> {
        let policy = crate::secretary::digest_policy::Owners(self.owners);
        Ok(self
            .history
            .nodes()?
            .iter()
            .filter_map(|object| {
                let event = object.event()?;
                (matches!(event.body(), digest_event::Said::Retracted)
                    && matches!(
                        prodrome_core::policy::Policy::standing(&policy, object.name(), event),
                        prodrome_core::policy::Standing::Binds
                    ))
                .then(|| object.name().clone())
            })
            .collect())
    }

    /// The texts the history names, each live or past ([`Digest::naming`]).
    ///
    /// # Errors
    /// As [`ReadDigests::read`].
    pub fn naming(&self) -> Result<Naming<Line>, Refusal> {
        Ok(self.read()?.naming())
    }

    /// Refuse unless the history and the lines both read whole.
    ///
    /// # Errors
    /// As [`crate::secretary::memories`]'s sweep: naming the store and what it left out.
    pub fn whole(&self) -> Result<(), Refusal> {
        timed(Phase::Whole, || {
            crate::secretary::memories::whole(self.history, "digests")?;
            crate::secretary::memories::whole(self.lines.lines, "conversations")
        })
    }

    /// The words kept under `name`; `None` once deleted.
    ///
    /// # Errors
    /// The text store unreadable.
    pub fn words(&self, name: &TextName) -> Result<Option<Line>, Refusal> {
        Ok(self.texts.get(name)?.map(|text| text.words().clone()))
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

    /// Retract the summary `node` shows, as the owner who tapped `tap`: the node reads
    /// unsummarized and is pending again, every summary above it reads stale until its node is
    /// asked again, so the compactor redoes the spine, and the sweep after the write deletes the
    /// retracted words.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for a node that shows no summary; [`ReadDigests::whole`]'s; the
    /// stores' own.
    pub fn retract(
        &self,
        node: &Hash,
        at: Datetime,
        tap: &crate::secretary::tap::Tap,
    ) -> Result<Hash, Refusal> {
        let digests = self.digests();
        let reads = digests.reads();
        reads.whole()?;
        if reads.read()?.summary(node).is_none() {
            return Err(Refusal::not_found(format!(
                "node {node} shows no summary to retract"
            )));
        }
        let event = digest_event::retracted(node.clone(), at, tap)
            .map_err(|refused| Refusal::internal(refused.to_string()))?;
        let object = digests.history.append(event)?;
        self.sweep_after_write();
        Ok(object)
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
        Ok(*read.node(node).ok_or_else(waiting)?.measure())
    }
}
