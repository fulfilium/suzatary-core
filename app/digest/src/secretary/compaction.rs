//! The compactor's rule: a node waiting for its summary is owed one ("pending non-empty ⇒
//! compact the most pending node, append its outcome"), discharged by the outcome it records.
//!
//! The model call is made outside the memory's hold, so a slow model never keeps the owner from
//! remembering; the write is made under it, which asks again whether the node still waits.
//! Every step refuses while a store's read left anything out.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::{Arc, Mutex, PoisonError};

use prodrome_data::event::{Envelope, Hash};
use prodrome_data::instant::instant_of;
use prodrome_data::literal::Datetime;
use prodrome_digest::Node;

use crate::model_name::ModelName;
use crate::secretary::compact::Compact;
use crate::secretary::compact_ask::Ask;
use crate::secretary::compact_budget::{Budget, Hold};
use crate::secretary::compacting::Compacting;
use crate::secretary::compactor::{self, Outcome};
use crate::secretary::digest::{self, Digest};
use crate::secretary::digest_event::{self, DigestEvent};
use crate::secretary::digest_line::NODE_BYTES;
use crate::secretary::digest_spent::Spent;
use crate::secretary::digest_view::VIEW_BYTES;
use crate::secretary::digests::{Digests, ReadDigests, compactor};
use crate::secretary::memories::Memories;
use suzatary_base::capability::Clock;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::nonempty::NonEmpty;
use suzatary_conversations::secretary::store::Entry;
use suzatary_lines::secretary::held_lines;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_store::store::Prodrome;
use suzatary_triage::rule::{Named, ReadingName, Rule};

/// The one digest the box keeps, as a rule's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TheDigest;

impl std::fmt::Display for TheDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the digest")
    }
}

/// What the rule reads: the digest, and each line it is of, read whole.
#[derive(Debug, Clone)]
pub struct Reading {
    digest: Digest,
    lines: std::sync::Arc<suzatary_store::store::Graph<LineEvent>>,
    gathered: std::sync::Arc<held_lines::Gathered>,
}

impl Reading {
    pub fn of(digests: &ReadDigests<'_>) -> Result<Self, Refusal> {
        suzatary_lines::first_read::first("digest", || {
            digests.whole()?;
            let graph = digests.line_graph()?;
            let gathered = digests.lines.fold(held_lines::HeldLines::gathered)?;
            Ok(Self {
                digest: digests.read_under(&gathered)?,
                lines: graph,
                gathered,
            })
        })
    }

    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The lines the digest is of, in the store's order.
    pub fn lines(&self) -> impl Iterator<Item = (&Hash, &Envelope<LineEvent>)> {
        self.lines.objects()
    }

    #[must_use]
    pub fn entries(&self) -> std::collections::BTreeMap<&Hash, &Entry> {
        self.gathered.entries().iter().collect()
    }

    /// The question for `node` and what its summary would be written from.
    ///
    /// # Errors
    /// [`Refusal::internal`] for a node whose children do not read, which a pending node's do.
    fn ask(&self, node: &Hash) -> Result<(Ask, Vec<Hash>), Refusal> {
        Ask::of(&self.digest, &self.entries(), node).ok_or_else(|| {
            Refusal::internal(format!(
                "node {node} is pending and its children do not read"
            ))
        })
    }

    /// What `rule`'s next call would spend; `None` when nothing is owed.
    fn next_call<R: Rung>(&self, rule: &Compaction<R>) -> Result<Option<Spent>, Refusal> {
        rule.due(&self.digest)
            .map(|(node, _)| Ok(Spent::call(self.ask(&node)?.0.bytes())))
            .transpose()
    }
}

impl Named for Reading {
    fn name(&self) -> ReadingName {
        self.digest.name()
    }
}

/// One model of the compactor's ladder: its name, or the compactor that runs it.
pub trait Rung {
    fn model(&self) -> &ModelName;
}

impl Rung for ModelName {
    fn model(&self) -> &ModelName {
        self
    }
}

impl Rung for &dyn Compact {
    fn model(&self) -> &ModelName {
        Compact::model(*self)
    }
}

/// Whether a node lies under the view a reader is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    /// Within a part of the view that is a summary or waits for one.
    InView,
    Elsewhere,
}

/// The spans of the view's parts under [`VIEW_BYTES`] that a summary stands for or that wait for
/// one: the nodes within them are those whose summaries a reader's view rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InView(Vec<Range<usize>>);

