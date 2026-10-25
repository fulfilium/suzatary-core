//! The lines a replica holds, read through the Prodrome's memoised fold, so a reading costs what
//! changed since the last one and its value is the plain reading's: every print decoded at once,
//! its largest down-set read by [`transcript::transcripts`] (`tests/held_lines.rs` states the law).
//!
//! Each print is decoded once, when it arrives. The history is the objects whose every ancestor
//! is held, kept as prints arrive, as a store's down-set is. The tree the fold walks has three
//! levels: the store, one node per conversation, and one leaf per line of the history. A leaf's
//! name covers the line and how each slot reads (erased by which erasure line, its key held, or
//! missing), and a conversation's covers its leaves, so a new line renames its leaf, its
//! conversation and the store, and every other node is one cache hit. Words are opened at the
//! leaves, the expensive step; a conversation orders its opened lines and folds them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use prodrome_core::dag::{Excluded, Unread, decode};
use prodrome_core::event::{Hash, parents_of};
use prodrome_core::literal::Datetime;
use prodrome_core::memo::{self, Algebra, Cache, Report, Tree};

use crate::secretary::answer::Renderable;
use crate::secretary::erasure::Erasure;
use crate::secretary::key_ring::KeyRing;
use crate::secretary::line_event::{LineEvent, LineVocabulary};
use crate::secretary::line_shape::LineShape;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::store::{ConversationId, Entry, NotAConversation, Stored};
use crate::secretary::transcript;
use crate::secretary::words::{Gone, Words};

/// The lines held: every print decoded once, the history, and the fold's cache over it.
pub struct HeldLines {
    prints: BTreeMap<Hash, Print>,
    /// Every name a held object rests on, with the objects resting on it directly.
    children: BTreeMap<Hash, BTreeSet<Hash>>,
    /// The names something rests on, or a print names, that are no object held.
    lacking: BTreeSet<Hash>,
    /// The objects whose every ancestor is held.
    history: BTreeSet<Hash>,
    conversations: BTreeMap<ConversationId, Conversation>,
    /// The conversations whose lines name each key.
    owners: BTreeMap<KeyName, BTreeSet<ConversationId>>,
    keys: KeyRing,
    cache: Cache<Read>,
    algebra: Reader,
    /// The store's name at the last fold of it, whose entry the next one replaces.
    root: Option<Hash>,
}

/// A line of the history as a build reads it: its name, its object and what it rests on.
type HeldLine<'p> = (&'p Hash, &'p Arc<LineEvent>, &'p Vec<Hash>);

/// One print, decoded once.
enum Print {
    /// An object carrying a line, and the names it rests on.
    Line {
        line: Arc<LineEvent>,
        parents: Vec<Hash>,
    },
    /// An object carrying no line, such as the genesis, and the names it rests on.
    Structure {
        parents: Vec<Hash>,
    },
    Unread(Unread),
}

impl Print {
    /// The names an object rests on; none for a print that is no object.
    fn parents(&self) -> Option<&[Hash]> {
        match self {
            Print::Line { parents, .. } | Print::Structure { parents } => Some(parents),
            Print::Unread(_) => None,
        }
    }
}

/// One conversation's lines in the history, each line's leaf as last built, and its node.
struct Conversation {
    lines: BTreeSet<Hash>,
    /// A rebuild keeps a line's leaf, and its name, while its slots read the same.
    leaves: BTreeMap<Hash, Arc<Node>>,
    built: Built,
}

/// A conversation's node, current or not.
enum Built {
    /// Never built.
    Unbuilt,
    /// Its lines or the keys of their words moved since it was built: rebuilt before it is read.
    /// `cached` names the node its last build left in the cache.
    Stale { cached: Hash },
    /// Current, with every slot of its lines an erasure of its own names.
    Fresh {
        node: Arc<Node>,
        dead: BTreeSet<KeyName>,
    },
}

/// A node of the tree the fold walks.
enum Node {
    Store {
        name: Hash,
        conversations: Vec<Arc<Node>>,
    },
    Conversation {
        name: Hash,
        id: ConversationId,
        lines: Vec<Arc<Node>>,
    },
    Line {
        name: Hash,
        leaf: Leaf,
    },
}

