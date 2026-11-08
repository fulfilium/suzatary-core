//! The lines a tab's core holds, whole: the rows of their skeleton as its pull moves them
//! ([`LinePulling`]), and what the landed row means, read through [`Outlines`] (its nodes) and
//! [`HeldLines`] (the prints, opened with its keys). Every move of the rows answers a
//! [`LineFrame`] and the conversations it moved, and the readings are told the landed row's
//! readable nodes and keys after each, so they read that row and nothing else. A print is content
//! held by name, kept whatever the rows name; a line every key of whose words is let go is kept
//! as its husk ([`LineReplica`]), and its print is the device's to delete.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::answer::Renderable;
use crate::conversation_id::ConversationId;
use crate::replica::held::Said;
use crate::secretary::held_lines::HeldLines;
use crate::secretary::key_ring::KeyRing;
use crate::secretary::line_form::{Husked, LineDelta, LineFrame, LinePulling, LineReplica};
use crate::secretary::locked_text::KeyName;
use crate::secretary::outlines::{NotANode, Outlines, Parsed, Scope};
use crate::wire::{Heads, SkeletonNode, SkeletonPayload};

/// The lines held: the rows, and the readings of the landed one.
#[derive(Default)]
pub struct HeldReplica {
    /// The prints and the keys of their words, the landed row's keys.
    pub lines: HeldLines,
    outlines: Outlines,
    kept: LineReplica,
}

/// Which conversations a move of the landed row moved: every one, where the row was reset or
/// heard a later epoch; else those with a line come or gone, or a key of their words given or
/// taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// Every conversation.
    All,
    /// These.
    These(BTreeSet<ConversationId>),
}

/// A move of the rows: the frame that makes it, and what it moved.
#[derive(Debug, Clone)]
pub struct Took {
    /// What another replica takes to make the same move.
    pub frame: LineFrame,
    /// What it moved.
    pub moved: Moved,
}

/// A skeleton's answer that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAnAnswer {
    /// A name that is no object's.
    #[error("the answer names {0:?}, which is no object's name")]
    Name(String),
    /// A node that does not read.
    #[error(transparent)]
    Node(#[from] NotANode),
}

impl HeldReplica {
    /// The skeleton's nodes the landed row reads.
    #[must_use]
    pub fn outlines(&self) -> &Outlines {
        &self.outlines
    }

    /// The skeleton's nodes beside the prints, each to read the other by.
    pub fn reading(&mut self) -> (&Outlines, &mut HeldLines) {
        (&self.outlines, &mut self.lines)
    }

    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &LinePulling {
        &self.kept.rows
    }

    /// The replica as the device keeps it: the rows and the husks.
    #[must_use]
    pub fn kept(&self) -> &LineReplica {
        &self.kept
    }

    /// The piece the box answered, `answer` heard at `fetched_at` with the keys `keys`, kept
    /// beside the landed row; it moves no reading. The answer's own `keys` are not read: `keys`
    /// is what the piece holds. Heads the answer leaves unsaid are those of what is held now.
    ///
    /// # Errors
    /// [`NotAnAnswer`] for a name or a node that does not read; nothing is kept.
    pub fn piece(
        &mut self,
        answer: SkeletonPayload,
        keys: KeyRing,
        fetched_at: u64,
    ) -> Result<LineDelta, NotAnAnswer> {
        let SkeletonPayload {
            heads,
            geneses,
            epoch,
            nodes,
            since,
            frontier,
            ..
        } = answer;
        let heads = match heads {
            Heads::Held => self.kept.rows.holding().replica().read().heads(),
            Heads::These { heads } => names(heads)?,
        };
        let said = Said {
            heads,
            geneses: names(geneses)?,
            epoch,
            nodes: nodes
                .into_iter()
                .map(Parsed::of)
                .collect::<Result<_, _>>()?,
            since,
            digest: name(frontier.digest)?,
            count: u64::try_from(frontier.count).unwrap_or(u64::MAX),
        };
        Ok(self.kept.rows.piece(said, keys, fetched_at))
    }

