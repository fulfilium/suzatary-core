//! The lines a tab's core holds, whole: the rows of their skeleton as its pull moves them
//! ([`LinePulling`]), and what the landed row means, read through [`Outlines`] (its nodes) and
//! [`HeldLines`] (the prints, opened with its keys). Every move of the rows answers a
//! [`LineFrame`] and the conversations it moved, and the readings are told the landed row's
//! readable nodes and keys after each, so they read that row and nothing else. A print is content
//! held by name, kept whatever the rows name; a line every key of whose words is let go is kept
//! as its husk ([`LineReplica`]), and its print is the device's to delete.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::secretary::held_lines::HeldLines;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_form::{Husked, LineDelta, LineFrame, LinePulling, LineReplica};
use crate::secretary::outlines::{NotANode, Outlines, Parsed, Scope};
use crate::secretary::own_line::OwnLine;
use crate::secretary::store::{Entry, Kind, Namer, Title};
use crate::wire::lines::{Heads, SkeletonNode, SkeletonPayload};
use prodrome_core::event::{Actor, Envelope};
use suzatary_base::answer::Renderable;
use suzatary_base::capability::{Datetime, Entropy};
use suzatary_base::conversation_id::ConversationId;
use suzatary_connect::connect::TemplateId;
use suzatary_memory::secretary::key_ring::KeyRing;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_memory::secretary::words::Words;
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::held::Said;
use suzatary_store::replica::row::Keys as _;
use suzatary_store::replica::writes::{Answer, Own, Writes};

/// The lines held: the rows, what this tab wrote, and the readings of the landed row with the
/// own lines it does not hold yet.
#[derive(Default)]
pub struct HeldReplica {
    /// The prints and the keys of their words, the landed row's keys and the own lines'.
    pub lines: HeldLines,
    outlines: Outlines,
    kept: LineReplica,
    own: Writes<OwnLine>,
}

/// A line a tab seals.
#[derive(Debug, Clone)]
pub enum Sealing {
    /// A conversation renamed.
    Rename {
        /// Which.
        conversation: ConversationId,
        /// Its new title.
        title: Title,
    },
    /// A topic opened under `title`; none leaves it for the title job.
    Open {
        /// Its title.
        title: Option<Title>,
    },
    /// A conversation set up from a template, titled by it.
    SetUp(TemplateId),
}