/// One line of the history, with how each slot of it reads, and the reseals of its slots (by the
/// key of the slot each seals), whose own slots `slots` reads too.
struct Leaf {
    object: Hash,
    line: Arc<LineEvent>,
    parents: Vec<Hash>,
    slots: BTreeMap<KeyName, Slot>,
    reseals: BTreeMap<KeyName, Vec<(Hash, Arc<LineEvent>)>>,
}

/// How one slot reads: erased by the erasure line named `by`, its key held, or missing.
#[derive(PartialEq, Eq)]
enum Slot {
    Erased { by: Hash, erasure: Erasure },
    Held,
    Missing,
}

impl Slot {
    fn print(&self) -> String {
        match self {
            Slot::Erased { by, .. } => format!("erased by {by}"),
            Slot::Held => "held".to_owned(),
            Slot::Missing => "missing".to_owned(),
        }
    }
}

/// One line opened: what ordering and folding its conversation read of it.
#[derive(Debug, PartialEq, Eq)]
struct Opened {
    object: Hash,
    at: Datetime,
    parents: Vec<Hash>,
    entry: Entry,
}

/// What a node folds to: the lines opened beneath it that no conversation has closed yet, and the
/// conversations closed. The join of the children's, and a conversation closes its open lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Read {
    open: Vec<Arc<Opened>>,
    closed: BTreeMap<ConversationId, Result<Arc<Stored>, NotAConversation>>,
}

impl Read {
    fn join(children: Vec<Read>) -> Read {
        children.into_iter().fold(Read::default(), |mut all, one| {
            all.open.extend(one.open);
            all.closed.extend(one.closed);
            all
        })
    }
}

impl Tree for Node {
    fn name(&self) -> &Hash {
        match self {
            Node::Store { name, .. }
            | Node::Conversation { name, .. }
            | Node::Line { name, .. } => name,
        }
    }

    fn children(&self) -> impl Iterator<Item = &Self> {
        let children: Box<dyn Iterator<Item = &Self>> = match self {
            Node::Store { conversations, .. } => Box::new(conversations.iter().map(|c| &**c)),
            Node::Conversation { lines, .. } => Box::new(lines.iter().map(|line| &**line)),
            Node::Line { .. } => Box::new(std::iter::empty()),
        };
        children
    }
}

/// The reading's algebra. It reads a held slot's key from the ring, which the slot's name covers:
/// a key's name is the hash of its bytes.
struct Reader {
    name: Hash,
}

impl Reader {
    fn new() -> Self {
        Self {
            name: memo::name(
                b"suzatary held lines: open at the leaves, fold at a conversation",
                [],
            ),
        }
    }
}

/// The algebra with the ring it reads held keys from.
struct Reading<'k> {
    reader: &'k Reader,
    keys: &'k KeyRing,
}

impl Algebra<Node> for Reading<'_> {
    type Out = Read;

    fn name(&self) -> &Hash {
        &self.reader.name
    }

    fn apply(&self, node: &Node, children: Vec<Read>) -> Read {
        match node {
            Node::Store { .. } => Read::join(children),
            Node::Conversation { id, .. } => {
                let mut read = Read::join(children);
                let lines = std::mem::take(&mut read.open);
                read.closed
                    .insert(id.clone(), folded(id, &lines).map(Arc::new));
                read
            }
            Node::Line { leaf, .. } => {
                let slots = |name: &KeyName| -> Words<TextKey> {
                    match leaf.slots.get(name) {
                        Some(Slot::Erased { erasure, .. }) => Gone::Erased {
                            erasure: erasure.clone(),
                        }
                        .into(),
                        Some(Slot::Held) => self.keys.key(name),
                        Some(Slot::Missing) | None => Gone::Missing.into(),
                    }
                };
                let reseals: BTreeMap<KeyName, Vec<&LineEvent>> = leaf
                    .reseals
                    .iter()
                    .map(|(of, seals)| {
                        (of.clone(), seals.iter().map(|(_, line)| &**line).collect())
                    })
                    .collect();
                let read = transcript::Through {
                    erased: &BTreeMap::new(),
                    reseals: &reseals,
                    slots: &slots,
                };
                Read {
                    open: vec![Arc::new(Opened {
                        object: leaf.object.clone(),
                        at: leaf.line.at(),
                        parents: leaf.parents.clone(),
                        entry: leaf.line.entry(&read),
                    })],
                    closed: BTreeMap::new(),
                }
            }
        }
    }
}

