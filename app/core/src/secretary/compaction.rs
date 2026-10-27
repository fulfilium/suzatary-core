//! The compactor's rule: a node waiting for its summary is owed one ("pending non-empty ⇒
//! compact the most pending node, append its outcome"), discharged by the outcome it records.
//!
//! The most pending node is the first the Prodrome's `pending` offers (smallest, then
//! leftmost), the owner's nodes first, that this rule's model has not already found unfit. A
//! node whose summary is stale, asked under an older asking or over a child's summary since
//! replaced ([`crate::secretary::digest::Standing`]), is pending again once its children are
//! current, and its new summary is appended over the old one, which it replaces. The outcome is
//! an object of the digest's history, a summary or an `Unfit`, so it moves the reading and the
//! next round takes the next nodes. A node is pending only once its children's summaries exist, so several
//! workers build a level as its children finish, each on a node [`Paced`] has claimed for it
//! alone. An import of the conversations that already exist is no special path: it is this rule
//! over those lines, at the budget's pace.
//!
//! Every call is stamped at its own instant and the outcome at the instant it came, read from the
//! clock each time, since five tries of a slow model span minutes and a budget's windows fold the
//! instants. The model call is made outside the memory's hold, so a slow model never keeps the
//! owner from remembering; the write is made under it ([`crate::secretary::memories::Held::summarize`]),
//! which asks again whether the node still waits, and every step refuses while a store's read
//! left anything out.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use prodrome_core::event::{Envelope, Hash};
use prodrome_core::fpl::instant_of;
use prodrome_core::literal::Datetime;

use crate::capability::{Clock, Compact};
use crate::model_name::ModelName;
use crate::refusal::Refusal;
use crate::rule::{Named, ReadingName, Rule};
use crate::secretary::compact_ask::Ask;
use crate::secretary::compact_budget::{Budget, Hold};
use crate::secretary::compacting::Compacting;
use crate::secretary::compactor::{self, Outcome};
use crate::secretary::digest::{self, Digest, Written};
use crate::secretary::digest_event::{self, DigestEvent};
use crate::secretary::digest_line::NODE_BYTES;
use crate::secretary::digest_spent::Spent;
use crate::secretary::digests::{Digests, compactor};
use crate::secretary::line_event::LineEvent;
use crate::secretary::memories::Memories;
use crate::secretary::store::Entry;
use crate::store::Prodrome;

/// The one digest the box keeps, as a rule's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TheDigest;

impl std::fmt::Display for TheDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the digest")
    }
}

/// What the rule reads: the digest, and the lines it is of, read whole.
#[derive(Debug, Clone)]
pub struct Reading {
    digest: Digest,
    lines: Vec<(Hash, Envelope<LineEvent>)>,
    opened: std::collections::BTreeMap<Hash, Entry>,
}

impl Reading {
    /// The digest as `digests` holds it now.
    ///
    /// # Errors
    /// [`Digests::whole`]'s, and the stores' own.
    pub fn of(digests: &Digests<'_>) -> Result<Self, Refusal> {
        digests.whole()?;
        let lines = digests.lines.lines.objects()?;
        let opened = digests.opened(&lines);
        Ok(Self {
            digest: digests.read_over(&lines, &opened)?,
            lines,
            opened,
        })
    }

    /// The digest.
    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The lines the digest is of, in the store's order.
    #[must_use]
    pub fn lines(&self) -> &[(Hash, Envelope<LineEvent>)] {
        &self.lines
    }

    /// Each line as read, by its name.
    #[must_use]
    pub fn entries(&self) -> std::collections::BTreeMap<&Hash, &Entry> {
        self.opened.iter().collect()
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

    /// What `rule`'s next call would spend: the first question for its most pending node.
    /// `None` when nothing is owed.
    ///
    /// # Errors
    /// As [`Reading::ask`].
    fn next_call(&self, rule: &Compaction) -> Result<Option<Spent>, Refusal> {
        rule.due(&self.digest)
            .map(|node| Ok(Spent::call(self.ask(&node)?.0.bytes())))
            .transpose()
    }
}

/// Named by the digest's own name, which every attempt's outcome moves.
impl Named for Reading {
    fn name(&self) -> ReadingName {
        self.digest.name()
    }
}

/// Compact with `model`: the rule's own field, since what is owed is what that model has not
/// already found unfit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compaction {
    /// The model the compactor runs.
    pub model: ModelName,
}