impl InView {
    #[must_use]
    pub fn of(digest: &Digest) -> Self {
        let Some(view) = digest.view(u64::try_from(VIEW_BYTES).unwrap_or(u64::MAX)) else {
            return Self(Vec::new());
        };
        let waiting = view
            .waiting
            .iter()
            .filter_map(|name| digest.node(name).map(Node::span));
        Self(
            view.parts
                .into_iter()
                .map(|part| part.span)
                .filter(|span| span.len() > 1)
                .chain(waiting)
                .collect(),
        )
    }

    #[must_use]
    pub fn reach(&self, span: &Range<usize>) -> Reach {
        if self
            .0
            .iter()
            .any(|shown| shown.start <= span.start && span.end <= shown.end)
        {
            Reach::InView
        } else {
            Reach::Elsewhere
        }
    }
}

/// Compact with the models of `ladder`, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compaction<R = ModelName> {
    /// The models, the one tried first first.
    pub ladder: NonEmpty<R>,
}

impl<R: Rung> Compaction<R> {
    /// The first model of the ladder that has not found `node` unfit; `None` once every one has.
    fn rung(&self, digest: &Digest, node: &Hash) -> Option<&R> {
        self.ladder
            .as_slice()
            .iter()
            .find(|rung| !digest.found_unfit(node, rung.model()))
    }

    /// Every node owed a summary, with the model it is owed to: first those under the view a
    /// reader is shown (their summaries are what a reader reads, or will once written), then the
    /// rest. Within each, the nodes labelled the owner's alone come first, since a stale one of
    /// those is shown to no reader until it is asked again; each part keeps `pending`'s order.
    #[must_use]
    pub fn owed(&self, digest: &Digest) -> Vec<(Hash, &R)> {
        let view = InView::of(digest);
        let mut owed: Vec<(Hash, &R)> = digest
            .pending()
            .into_iter()
            .filter_map(|node| {
                let rung = self.rung(digest, &node)?;
                Some((node, rung))
            })
            .collect();
        owed.sort_by_cached_key(|(node, _)| {
            digest
                .node(node)
                .map(|node| (view.reach(&node.span()), !node.measure().is_owners()))
        });
        owed
    }

    #[must_use]
    pub fn due(&self, digest: &Digest) -> Option<(Hash, &R)> {
        self.owed(digest).into_iter().next()
    }

    /// The pending nodes every model of the ladder found unfit: nothing above them is summarised.
    #[must_use]
    pub fn unfit(&self, digest: &Digest) -> Vec<Hash> {
        digest
            .pending()
            .into_iter()
            .filter(|node| self.rung(digest, node).is_none())
            .collect()
    }
}

impl<R: Rung> Rule for Compaction<R> {
    type Key = TheDigest;
    type Reading = Reading;
    type Witness = ();