/// One conversation's opened lines, in causal order, folded.
fn folded(id: &ConversationId, lines: &[Arc<Opened>]) -> Result<Stored, NotAConversation> {
    let order = transcript::causal(
        &lines
            .iter()
            .map(|line| (&line.object, line.at, line.parents.as_slice()))
            .collect::<Vec<_>>(),
    );
    Stored::of_lines(id, order.iter().map(|(at, _)| &lines[*at].entry))
}

impl Default for HeldLines {
    fn default() -> Self {
        Self::new()
    }
}

impl HeldLines {
    /// Nothing held.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prints: BTreeMap::new(),
            children: BTreeMap::new(),
            lacking: BTreeSet::new(),
            history: BTreeSet::new(),
            conversations: BTreeMap::new(),
            owners: BTreeMap::new(),
            keys: KeyRing::default(),
            cache: Cache::default(),
            algebra: Reader::new(),
            root: None,
        }
    }

    /// `add`, each print beside the name it is claimed to have, decoded once, and `drop` let go.
    /// A print whose bytes are not an object at the schema is held as unread, and what rests on
    /// it stays out of the history.
    pub fn hold(&mut self, add: Vec<(Hash, Vec<u8>)>, drop: &[Hash]) {
        let dropped = drop
            .iter()
            .filter(|name| self.prints.remove(*name).is_some())
            .count();
        let mut arrived = Vec::new();
        for (name, bytes) in add {
            if self.prints.contains_key(&name) {
                continue;
            }
            let print = match decode::<LineEvent>(&LineVocabulary, &name, &bytes) {
                Ok(envelope) => {
                    let parents = parents_of(&envelope);
                    match envelope.into_event() {
                        Some(line) => Print::Line {
                            line: Arc::new(line),
                            parents,
                        },
                        None => Print::Structure { parents },
                    }
                }
                Err(why) => Print::Unread(why),
            };
            self.prints.insert(name.clone(), print);
            arrived.push(name);
        }
        if dropped > 0 {
            self.rebuild();
        } else {
            for name in &arrived {
                self.place(name);
            }
            for name in &arrived {
                self.enter(name);
            }
        }
    }

    /// The keys `given` joined in and `gone` taken out: each conversation a line of which names a
    /// key whose standing moved is read again. What was opened with a key taken out is forgotten
    /// at once, not at the next reading: the leaves naming it, their conversations and the store.
    pub fn keys(&mut self, given: &BTreeMap<KeyName, String>, gone: &[KeyName]) {
        let moved = self.keys.change(given, gone);
        let lost: BTreeSet<&KeyName> = moved.iter().filter(|name| !self.keys.holds(name)).collect();
        let ids: BTreeSet<ConversationId> = moved
            .iter()
            .flat_map(|name| self.owners.get(name).into_iter().flatten().cloned())
            .collect();
        let mut opened = Vec::new();
        for id in &ids {
            self.stale(id);
            let Some(conversation) = self.conversations.get(id) else {
                continue;
            };
            if let Built::Stale { cached } = &conversation.built
                && !lost.is_empty()
            {
                opened.push(cached.clone());
            }
            opened.extend(
                conversation
                    .leaves
                    .values()
                    .filter_map(|leaf| match &**leaf {
                        Node::Line { name, leaf }
                            if leaf.slots.keys().any(|key| lost.contains(key)) =>
                        {
                            Some(name.clone())
                        }
                        Node::Line { .. } | Node::Store { .. } | Node::Conversation { .. } => None,
                    }),
            );
        }
        if !opened.is_empty() {
            opened.extend(self.root.take());
            self.forget(opened);
        }
    }

    /// Every conversation, newest first, each its fold or why its lines are none; the first such
    /// refusal, by id, stands for the store, as the plain reading's does. Beside it, what the fold
    /// computed and what it found in the cache.
    ///
    /// # Errors
    /// The first conversation, by id, whose lines do not fold.
    pub fn read(&mut self) -> (Result<Vec<Arc<Stored>>, NotAConversation>, Report) {
        let ids: Vec<ConversationId> = self.conversations.keys().cloned().collect();
        let conversations: Vec<Arc<Node>> = ids.iter().filter_map(|id| self.built(id)).collect();
        let name = memo::name(b"store", conversations.iter().map(|node| node.name()));
        let store = Node::Store {
            name: name.clone(),
            conversations,
        };
        let (read, report) = memo::fold(
            &Reading {
                reader: &self.algebra,
                keys: &self.keys,
            },
            &store,
            &mut self.cache,
        );
        if let Some(was) = self.root.replace(name.clone()).filter(|was| *was != name) {
            self.forget([was]);
        }
        let stored = read
            .closed
            .into_values()
            .collect::<Result<Vec<_>, _>>()
            .map(|mut stored| {
                stored.reverse();
                stored
            });
        (stored, report)
    }

    /// Conversation `id`'s fold, or why its lines are none; `None` where the history holds no line
    /// of it. Only its own node is folded.
    pub fn conversation(
        &mut self,
        id: &ConversationId,
    ) -> Option<Result<Arc<Stored>, NotAConversation>> {
        let node = self.built(id)?;
        let (mut read, _) = memo::fold(
            &Reading {
                reader: &self.algebra,
                keys: &self.keys,
            },
            &*node,
            &mut self.cache,
        );
        read.closed.remove(id)
    }

    /// Every key an erasure line of its own conversation names: what a replica forgets.
    pub fn dead(&mut self) -> BTreeSet<KeyName> {
        let ids: Vec<ConversationId> = self.conversations.keys().cloned().collect();
        for id in &ids {
            self.built(id);
        }
        self.conversations
            .values()
            .flat_map(|conversation| match &conversation.built {
                Built::Fresh { dead, .. } => dead.iter().cloned().collect::<Vec<_>>(),
                Built::Stale { .. } | Built::Unbuilt => Vec::new(),
            })
            .collect()
    }

    /// Every key an erasure of its own conversation names, and every key in `told` a line names,
    /// each with the blocks its words were rendered to where `keys` opens it: what a replica
    /// forgets, its rendered blocks first, then the key, as the box's sweep does.
    #[must_use]
    pub fn erased(
        &self,
        keys: &KeyRing,
        told: &BTreeSet<KeyName>,
    ) -> BTreeMap<KeyName, Vec<Renderable>> {
        let lines: Vec<(&Hash, &LineEvent)> = self.lines().collect();
        let named = lines.iter().flat_map(|(_, line)| {
            line.keys()
                .enumerate()
                .filter(|(_, name)| told.contains(*name))
                .map(move |(slot, name)| (*line, slot, name))
        });
        let mut dead: BTreeMap<KeyName, Vec<Renderable>> = BTreeMap::new();
        let opens = |line: &LineEvent| {
            transcript::opens(
                line,
                line.keys()
                    .next()
                    .and_then(|own| keys.key(own).into_kept())
                    .as_ref(),
            )
        };
        for (line, slot, name) in transcript::dead(lines.iter().copied(), &opens)
            .into_iter()
            .chain(named)
        {
            let fed = match keys.key(name) {
                Words::Kept { kept } => transcript::fed(line, slot, &kept),
                Words::Gone { .. } => Vec::new(),
            };
            let blocks = dead.entry(name.clone()).or_default();
            for block in fed {
                if !blocks.contains(&block) {
                    blocks.push(block);
                }
            }
        }
        dead
    }

    /// What the history leaves out, as the Prodrome's DAG says it (`Dag::excluded`): each name
    /// lacking (a print that is not an object, or a parent no object is) with why and what rests
    /// on it.
    #[must_use]
    pub fn excluded(&self) -> Vec<Excluded> {
        self.lacking
            .iter()
            .map(|name| {
                let mut resting = BTreeSet::new();
                let mut pending: Vec<&Hash> =
                    self.children.get(name).into_iter().flatten().collect();
                while let Some(next) = pending.pop() {
                    if resting.insert(next.clone()) {
                        pending.extend(self.children.get(next).into_iter().flatten());
                    }
                }
                Excluded {
                    name: name.clone(),
                    why: match self.prints.get(name) {
                        Some(Print::Unread(why)) => Some(why.clone()),
                        Some(Print::Line { .. } | Print::Structure { .. }) | None => None,
                    },
                    resting,
                }
            })
            .collect()
    }

    /// Every line of the history, by name.
    fn lines(&self) -> impl Iterator<Item = (&Hash, &LineEvent)> {
        self.history
            .iter()
            .filter_map(|name| match self.prints.get(name) {
                Some(Print::Line { line, .. }) => Some((name, &**line)),
                Some(Print::Structure { .. } | Print::Unread(_)) | None => None,
            })
    }

    /// The edges of the print `name`, and what it makes lacking or no longer lacking.
    fn place(&mut self, name: &Hash) {
        let Some(print) = self.prints.get(name) else {
            return;
        };
        match print.parents() {
            Some(parents) => {
                self.lacking.remove(name);
                for parent in parents {
                    self.children
                        .entry(parent.clone())
                        .or_default()
                        .insert(name.clone());
                    if self.prints.get(parent).and_then(Print::parents).is_none() {
                        self.lacking.insert(parent.clone());
                    }
                }
            }
            None => {
                self.lacking.insert(name.clone());
            }
        }
    }

    /// The object `name` into the history once every parent is in it, and then each object
    /// waiting on it that it completes.
    fn enter(&mut self, name: &Hash) {
        let mut pending = vec![name.clone()];
        while let Some(next) = pending.pop() {
            if self.history.contains(&next) {
                continue;
            }
            let Some(print) = self.prints.get(&next) else {
                continue;
            };
            let Some(parents) = print.parents() else {
                continue;
            };
            if !parents.iter().all(|parent| self.history.contains(parent)) {
                continue;
            }
            let line = match print {
                Print::Line { line, .. } => Some(Arc::clone(line)),
                Print::Structure { .. } | Print::Unread(_) => None,
            };
            self.history.insert(next.clone());
            if let Some(line) = line {
                let id = line.conversation().clone();
                for key in line.keys() {
                    self.owners
                        .entry(key.clone())
                        .or_default()
                        .insert(id.clone());
                }
                self.conversations
                    .entry(id.clone())
                    .or_insert_with(|| Conversation {
                        lines: BTreeSet::new(),
                        leaves: BTreeMap::new(),
                        built: Built::Unbuilt,
                    })
                    .lines
                    .insert(next.clone());
                self.stale(&id);
            }
            pending.extend(self.children.get(&next).into_iter().flatten().cloned());
        }
    }

    /// Everything derived from the prints made again, after some were let go: the decoded prints
    /// and the cache's entries for nodes whose names come back stay.
    fn rebuild(&mut self) {
        self.children.clear();
        self.lacking.clear();
        self.history.clear();
        self.owners.clear();
        let was = std::mem::take(&mut self.conversations);
        let names: Vec<Hash> = self.prints.keys().cloned().collect();
        for name in &names {
            self.place(name);
        }
        for name in &names {
            self.enter(name);
        }
        for (id, conversation) in was {
            let cached = match conversation.built {
                Built::Unbuilt => None,
                Built::Stale { cached } => Some(cached),
                Built::Fresh { node, .. } => Some(node.name().clone()),
            };
            match self.conversations.get_mut(&id) {
                Some(now) => {
                    now.leaves = conversation.leaves;
                    now.built = cached.map_or(Built::Unbuilt, |cached| Built::Stale { cached });
                }
                None => self.forget(
                    conversation
                        .leaves
                        .values()
                        .map(|leaf| leaf.name().clone())
                        .chain(cached),
                ),
            }
        }
    }

    /// Conversation `id` read again before its next reading.
    fn stale(&mut self, id: &ConversationId) {
        if let Some(conversation) = self.conversations.get_mut(id)
            && let Built::Fresh { node, .. } = &conversation.built
        {
            conversation.built = Built::Stale {
                cached: node.name().clone(),
            };
        }
    }

    /// Conversation `id`'s node, built from its lines if they moved since; the entries of its
    /// last build that the new one does not name are dropped from the cache.
    fn built(&mut self, id: &ConversationId) -> Option<Arc<Node>> {
        let conversation = self.conversations.get_mut(id)?;
        let was = match &conversation.built {
            Built::Fresh { node, .. } => return Some(Arc::clone(node)),
            Built::Stale { cached } => Some(cached.clone()),
            Built::Unbuilt => None,
        };
        let (node, dead, leaves) = build(&self.prints, &self.keys, id, conversation);
        let gone: Vec<Hash> = conversation
            .leaves
            .iter()
            .filter(|(line, leaf)| leaves.get(*line).is_none_or(|now| !Arc::ptr_eq(now, leaf)))
            .map(|(_, leaf)| leaf.name().clone())
            .chain(was.filter(|name| name != node.name()))
            .collect();
        conversation.leaves = leaves;
        conversation.built = Built::Fresh {
            node: Arc::clone(&node),
            dead,
        };
        self.forget(gone);
        Some(node)
    }

    fn forget(&mut self, names: impl IntoIterator<Item = Hash>) {
        for name in names {
            self.cache.remove(&memo::Key {
                function: self.algebra.name.clone(),
                argument: name,
            });
        }
    }
}