impl Compaction {
    /// Every node owed a summary, most pending first: what `pending` offers, a stale summary
    /// read as none, less each this model found unfit at today's [`NODE_BYTES`] or above, so
    /// another model, or a larger limit, is owed it again. The nodes labelled the owner's alone
    /// come first, since a stale one of those is the summary a reader could take for the owner's
    /// words; each part keeps `pending`'s order.
    #[must_use]
    pub fn owed(&self, digest: &Digest) -> Vec<Hash> {
        let owners = |node: &Hash| {
            digest
                .tree()
                .and_then(|tree| tree.node(node))
                .is_some_and(|node| node.measure().is_owners())
        };
        let (mut first, rest): (Vec<Hash>, Vec<Hash>) = digest
            .pending()
            .into_iter()
            .filter(|node| {
                !digest.candidates(node).iter().any(|candidate| {
                    matches!(
                        &candidate.said,
                        Written::Unfit { model, limit, .. }
                            if model == &self.model
                                && usize::try_from(*limit).unwrap_or(usize::MAX) >= NODE_BYTES
                    )
                })
            })
            .partition(|node| owners(node));
        first.extend(rest);
        first
    }

    /// The most pending node.
    #[must_use]
    pub fn due(&self, digest: &Digest) -> Option<Hash> {
        self.owed(digest).into_iter().next()
    }
}

impl Rule for Compaction {
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

/// What one firing did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compacted {
    /// A summary of `node`, recorded as `object`.
    Summarized {
        /// The node.
        node: Hash,
        /// Its event.
        object: Hash,
    },
    /// No line fitted `node`; recorded as `object`.
    Unfit {
        /// The node.
        node: Hash,
        /// Its event.
        object: Hash,
    },
}

/// Perform the rule on `reading`: ask `compact` for the most pending node's line and record
/// the outcome at the instant it came, as [`Claim::perform`] does for a claimed node. `None`
/// when nothing is owed.
///
/// # Errors
/// The budget's hold, as a [`Refusal::conflict`]; [`Refusal::unprocessable`] for a compactor
/// that did not answer; the write's refusals (the node no longer waits, a store's read left
/// something out) and the stores' own.
pub fn perform(
    memories: &Memories<'_>,
    compact: &dyn Compact,
    reading: &Reading,
    budget: Budget,
    clock: &dyn Clock,
) -> Result<Option<Compacted>, Refusal> {
    let rule = Compaction {
        model: compact.model().clone(),
    };
    let Some(node) = rule.due(&reading.digest) else {
        return Ok(None);
    };
    let (ask, over) = reading.ask(&node)?;
    attempt(memories, compact, &node, ask, over, budget, clock).map(Some)
}

/// Ask `compact` for `node`'s line and record the outcome at the instant it came; a question
/// no day's cap admits is recorded unfit, asking nothing. Every call is appended as an `Asked`
/// stamped at its own instant before it is made, and refused once `budget` holds then, both
/// under the digest history's hold and against every call the history records, so calls made
/// at once by several workers are counted as one budget and none is made past a cap.
///
/// # Errors
/// As [`perform`].
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
        // No call is made: a question larger than a whole day's cap would wait forever.
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

/// Every call `history` records, stamped.
fn spent(history: &dyn Prodrome<DigestEvent>) -> Result<Vec<(Datetime, Spent)>, Refusal> {
    Ok(digest::attempts(&history.nodes()?)
        .into_iter()
        .map(|(_, at, spent)| (at, spent))
        .collect())
}

/// The nodes some worker holds a [`Claim`] on.
type Taken = Arc<Mutex<BTreeSet<Hash>>>;

/// The rule at the budget's pace, shared by every worker the box runs: [`Paced::take`] hands
/// out nodes no live [`Claim`] holds, read after every earlier claim was let go, so no two
/// workers ever compact one node. The box keeps one for as long as it runs; a restart's fresh
/// one takes nothing already recorded, since every outcome moves the reading.
#[derive(Debug)]
pub struct Paced {
    rule: Compaction,
    budget: Budget,
    taken: Taken,
}

/// What one round of [`Paced::take`] came to.
#[derive(Debug)]
pub enum Round {
    /// Nodes to compact, each held by its claim until the claim is dropped.
    Taken(Vec<Claim>),
    /// Work is owed and the budget holds its next call back until [`Hold::until`].
    Held(Hold),
    /// Nothing is owed that no worker holds.
    Idle,
}

/// One node taken for one worker, with the question its reading asks of it. No other claim on
/// the node exists while this one does. [`Claim::perform`] spends it and lets the node go
/// after its write, so whatever it wrote is in the reading the next round takes from.
#[derive(Debug)]
pub struct Claim {
    node: Hash,
    ask: Ask,
    over: Vec<Hash>,
    budget: Budget,
    taken: Taken,
}

/// What one claim's step came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Performed {
    /// The rule fired and recorded its outcome.
    Compacted(Compacted),
    /// A cap was reached partway through the attempt, until [`Hold::until`].
    Held(Hold),
}