/// Why a tab cannot seal a line now.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotSealed {
    /// The rows name no genesis of the lines yet: nothing was pulled.
    #[error("the lines are not on this device yet, so nothing is written to them")]
    Unpulled,
    /// The conversation renamed is not one the tab reads.
    #[error("no conversation {0} on this device")]
    Unknown(ConversationId),
    /// The line itself.
    #[error(transparent)]
    Line(#[from] crate::secretary::line_event::NotALine),
    /// The change over it.
    #[error("no change of the lines: {0}")]
    Change(String),
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
    #[error("the answer names the key {0:?}, which is no key's name")]
    Key(String),
    /// A node that does not read.
    #[error(transparent)]
    Node(#[from] NotANode),
}

/// A husk the box answered in place of a line's print that this replica does not take.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotItsHusk {
    #[error("the box answered a husk for {0}, which names no line this replica reads")]
    Unknown(String),
    #[error("{0}: {1}")]
    Unread(String, suzatary_store::store::NotAHusk),
    #[error("the box's husk for {0} is not the line its node says")]
    Header(String),
    #[error("the box answered a husk for {0}, whose words a live key this replica holds opens")]
    Opens(String),
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

    /// What this tab wrote, and how the box answered each.
    #[must_use]
    pub fn own(&self) -> &Writes<OwnLine> {
        &self.own
    }

    /// `sealing` written by `actor` at `at`, its words sealed under keys drawn from `entropy`,
    /// resting on the heads of its conversation among the lines read: the own line, held and read
    /// at once, its name, and what it moved.
    ///
    /// # Errors
    /// [`NotSealed`]: nothing pulled yet, a rename of a conversation not read here, or words the
    /// door refuses.
    pub fn seal(
        &mut self,
        sealing: Sealing,
        at: Datetime,
        actor: &Actor,
        entropy: &dyn Entropy,
    ) -> Result<(Vec<Own<OwnLine>>, Hash, Moved), NotSealed> {
        let genesis = self
            .kept
            .rows
            .landed()
            .replica()
            .read()
            .word()
            .and_then(|word| word.geneses.first().cloned())
            .ok_or(NotSealed::Unpulled)?;
        let (line, keys, deps) = match sealing {
            Sealing::Rename {
                conversation,
                title,
            } => {
                let lines: Vec<&crate::wire::lines::SkeletonNode> = self
                    .outlines
                    .given()
                    .filter(|node| {
                        matches!(&node.header, crate::wire::lines::NodeHeader::Line {
                            conversation: of, ..
                        } if of == conversation.as_str())
                    })
                    .collect();
                if lines.is_empty() {
                    return Err(NotSealed::Unknown(conversation));
                }
                let under: BTreeSet<&str> = lines
                    .iter()
                    .flat_map(|node| node.parents.iter().map(String::as_str))
                    .collect();
                let deps: Vec<Hash> = lines
                    .iter()
                    .filter(|node| !under.contains(node.hash.as_str()))
                    .filter_map(|node| Hash::new(node.hash.clone()).ok())
                    .collect();
                let entry = Entry::Titled {
                    at,
                    title: Words::kept(title),
                    by: Namer::Person {
                        actor: actor.as_str().to_owned(),
                    },
                };
                let held = u32::try_from(lines.len()).unwrap_or(u32::MAX);
                let (line, keys) =
                    LineEvent::write(conversation, actor.clone(), held, &entry, entropy)?;
                (line, keys, deps)
            }
            Sealing::Open { title } => {
                let entry = Entry::Opened {
                    at,
                    title: Words::kept(title.map_or_else(String::new, String::from)),
                    model: String::new(),
                    kind: Kind::Topic,
                };
                let (line, keys) = LineEvent::begin(actor.clone(), &entry, entropy)?;
                (line, keys, Vec::new())
            }
            Sealing::SetUp(template) => {
                let entry = Entry::SetUp {
                    at,
                    title: Words::kept(template.template().called()),
                    template,
                };
                let (line, keys) = LineEvent::begin(actor.clone(), &entry, entropy)?;
                (line, keys, Vec::new())
            }
        };
        let change = prodrome_core::change::mk_change(genesis, deps, line)
            .map_err(|e| NotSealed::Change(e.to_string()))?;
        let envelope = Envelope::Change(change);
        let own = OwnLine::sealed(&envelope, &keys).unwrap_or_else(|_| {
            unreachable!("a line this tab sealed reads back with the keys it sealed it under")
        });
        let name = own.name().clone();
        self.lines
            .hold(vec![(name.clone(), own.print().as_bytes().to_vec())], &[]);
        let owned = self.own.write(vec![own]);
        let moved = self.moving(|_| ()).1;
        Ok((owned, name, moved))
    }

    /// `owned` taken, whoever made it: another tab's writes, or the device's own read back; each
    /// line's print held.
    pub fn take_own(&mut self, owned: &[Own<OwnLine>]) -> Moved {
        let prints: Vec<(Hash, Vec<u8>)> = owned
            .iter()
            .filter(|own| matches!(own.node, OwnLine::Line { .. }))
            .map(|own| {
                (
                    own.node.name().clone(),
                    own.node.print().as_bytes().to_vec(),
                )
            })
            .collect();
        self.lines.hold(prints, &[]);
        self.own.apply(owned);
        self.moving(|_| ()).1
    }

    /// A move of the own writes made by `step`: what it joined in, and what it moved.
    pub fn answer_own(
        &mut self,
        step: impl FnOnce(&mut Writes<OwnLine>) -> Vec<Own<OwnLine>>,
    ) -> (Vec<Own<OwnLine>>, Moved) {
        let owned = step(&mut self.own);
        let moved = self.moving(|_| ()).1;
        (owned, moved)
    }

    /// The own writes the box has not taken and may take, beside what is held, landed or not, in
    /// posts of at most `most`, each line beside its signatures.
    #[must_use]
    pub fn unsent(&self, most: usize) -> Vec<Vec<&OwnLine>> {
        let holding = self.kept.rows.holding();
        let reading = holding.replica().read();
        self.own
            .pieces(|name| reading.holds(name), OwnLine::role, most)
    }

    /// The own lines no signature written here covers.
    #[must_use]
    pub fn unsigned(&self) -> Vec<&OwnLine> {
        let holding = self.kept.rows.holding();
        let reading = holding.replica().read();
        self.own.unsigned(|name| reading.holds(name), OwnLine::role)
    }

    /// The box's answers to the own writes.
    pub fn answered(&mut self, answers: Vec<(Hash, Answer)>) -> (Vec<Own<OwnLine>>, Moved) {
        self.answer_own(|own| own.answered(answers))
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

    /// A whole pull of the skeleton landed, each piece `ask` answers from the frontier named
    /// (`""` first).
    pub fn pulled<E: From<NotAnAnswer>>(
        mut ask: impl FnMut(&str) -> Result<SkeletonPayload, E>,
    ) -> Result<Self, E> {
        let mut replica = Self::default();
        let mut since = String::new();
        loop {
            let piece = ask(&since)?;
            let (rest, frontier) = (piece.rest, piece.frontier.digest.clone());
            let keys = piece
                .keys
                .iter()
                .map(|(name, key)| {
                    Ok((
                        KeyName::new(name.clone()).map_err(|_| NotAnAnswer::Key(name.clone()))?,
                        key.clone(),
                    ))
                })
                .collect::<Result<_, NotAnAnswer>>()?;
            replica.piece(piece, KeyRing::of(&keys), 0)?;
            if rest == 0 {
                let _ = replica.land();
                return Ok(replica);
            }
            since = frontier;
        }
    }

    /// A content answer taken: each literal held, each husk taken in its place.
    pub fn take(&mut self, content: Vec<crate::wire::lines::Content>) -> Result<Took, NotItsHusk> {
        let (mut prints, mut husks) = (Vec::new(), Vec::new());
        for object in content {
            match object {
                crate::wire::lines::Content::Literal { hash, literal } => {
                    prints.push((
                        Hash::new(hash.clone()).map_err(|_| NotItsHusk::Unknown(hash))?,
                        literal.into_bytes(),
                    ));
                }
                crate::wire::lines::Content::Husk { hash, husk } => {
                    husks.push((
                        Hash::new(hash.clone()).map_err(|_| NotItsHusk::Unknown(hash))?,
                        husk,
                    ));
                }
            }
        }
        self.lines.hold(prints, &[]);
        self.take_husks(husks)
    }

    /// The husks the box answered, each kept where it is its node's line and no live key held
    /// opens it, where a purge would have husked the print; else none is kept.
    pub fn take_husks(&mut self, given: Vec<(Hash, String)>) -> Result<Took, NotItsHusk> {
        let pending = self.pending();
        let ring = self.kept.rows.holding().keys().clone();
        // A key still held may be dead already, retired by a line held or a reseal the rows
        // read, and waiting on its purge, which fetches the lines naming it first.
        let mut dead = self.lines.dead(&|key| ring.holds(key));
        dead.extend(self.outlines.retired(&pending, &|key| ring.holds(key)));
        let husked = given
            .into_iter()
            .map(|(name, text)| {
                let shown = name.as_str().to_owned();
                let node = self
                    .outlines
                    .node(&name)
                    .or_else(|| {
                        pending
                            .iter()
                            .map(Parsed::given)
                            .find(|node| node.hash == shown)
                    })
                    .ok_or_else(|| NotItsHusk::Unknown(shown.clone()))?;
                let husk =
                    suzatary_store::store::Husk::<crate::secretary::line_event::LineEvent>::read(
                        &text,
                    )
                    .map_err(|e| NotItsHusk::Unread(shown.clone(), e))?;
                if crate::secretary::line_form::line_header(husk.event()) != node.header {
                    return Err(NotItsHusk::Header(shown));
                }
                if husk.opens(|key| ring.holds(key) && !dead.contains(key)) {
                    return Err(NotItsHusk::Opens(shown));
                }
                let parents = node
                    .parents
                    .iter()
                    .map(|parent| Hash::new(parent.clone()))
                    .collect::<Result<_, _>>()
                    .map_err(|_| NotItsHusk::Header(shown))?;
                Ok(Husked {
                    name,
                    parents,
                    husk,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.husked(husked.iter().cloned());
        let moved = Moved::These(
            husked
                .iter()
                .map(|h| h.husk.event().conversation().clone())
                .collect(),
        );
        // No row moves: the frame's delta forgets nothing, and its husks are the move.
        Ok(Took {
            frame: LineFrame {
                delta: LineDelta::Forgot(BTreeSet::new()),
                husked,
            },
            moved,
        })
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
        let mut keys = landed.keys().clone();
        let mut own: BTreeMap<&Hash, &Parsed> = BTreeMap::new();
        for line in self
            .own
            .standing(|name| reading.holds(name), reading.gone())
        {
            if let OwnLine::Line {
                node, keys: opens, ..
            } = line
            {
                own.insert(node.name(), node);
                keys = keys.union(opens);
            }
        }
        let came: Vec<Parsed> = reading
            .nodes()
            .chain(own.values().copied())
            .filter(|node| !self.outlines.holds(node.name()))
            .cloned()
            .collect();
        let went: Vec<Hash> = self
            .outlines
            .names()
            .filter(|name| !reading.holds(name) && !own.contains_key(name))
            .cloned()
            .collect();
        let mut touched = if came.is_empty() && went.is_empty() {
            BTreeSet::new()
        } else {
            self.outlines.hold(came, &went)
        };
        let rekeyed = self.lines.rekey(keys);
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
        let mut dead = self.lines.dead(&|key| ring.holds(key));
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