/// The node of conversation `id` over its lines in the history, every key of them an erasure of
/// its own names, and each line's leaf, the one it had wherever its slots read the same.
fn build(
    prints: &BTreeMap<Hash, Print>,
    keys: &KeyRing,
    id: &ConversationId,
    conversation: &Conversation,
) -> (Arc<Node>, BTreeSet<KeyName>, BTreeMap<Hash, Arc<Node>>) {
    let lines: Vec<HeldLine<'_>> = conversation
        .lines
        .iter()
        .filter_map(|name| match prints.get(name) {
            Some(Print::Line { line, parents }) => Some((name, line, parents)),
            Some(Print::Structure { .. } | Print::Unread(_)) | None => None,
        })
        .collect();
    let erasers = transcript::erasers(lines.iter().map(|(name, line, _)| (*name, &***line)));
    let opens = |line: &LineEvent| {
        transcript::opens(
            line,
            line.keys()
                .next()
                .and_then(|own| keys.key(own).into_kept())
                .as_ref(),
        )
    };
    let superseded = transcript::superseded(
        lines.iter().map(|(name, line, _)| (*name, &***line)),
        &opens,
    );
    let dead: BTreeSet<KeyName> = lines
        .iter()
        .flat_map(|(_, line, _)| line.keys())
        .filter(|key| erasers.contains_key(*key) || superseded.contains(*key))
        .cloned()
        .collect();
    let mut reseals: BTreeMap<KeyName, Vec<(Hash, Arc<LineEvent>)>> = BTreeMap::new();
    for (name, line, _) in &lines {
        if let LineShape::Resealed { of, .. } = line.shape() {
            reseals
                .entry(of.clone())
                .or_default()
                .push(((*name).clone(), Arc::clone(line)));
        }
    }
    let order = transcript::reseals(lines.iter().map(|(name, line, _)| (*name, &***line)));
    for (of, seals) in &mut reseals {
        let Some(ranked) = order.get(of) else {
            continue;
        };
        seals.sort_by_key(|(_, line)| {
            ranked
                .iter()
                .position(|ranked| std::ptr::eq(*ranked, std::ptr::from_ref(&**line)))
        });
    }
    let status = |key: &KeyName| match erasers.get(key) {
        Some((by, erasure)) => Slot::Erased {
            by: (*by).clone(),
            erasure: (*erasure).clone(),
        },
        None if keys.holds(key) => Slot::Held,
        None => Slot::Missing,
    };
    let leaves: BTreeMap<Hash, Arc<Node>> = lines
        .iter()
        .map(|(name, line, parents)| {
            let own: BTreeMap<KeyName, Vec<(Hash, Arc<LineEvent>)>> = line
                .keys()
                .filter_map(|key| Some((key.clone(), reseals.get(key)?.clone())))
                .collect();
            let slots: BTreeMap<KeyName, Slot> = line
                .keys()
                .chain(own.values().flatten().flat_map(|(_, reseal)| reseal.keys()))
                .map(|key| (key.clone(), status(key)))
                .collect();
            let kept = conversation
                .leaves
                .get(*name)
                .filter(|leaf| match &***leaf {
                    Node::Line { leaf, .. } => {
                        leaf.slots == slots && names(&leaf.reseals) == names(&own)
                    }
                    Node::Store { .. } | Node::Conversation { .. } => false,
                });
            let leaf = kept.map_or_else(
                || Arc::new(leaf(name, line, parents, slots, own)),
                Arc::clone,
            );
            ((*name).clone(), leaf)
        })
        .collect();
    let name = memo::name(
        format!("conversation {id}").as_bytes(),
        leaves.values().map(|leaf| leaf.name()),
    );
    let node = Arc::new(Node::Conversation {
        name,
        id: id.clone(),
        lines: leaves.values().cloned().collect(),
    });
    (node, dead, leaves)
}