impl Paced {
    /// The rule for `compact`'s model, held to `budget`.
    #[must_use]
    pub fn new(compact: &dyn Compact, budget: Budget) -> Self {
        Self {
            rule: Compaction {
                model: compact.model().clone(),
            },
            budget,
            taken: Taken::default(),
        }
    }

    /// One round at `clock`'s instant: read the digest whole and take at most `free` of the
    /// nodes owed that no claim holds, most pending first, stopping at the first whose call
    /// the budget holds back, counting the first calls of those already taken this round.
    /// The read is made under the claims' lock, and a claim is let go under that lock only
    /// after its write, so a round either sees a node as claimed or reads what its claim wrote.
    ///
    /// # Errors
    /// [`Reading::of`]'s.
    pub fn take(
        &self,
        memories: &Memories<'_>,
        free: usize,
        clock: &dyn Clock,
    ) -> Result<Round, Refusal> {
        let mut taken = self.taken.lock().unwrap_or_else(PoisonError::into_inner);
        let reading = Reading::of(&memories.digests)?;
        let now = clock.now();
        let mut spent: Vec<_> = reading.digest.attempts().collect();
        let mut chosen = Vec::new();
        let owed = self.rule.owed(&reading.digest);
        for node in owed.into_iter().filter(|node| !taken.contains(node)) {
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
            chosen.push((node, ask, over));
        }
        if chosen.is_empty() {
            return Ok(Round::Idle);
        }
        // Claims are made only here, past every refusal: one dropped while `taken` is locked
        // would wait on the lock it is dropped under.
        Ok(Round::Taken(
            chosen
                .into_iter()
                .map(|(node, ask, over)| {
                    taken.insert(node.clone());
                    Claim {
                        node,
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

impl Claim {
    /// Compact the node at `clock`'s instants, asking `compact`, as [`perform`] does, over the
    /// question the claim was taken with, once, since the claim is spent by it. A refusal while
    /// the budget holds the node's first call is that hold, not a failure.
    ///
    /// # Errors
    /// [`perform`]'s refusals, as a [`Failed`] that keeps the node until it is dropped.
    pub fn perform(
        self,
        memories: &Memories<'_>,
        compact: &dyn Compact,
        clock: &dyn Clock,
    ) -> Result<Performed, Failed> {
        let (ask, over) = (self.ask.clone(), self.over.clone());
        let refused = match attempt(memories, compact, &self.node, ask, over, self.budget, clock) {
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

/// A claim's step that failed, still holding its node, so a worker can wait out its backoff
/// without another taking the node meanwhile.
#[derive(Debug)]
pub struct Failed {
    refused: Refusal,
    _claim: Box<Claim>,
}

impl Failed {
    /// Why the step failed.
    #[must_use]
    pub fn refusal(&self) -> &Refusal {
        &self.refused
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.taken
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.node);
    }
}

/// Summaries owed that the compactor is not writing now, and why: what the briefing says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// How many nodes are owed one now.
    pub nodes: usize,
    /// What keeps them waiting, if anything does.
    pub why: Option<Why>,
    /// How many waiting nodes the compactor's model found no line for; every node above one
    /// waits for good, until another model or a larger limit is owed it.
    pub unfit: usize,
}

/// What keeps owed summaries waiting.
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
        if self.unfit > 0 {
            write!(
                f,
                "; {} found no line of {NODE_BYTES} bytes, so nothing above them is summarised \
                 until another model or a larger limit is tried",
                self.unfit
            )?;
        }
        Ok(())
    }
}

/// What holds the compactor back at `now`, as `digests` reads: a hold or the switch, and the
/// nodes its model found unfit; `None` when nothing waits on either.
///
/// # Errors
/// [`Reading::of`]'s.
pub fn waiting(digests: &Digests<'_>, now: Datetime) -> Result<Option<Waiting>, Refusal> {
    let reading = Reading::of(digests)?;
    let pending = reading.digest.pending().len();
    let (owed, why) = match digests.compacting {
        Compacting::Off => (pending, Some(Why::Off)),
        Compacting::On { model, budget, .. } => {
            let spent: Vec<_> = reading.digest.attempts().collect();
            let rule = Compaction {
                model: model.clone(),
            };
            let held = reading
                .next_call(&rule)?
                .filter(|next| budget.admits(*next))
                .and_then(|next| budget.hold(&spent, next, instant_of(now)));
            (rule.owed(&reading.digest).len(), held.map(Why::Held))
        }
    };
    let unfit = match digests.compacting {
        Compacting::Off => 0,
        Compacting::On { .. } => pending - owed,
    };
    let waits = (why.is_some() && owed > 0) || unfit > 0;
    Ok(waits.then_some(Waiting {
        nodes: owed,
        why: why.filter(|_| owed > 0),
        unfit,
    }))
}