    /// The pending pieces landed.
    pub fn land(&mut self) -> Took {
        self.took(LinePulling::land)
    }

    /// Every row reset, in the generation after both: a later epoch overtook what is held.
    pub fn start_over(&mut self) -> Took {
        self.took(LinePulling::start_over)
    }

    /// The keys `gone` let go, from every row, and every line the rows read each key of whose
    /// words is then let go kept as its husk ([`HeldReplica::bare`]).
    pub fn forget(&mut self, gone: BTreeSet<KeyName>) -> Took {
        let (delta, moved) = self.moving(|rows| rows.forget(gone));
        let names = self.bare();
        let husked: Vec<Husked> = self
            .lines
            .husk(&names)
            .into_iter()
            .map(|(name, parents, husk)| Husked {
                name,
                parents,
                husk,
            })
            .collect();
        for husked in &husked {
            self.kept.husks.insert(husked.name.clone(), husked.clone());
        }
        Took {
            frame: LineFrame { delta, husked },
            moved,
        }
    }

    /// The name of every line the rows read, landed or in a pull's pieces, held as a print with
    /// words each key of which no row holds: a line every slot of which is erased, which a purge
    /// keeps as its husk. A line a slot of which still opens is kept whole.
    #[must_use]
    pub fn bare(&self) -> BTreeSet<Hash> {
        let holding = self.kept.rows.holding();
        let ring = holding.keys();
        let pending: BTreeSet<Hash> = self.pending().iter().map(|n| n.name().clone()).collect();
        self.lines
            .bare(|key| ring.holds(key))
            .into_iter()
            .filter(|name| self.outlines.holds(name) || pending.contains(name))
            .collect()
    }

    /// `frame` taken: another tab's, or one read back from the log.
    pub fn apply(&mut self, frame: &LineFrame) -> Moved {
        let moved = self.moving(|rows| rows.apply(&frame.delta)).1;
        self.husked(frame.husked.iter().cloned());
        moved
    }

    /// The replica replaced by `kept`, as a warm load reads it; the prints are kept, and each of
    /// its husks stands in place of its line's print.
    pub fn load(&mut self, kept: LineReplica) -> Moved {
        let LineReplica { rows, husks } = kept;
        let moved = self.moving(|held| *held = rows).1;
        self.kept.husks = BTreeMap::new();
        self.husked(husks.into_values());
        moved
    }

    fn husked(&mut self, husks: impl IntoIterator<Item = Husked>) {
        let husks: Vec<Husked> = husks.into_iter().collect();
        self.lines.hold_husks(
            husks
                .iter()
                .map(|h| (h.name.clone(), h.parents.clone(), h.husk.clone()))
                .collect(),
        );
        self.kept
            .husks
            .extend(husks.into_iter().map(|h| (h.name.clone(), h)));
    }

    fn took(&mut self, step: impl FnOnce(&mut LinePulling) -> LineDelta) -> Took {
        let (delta, moved) = self.moving(step);
        Took {
            frame: LineFrame {
                delta,
                husked: Vec::new(),
            },
            moved,
        }
    }

    /// `step` taken, and the readings told the landed row it leaves.
    fn moving<A>(&mut self, step: impl FnOnce(&mut LinePulling) -> A) -> (A, Moved) {
        let landed = self.kept.rows.landed();
        let was = (landed.generation(), landed.replica().epoch());
        let taken = step(&mut self.kept.rows);
        let landed = self.kept.rows.landed();
        let reading = landed.replica().read();
        let came: Vec<Parsed> = reading
            .nodes()
            .filter(|node| !self.outlines.holds(node.name()))
            .cloned()
            .collect();
        let went: Vec<Hash> = self
            .outlines
            .names()
            .filter(|name| !reading.holds(name))
            .cloned()
            .collect();
        let mut touched = if came.is_empty() && went.is_empty() {
            BTreeSet::new()
        } else {
            self.outlines.hold(came, &went)
        };
        let rekeyed = self.lines.rekey(landed.keys().clone());
        touched.extend(self.outlines.naming(&rekeyed));
        let moved = if (landed.generation(), landed.replica().epoch()) == was {
            Moved::These(touched)
        } else {
            Moved::All
        };
        (taken, moved)
    }
}