/// The names of the reseals of a leaf's slots, by the key of the slot each seals.
fn names(reseals: &BTreeMap<KeyName, Vec<(Hash, Arc<LineEvent>)>>) -> Vec<(&KeyName, &Hash)> {
    reseals
        .iter()
        .flat_map(|(of, seals)| seals.iter().map(move |(name, _)| (of, name)))
        .collect()
}

/// The leaf of the line `name`, its name covering the line, how each slot reads, and each reseal
/// of its slots with how that reads.
fn leaf(
    name: &Hash,
    line: &Arc<LineEvent>,
    parents: &[Hash],
    slots: BTreeMap<KeyName, Slot>,
    reseals: BTreeMap<KeyName, Vec<(Hash, Arc<LineEvent>)>>,
) -> Node {
    let read = |key: &KeyName| slots.get(key).map_or_else(String::new, Slot::print);
    let own = format!(
        "line {name} {} resealed {}",
        line.keys().map(read).collect::<Vec<_>>().join(","),
        reseals
            .values()
            .flatten()
            .map(|(seal, reseal)| format!(
                "{seal}:{}",
                reseal.keys().map(read).collect::<Vec<_>>().join(",")
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    Node::Line {
        name: memo::name(own.as_bytes(), []),
        leaf: Leaf {
            object: name.clone(),
            line: Arc::clone(line),
            parents: parents.to_vec(),
            slots,
            reseals,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::conversations::InMemory;
    use crate::secretary::store::Kind;
    use crate::store::Doc as _;
    use crate::store::Prodrome as _;
    use crate::store::Reads as _;
    use prodrome_core::event::canonical_envelope;

    /// Every key `held` keeps, spelled as a replica is given it.
    fn given(held: &InMemory) -> BTreeMap<KeyName, String> {
        held.keys
            .names()
            .into_iter()
            .map(|name| {
                let key = held.keys.get(&name).expect("reads").expect("held");
                (name, crate::secretary::key_ring::spelled(&key))
            })
            .collect()
    }

    /// Every object `held` keeps, printed as a replica is given it.
    fn prints(held: &InMemory) -> Vec<(Hash, Vec<u8>)> {
        held.lines
            .objects()
            .expect("objects")
            .into_iter()
            .map(|(name, object)| (name, canonical_envelope(&object).into_bytes()))
            .collect()
    }

    fn opened(title: &str) -> Entry {
        Entry::Opened {
            at: Datetime::new(2026, 10, 1, 9, 0, 0, 0).expect("an instant"),
            title: Words::kept(title.to_owned()),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        }
    }

    /// Opening held lines parses their words and scrubs none: the scrub ran once, when each line
    /// was written.
    #[test]
    fn opening_held_lines_scrubs_nothing() {
        let held = InMemory::default();
        let id = ConversationId::new("2026-10-01-1").expect("a slug");
        transcript::start(held.conversations(), &id, &opened("Notes, café")).expect("started");
        for n in 0..40 {
            let said = Entry::Said {
                at: Datetime::new(2026, 10, 1, 9, 1, n, 0).expect("an instant"),
                turn: crate::capability::Turn::Person {
                    text: format!("Merci, voilà la réponse {n} — 確認"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            };
            transcript::push(held.conversations(), &id, &said).expect("pushed");
        }
        let mut lines = HeldLines::new();
        let before = crate::redact::scrubs();
        assert!(before > 40, "each write scrubbed its words");
        lines.keys(&given(&held), &[]);
        lines.hold(prints(&held), &[]);
        let read = lines.read().0.expect("reads");
        assert_eq!(read[0].turns.len(), 40, "every line opened");
        assert_eq!(crate::redact::scrubs(), before, "and none scrubbed");
    }

    /// A replica reads a conversation whose stale slots were sealed again, and whose replaced
    /// keys are gone, as the box reads it: each slot from the reseal that stands for it.
    #[test]
    fn a_resealed_conversation_reads_as_the_box_reads_it() {
        use crate::secretary::line_event::LineEvent;
        use crate::secretary::scrubbed_under::ScrubbedUnder;
        let held = InMemory::default();
        let id = ConversationId::new("2026-10-25-1").expect("a slug");
        let at = |second: u32| Datetime::new(2026, 10, 25, 9, 0, second, 0).expect("an instant");
        let said = Entry::Said {
            at: at(1),
            turn: crate::capability::Turn::Person {
                text: "reset at https://example.test/r?token=s3cret".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        };
        for (count, entry) in [opened("Notes"), said].iter().enumerate() {
            let (line, keys) = LineEvent::written_under(
                &ScrubbedUnder::Unversioned,
                id.clone(),
                transcript::writer(),
                u32::try_from(count).expect("a count"),
                entry,
                &held.entropy,
            )
            .expect("a line");
            for key in &keys {
                held.keys.set(&key.name(), key).expect("kept");
            }
            held.lines.append(line).expect("appended");
        }
        let done = crate::secretary::rescrub::run(held.conversations(), at(9)).expect("runs");
        assert_eq!((done.resealed, done.destroyed), (2, 2));
        let boxed = transcript::read(held.conversations(), &id)
            .expect("reads")
            .into_stored();
        let mut lines = HeldLines::new();
        lines.keys(&given(&held), &[]);
        lines.hold(prints(&held), &[]);
        let read = lines.read().0.expect("reads");
        assert_eq!(*read[0], boxed);
        assert!(!format!("{boxed:?}").contains("s3cret"), "{boxed:?}");
        assert_eq!(
            lines.dead().len(),
            2,
            "a replica forgets each key a reseal replaced"
        );
    }

    /// A key taken out takes what was opened with it out of the cache at once, before any reading.
    #[test]
    fn a_key_taken_out_leaves_no_words_it_opened() {
        let held = InMemory::default();
        let id = ConversationId::new("2026-10-01-1").expect("a slug");
        transcript::start(
            held.conversations(),
            &id,
            &opened("a title nobody else says"),
        )
        .expect("started");
        let mut lines = HeldLines::new();
        let given = given(&held);
        lines.keys(&given, &[]);
        lines.hold(prints(&held), &[]);
        assert!(lines.read().0.is_ok());
        let says = |lines: &HeldLines| {
            lines
                .cache
                .pairs()
                .any(|(_, read)| format!("{read:?}").contains("a title nobody else says"))
        };
        assert!(says(&lines), "the reading opened the title");
        let gone: Vec<KeyName> = given.into_keys().collect();
        lines.keys(&BTreeMap::new(), &gone);
        assert!(!says(&lines), "nothing cached still says it");
    }
}
