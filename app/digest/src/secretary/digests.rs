//! The digest's stores as one value: its history, a prodrome at the [`DigestEvent`] schema that
//! names each summary's words, its texts, and the conversations whose lines are the tree's leaves.
//!
//! Reading takes no lock; writing a summary asks "is this node still pending?" and files a naming
//! into a text store the sweep deletes from, so it is made under the memory's hold ([`Held`]), the
//! one the sweep asks under.

use crate::secretary::compacting::Compacting;
use prodrome_data::event::{Actor, Hash};
use prodrome_data::literal::Datetime;

use crate::model_name::ModelName;
use crate::secretary::compact_ask::Beside;
use crate::secretary::digest::{self, Digest};
use crate::secretary::digest_event::{self, Attempt, DigestEvent};
use crate::secretary::digest_line::Line;
use crate::secretary::memories::Held;
use suzatary_base::refusal::Refusal;
use suzatary_lines::parts;
use suzatary_lines::secretary::conversations::{Conversations, ReadConversations};
use suzatary_lines::secretary::held_lines::{Gathered, HeldLines};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::timings::{Phase, timed};
use suzatary_memory::secretary::salted_text::{Text, TextName};
use suzatary_memory::secretary::texts::Naming;
use suzatary_store::store::{Gets, Lists, Prodrome, Reads};

/// Who every attempt is written as; its word binds about summaries alone.
pub(crate) const COMPACTOR: &str = "compactor";

/// # Panics
/// Never: the constant is an actor's name.
#[must_use]
pub fn compactor() -> Actor {
    Actor::new(COMPACTOR).expect("a constant actor name")
}

#[derive(Clone, Copy)]
pub struct Digests<'a> {
    /// What was said of each node, by the node's name; never a summary's words.
    pub history: &'a dyn Prodrome<DigestEvent>,
    pub texts: &'a dyn Lists<TextName, Text<Line>>,
    pub lines: Conversations<'a>,
    /// Whose retraction or erasure is believed: the deployment's writers.
    pub owners: &'a std::collections::BTreeSet<Actor>,
    pub compacting: &'a Compacting,
}

/// The digest as a reader holds it: no field reaches a write.
#[derive(Clone, Copy)]
pub struct ReadDigests<'a> {
    pub history: &'a dyn Reads<DigestEvent>,
    pub texts: &'a dyn Gets<TextName, Text<Line>>,
    pub lines: ReadConversations<'a>,
    /// Whose retraction or erasure is believed.
    pub owners: &'a std::collections::BTreeSet<Actor>,
    pub compacting: &'a Compacting,
}

impl<'a> Digests<'a> {
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
    pub fn read(&self) -> Result<Digest, Refusal> {
        self.read_under(&*self.lines.fold(HeldLines::gathered)?)
    }

    /// The conversations' lines, shared by every reader at the same heads.
    pub(crate) fn line_graph(
        &self,
    ) -> Result<std::sync::Arc<suzatary_store::store::Graph<LineEvent>>, Refusal> {
        Ok(timed(Phase::Lines, || self.lines.lines.graph())?)
    }

    pub(crate) fn read_under(&self, gathered: &Gathered) -> Result<Digest, Refusal> {
        let shape = parts::part("shape", || digest::Shape::over(gathered)).map_err(|cycle| {
            Refusal::internal(format!(
                "the conversations' lines rest on {} in a cycle",
                cycle.0.as_str()
            ))
        })?;
        let history = timed(Phase::History, || self.history.nodes())?;
        let unread = std::cell::RefCell::new(None);
        let read = timed(Phase::Fold, || {
            self.lines.opened.said(|kept| {
                let read = digest::under_kept(
                    std::sync::Arc::clone(&shape),
                    &history,
                    self.owners,
                    |name| {
                        self.words(name).unwrap_or_else(|failed| {
                            unread.borrow_mut().get_or_insert(failed);
                            None
                        })
                    },
                    kept,
                );
                // Words that did not read are kept by no entry.
                if unread.borrow().is_some() {
                    kept.clear();
                }
                read
            })
        });
        match unread.into_inner() {
            Some(failed) => Err(failed),
            None => Ok(read),
        }
    }

    /// Every retraction an owner wrote, by object name, without reading the lines.
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
                        prodrome_dag::policy::Policy::standing(&policy, object.name(), event),
                        prodrome_data::policy::Standing::Binds
                    ))
                .then(|| object.name().clone())
            })
            .collect())
    }

    pub fn naming(&self) -> Result<Naming<Line>, Refusal> {
        let read = self.read()?;
        Ok(parts::part("naming", || read.naming()))
    }

    /// Refuse unless the history and the lines both read whole.
    pub fn whole(&self) -> Result<(), Refusal> {
        timed(Phase::Whole, || {
            crate::secretary::memories::whole(self.history, "digests")?;
            crate::secretary::memories::whole(self.lines.lines, "conversations")
        })
    }

    /// The words kept under `name`; `None` once deleted.
    pub fn words(&self, name: &TextName) -> Result<Option<Line>, Refusal> {
        Ok(self.texts.get(name)?.map(|text| text.words().clone()))
    }
}

impl Held<'_> {
    /// Record `line` as the compactor's summary of `node`: its words kept first, then the event
    /// naming them, and the words removed again if the event is refused. The event's taint is the
    /// node's, never an argument, so no caller can label a summary cleaner than its lines.
    ///
    /// # Errors
    /// [`Refusal::conflict`] for a node that is not pending (holding a current summary, holding
    /// the last line, or not in the tree).
    pub fn summarize(
        &self,
        node: &Hash,
        line: &Line,
        over: Vec<Hash>,
        model: ModelName,
        at: Datetime,
    ) -> Result<Hash, Refusal> {
        let (label, shown) = self.pending(node)?;
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
                taint: shown,
                beside: Beside::of(&label),
            },
        )
        .map_err(|refused| Refusal::unprocessable(refused.to_string()))?;
        let digests = self.digests();
        digests.texts.set(&name, &text)?;
        match digests.history.append(event) {
            Ok(object) => Ok(object),
            Err(refused) => {
                let _ = digests.texts.remove(&name);
                Err(refused.into())
            }
        }
    }

    /// Record that `model` found no line for `node`, its shortest answer `shortest` bytes.
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

    /// Retract the summary `node` shows, as the owner who tapped `tap`; the sweep after the write
    /// deletes the retracted words.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for a node that shows no summary; [`ReadDigests::whole`]'s.
    pub fn retract(
        &self,
        node: &Hash,
        at: Datetime,
        tap: &suzatary_signatures::signatures::Tap,
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

    /// Refuse unless every store reads whole and `node` is waiting for a summary; its label, and
    /// what its asking shows in words carries ([`Digest::asked`]), when it is.
    fn pending(
        &self,
        node: &Hash,
    ) -> Result<
        (
            crate::secretary::digest_label::Label,
            suzatary_conversations::secretary::taint::Taint,
        ),
        Refusal,
    > {
        let digests = self.digests().reads();
        digests.whole()?;
        let read = digests.read()?;
        let waiting = || Refusal::conflict(format!("node {node} is not waiting for a summary"));
        if !read.pending().contains(node) {
            return Err(waiting());
        }
        let label = *read.node(node).ok_or_else(waiting)?.measure();
        Ok((label, read.asked(node).ok_or_else(waiting)?))
    }
}