fn name(text: String) -> Result<Hash, NotAnAnswer> {
    Hash::new(text.clone()).map_err(|_| NotAnAnswer::Name(text))
}

fn names(texts: Vec<String>) -> Result<BTreeSet<Hash>, NotAnAnswer> {
    texts.into_iter().map(name).collect()
}
/// What a device keeps of the lines' content: the plan within the budget and every print held,
/// which a reading loaded; and what of the plan the prints held lack, which a prefetch fetches.
pub struct Plan<'r> {
    /// Every name whose content the device keeps.
    pub keep: BTreeSet<&'r Hash>,
    /// The nodes of the plan whose print is not held, newest conversation first.
    pub ahead: Vec<&'r SkeletonNode>,
}

impl HeldReplica {
    /// Every node `scope` reads, of the landed row and of a pull's pieces not landed yet, whose
    /// print is not held, as it was given.
    #[must_use]
    pub fn lacking(&self, scope: &Scope) -> Vec<SkeletonNode> {
        let pending = self.pending();
        let given: BTreeMap<&Hash, &SkeletonNode> = pending
            .iter()
            .map(|node| (node.name(), node.given()))
            .collect();
        self.outlines
            .scope(scope, &pending)
            .into_iter()
            .filter(|name| !self.lines.holds(name))
            .filter_map(|name| {
                self.outlines
                    .node(name)
                    .or_else(|| given.get(name).copied())
            })
            .cloned()
            .collect()
    }

    /// Every key held, landed or in a pull's pieces, that is dead: one the prints held name as
    /// erased, one the box's rule retires over the landed row and the pieces, or one in `told`
    /// (what the box's pull says an erasure names); each with the blocks its words fed where the
    /// keys held open them. What a purge forgets, the blocks first and then the keys.
    pub fn owed(&mut self, told: &BTreeSet<KeyName>) -> BTreeMap<KeyName, Vec<Renderable>> {
        let holding = self.kept.rows.holding().into_owned();
        let ring = holding.keys();
        let pending = self.pending();
        let mut dead = self.lines.dead();
        dead.extend(self.outlines.retired(&pending, &|key| ring.holds(key)));
        dead.extend(told.iter().cloned());
        let owed: BTreeSet<KeyName> = ring
            .iter()
            .map(|(name, _)| name)
            .filter(|name| dead.contains(*name))
            .cloned()
            .collect();
        let mut erased = self.lines.erased(ring, &owed);
        erased.retain(|name, _| owed.contains(name));
        for name in owed {
            erased.entry(name).or_default();
        }
        erased
    }

    /// The nodes a pull's pieces read that the landed row's readings do not hold.
    fn pending(&self) -> Vec<Parsed> {
        self.kept
            .rows
            .pending()
            .replica()
            .read()
            .nodes()
            .filter(|node| !self.outlines.holds(node.name()))
            .cloned()
            .collect()
    }

    /// The plan of the skeleton held within `budget`, beside the prints held.
    #[must_use]
    pub fn plan(&self, budget: usize) -> Plan<'_> {
        let planned = self.outlines.plan(budget);
        let ahead = planned
            .iter()
            .filter(|name| !self.lines.holds(name))
            .filter_map(|name| self.outlines.node(name))
            .collect();
        let held = self.lines.names().filter(|name| self.outlines.holds(name));
        Plan {
            keep: planned.into_iter().chain(held).collect(),
            ahead,
        }
    }
}