    fn name(&self) -> &'static str {
        "compact"
    }

    fn holds(&self, reading: &Reading) -> bool {
        self.due(&reading.digest).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compacted {
    Summarized {
        node: Hash,
        object: Hash,
    },
    /// No line fitted `node`; recorded as `object`.
    Unfit {
        node: Hash,
        /// Its event.
        object: Hash,
    },
}

/// Perform `rule` on `reading`: ask the most pending node's model for its line and record the
/// outcome. `None` when nothing is owed.
///
/// # Errors
/// The budget's hold, as a [`Refusal::conflict`]; [`Refusal::unprocessable`] for a compactor
/// that did not answer; a [`Refusal::conflict`] where the node no longer waits.
pub fn perform(
    memories: &Memories<'_>,
    rule: &Compaction<&dyn Compact>,
    reading: &Reading,
    budget: Budget,
    clock: &dyn Clock,
) -> Result<Option<Compacted>, Refusal> {
    let Some((node, compact)) = rule.due(&reading.digest) else {
        return Ok(None);
    };
    let (ask, over) = reading.ask(&node)?;
    attempt(memories, *compact, &node, ask, over, budget, clock).map(Some)
}

/// Ask `compact` for `node`'s line and record the outcome; a question no day's cap admits is
/// recorded unfit, asking nothing. Every call is appended as an `Asked` before it is made, under
/// the digest history's hold, so calls made at once by several workers count as one budget.
/// Every instant is read from the clock each time, since five tries of a slow model span minutes.
fn attempt(
    memories: &Memories<'_>,
    compact: &dyn Compact,
    node: &Hash,
    ask: Ask,
    over: Vec<Hash>,
    budget: Budget,
    clock: &dyn Clock,
) -> Result<Compacted, Refusal> {
    let model = compact.model().clone();
    let node = node.clone();
    if !budget.admits(Spent::call(ask.bytes())) {
        return Ok(Compacted::Unfit {
            object: memories.hold()?.unfit(&node, model, 0, clock.now())?,
            node,
        });
    }
    let history = memories.digests.history;
    let outcome = compactor::compact(compact, ask, &mut |sent| {
        let _held = history.locked()?;
        let now = clock.now();
        if let Some(hold) = budget.hold(&spent(history)?, Spent::call(sent), instant_of(now)) {
            return Err(Refusal::conflict(hold.to_string()));
        }
        let asked = digest_event::asked(node.clone(), now, compactor(), sent)
            .map_err(|refused| Refusal::internal(refused.to_string()))?;
        history.append(asked)?;
        Ok(())
    })?;
    let now = clock.now();
    let held = memories.hold()?;
    Ok(match outcome {
        Outcome::Fits { line } => Compacted::Summarized {
            object: held.summarize(&node, &line, over, model, now)?,
            node,
        },
        Outcome::Unfit { shortest } => Compacted::Unfit {
            object: held.unfit(&node, model, shortest, now)?,
            node,
        },
    })
}

fn spent(history: &dyn Prodrome<DigestEvent>) -> Result<Vec<(Datetime, Spent)>, Refusal> {
    Ok(digest::attempts(&history.nodes()?)
        .into_iter()
        .map(|(_, at, spent)| (at, spent))
        .collect())
}

/// The nodes some worker holds a [`Claim`] on.
type Taken = Arc<Mutex<BTreeSet<Hash>>>;

/// The rule at the budget's pace, shared by every worker the box runs, so no two workers ever
/// compact one node.
pub struct Paced<'c> {
    rule: Compaction<&'c dyn Compact>,
    budget: Budget,
    taken: Taken,
}

#[derive(Debug)]
pub enum Round<'c> {
    /// Nodes to compact, each held by its claim until the claim is dropped.
    Taken(Vec<Claim<'c>>),
    /// Work is owed and the budget holds its next call back until [`Hold::until`].
    Held(Hold),
    /// Nothing is owed that no worker holds.
    Idle,
}

/// One node taken for one worker; no other claim on the node exists while this one does.
pub struct Claim<'c> {
    node: Hash,
    compact: &'c dyn Compact,
    ask: Ask,
    over: Vec<Hash>,
    budget: Budget,
    taken: Taken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Performed {
    Compacted(Compacted),
    /// A cap was reached partway through the attempt, until [`Hold::until`].
    Held(Hold),
}

impl<'c> Paced<'c> {
    #[must_use]
    pub fn new(rule: Compaction<&'c dyn Compact>, budget: Budget) -> Self {
        Self {
            rule,
            budget,
            taken: Taken::default(),
        }
    }

    /// One round: take at most `free` of the nodes owed that no claim holds, most pending first,
    /// stopping at the first whose call the budget holds back. A claim is let go under the
    /// claims' lock only after its write, so a round either sees a node as claimed or reads
    /// what its claim wrote.
    pub fn take(
        &self,
        memories: &Memories<'_>,
        free: usize,
        clock: &dyn Clock,
    ) -> Result<Round<'c>, Refusal> {
        let mut taken = self.taken.lock().unwrap_or_else(PoisonError::into_inner);
        let reading = Reading::of(&memories.digests.reads())?;
        let now = clock.now();
        let mut spent: Vec<_> = reading.digest.attempts().collect();
        let mut chosen = Vec::new();
        let owed = self.rule.owed(&reading.digest);
        for (node, compact) in owed.into_iter().filter(|(node, _)| !taken.contains(node)) {
            if chosen.len() == free {
                break;
            }
            let (ask, over) = reading.ask(&node)?;
            let first = Spent::call(ask.bytes());
            if self.budget.admits(first) {
                if let Some(hold) = self.budget.hold(&spent, first, instant_of(now)) {
                    if chosen.is_empty() {
                        return Ok(Round::Held(hold));
                    }
                    break;
                }
                spent.push((now, first));
            }
            chosen.push((node, *compact, ask, over));
        }
        if chosen.is_empty() {
            return Ok(Round::Idle);
        }
        // Claims are made only here, past every refusal: one dropped while `taken` is locked
        // would wait on the lock it is dropped under.
        Ok(Round::Taken(
            chosen
                .into_iter()
                .map(|(node, compact, ask, over)| {
                    taken.insert(node.clone());
                    Claim {
                        node,
                        compact,
                        ask,
                        over,
                        budget: self.budget,
                        taken: Arc::clone(&self.taken),
                    }
                })
                .collect(),
        ))
    }
}

impl<'c> Claim<'c> {
    /// Compact the node, once, since the claim is spent by it. A refusal while the budget holds
    /// the node's first call is that hold, not a failure.
    ///
    /// # Errors
    /// [`perform`]'s refusals, as a [`Failed`] that keeps the node until it is dropped.
    pub fn perform(
        self,
        memories: &Memories<'_>,
        clock: &dyn Clock,
    ) -> Result<Performed, Failed<'c>> {
        let (ask, over) = (self.ask.clone(), self.over.clone());
        let attempted = attempt(
            memories,
            self.compact,
            &self.node,
            ask,
            over,
            self.budget,
            clock,
        );
        let refused = match attempted {
            Ok(done) => return Ok(Performed::Compacted(done)),
            Err(refused) => refused,
        };
        let first = Spent::call(self.ask.bytes());
        let holding = spent(memories.digests.history).map(|spent| {
            self.budget
                .hold(&spent, first, instant_of(clock.now()))
                .filter(|_| self.budget.admits(first))
        });
        match holding {
            Ok(Some(hold)) => Ok(Performed::Held(hold)),
            Ok(None) => Err(Failed {
                refused,
                _claim: Box::new(self),
            }),
            Err(unread) => Err(Failed {
                refused: unread,
                _claim: Box::new(self),
            }),
        }
    }
}

/// A claim's step that failed, still holding its node through the worker's backoff.
#[derive(Debug)]
pub struct Failed<'c> {
    refused: Refusal,
    _claim: Box<Claim<'c>>,
}

impl Failed<'_> {
    #[must_use]
    pub fn refusal(&self) -> &Refusal {
        &self.refused
    }
}

impl std::fmt::Debug for Claim<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Claim")
            .field("node", &self.node)
            .field("model", Compact::model(self.compact))
            .finish_non_exhaustive()
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.taken
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.node);
    }
}

/// Summaries owed that the compactor is not writing now, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    pub nodes: usize,
    pub why: Option<Why>,
    /// The waiting nodes every model of the ladder found no line for.
    pub unfit: Vec<Hash>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The kill switch.
    Off,
    /// A cap, until its instant.
    Held(Hold),
}

impl std::fmt::Display for Waiting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.why {
            Some(Why::Off) => write!(
                f,
                "the compactor is off, and {} summaries wait for it",
                self.nodes
            )?,
            Some(Why::Held(hold)) => {
                write!(f, "{hold}, and {} summaries wait for it", self.nodes)?;
            }
            None => write!(f, "{} summaries wait", self.nodes)?,
        }
        if !self.unfit.is_empty() {
            let named: Vec<&str> = self.unfit.iter().map(short).collect();
            write!(
                f,
                "; no model of the ladder found a line of {NODE_BYTES} bytes for {} ({}), so \
                 nothing above them is summarised until the instance file's `compactors` names \
                 another model",
                self.unfit.len(),
                named.join(", ")
            )?;
        }
        Ok(())
    }
}

/// What holds the compactor back at `now`; `None` when nothing waits.
pub fn waiting(digests: &Digests<'_>, now: Datetime) -> Result<Option<Waiting>, Refusal> {
    let reading = Reading::of(&digests.reads())?;
    let (owed, why, unfit) = match digests.compacting {
        Compacting::Off => (reading.digest.pending().len(), Some(Why::Off), Vec::new()),
        Compacting::On { ladder, budget, .. } => {
            let spent: Vec<_> = reading.digest.attempts().collect();
            let rule = Compaction {
                ladder: ladder.clone(),
            };
            let held = reading
                .next_call(&rule)?
                .filter(|next| budget.admits(*next))
                .and_then(|next| budget.hold(&spent, next, instant_of(now)));
            (
                rule.owed(&reading.digest).len(),
                held.map(Why::Held),
                rule.unfit(&reading.digest),
            )
        }
    };
    let waits = (why.is_some() && owed > 0) || !unfit.is_empty();
    Ok(waits.then_some(Waiting {
        nodes: owed,
        why: why.filter(|_| owed > 0),
        unfit,
    }))
}

/// A node's name as a line names it: its first twelve characters.
#[must_use]
pub fn short(node: &Hash) -> &str {
    &node.as_str()[..12]
}
