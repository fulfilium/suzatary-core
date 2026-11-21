//! The skeleton's lines by conversation, each conversation's outline kept until a line of it comes
//! or goes: what the stream counts and chooses its rounds by, with no line's object read; and
//! each conversation's heads, which a new line rests on.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;

use crate::secretary::outline::{self, Outline};
use crate::secretary::step::Step;
use crate::wire::lines as wire;
use suzatary_base::conversation_id::ConversationId;
use suzatary_memory::secretary::locked_text::KeyName;

/// The skeleton's lines held, by conversation, and every node it holds as it was given.
#[derive(Default)]
pub struct Outlines {
    nodes: BTreeMap<Hash, Node>,
    conversations: BTreeMap<ConversationId, Conversation>,
    given: BTreeMap<Hash, wire::SkeletonNode>,
    /// What each node held rests on, parsed once, a structure node's included.
    parents: BTreeMap<Hash, Vec<Hash>>,
    /// The lines held resting on each name.
    children: BTreeMap<Hash, BTreeSet<Hash>>,
}

/// A conversation's lines as a new line rests on them.
#[derive(Debug)]
pub enum Frontier<'o> {
    /// All its lines rest on is held: the lines none rests on, and how many it has.
    Closed {
        heads: &'o BTreeSet<Hash>,
        lines: usize,
    },
    /// What its lines rest on that is not held.
    Open(&'o BTreeSet<Hash>),
}

/// What a reading reads of the skeleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Every node.
    Whole,
    /// These conversations, each with every node its lines rest on.
    Conversations(BTreeSet<ConversationId>),
    /// These nodes (the lines the stream chose), and nothing they rest on.
    Objects(BTreeSet<Hash>),
    /// Each conversation a line naming one of these keys is in, read as `Conversations`.
    Naming(BTreeSet<KeyName>),
}

/// One skeleton node, parsed once where it arrives, beside the node as it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    given: wire::SkeletonNode,
    read: Read,
}

/// What a node is: a line, or a node that carries none, such as the genesis, by its name and
/// what it rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    Line(Node),
    Structure(Hash, Vec<Hash>),
}

/// One line node, parsed once where it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    name: Hash,
    conversation: ConversationId,
    at: Datetime,
    parents: Vec<Hash>,
    keys: Vec<KeyName>,
    step: Step,
    writer: wire::LineWriter,
}

/// A skeleton node that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a node of the lines does not read: {0}")]
pub struct NotANode(String);

impl Parsed {
    /// The node `node` is.
    ///
    /// # Errors
    /// [`NotANode`] for a name, a conversation, an instant or a key that does not read.
    pub fn of(node: wire::SkeletonNode) -> Result<Self, NotANode> {
        let name = |text: &str| Hash::new(text).map_err(|e| NotANode(e.to_string()));
        let read = match &node.header {
            wire::NodeHeader::Structure { .. } => Read::Structure(
                name(&node.hash)?,
                node.parents
                    .iter()
                    .map(|parent| name(parent))
                    .collect::<Result<_, _>>()?,
            ),
            wire::NodeHeader::Line {
                conversation,
                at,
                keys,
                step,
                writer,
                ..
            } => Read::Line(Node {
                name: name(&node.hash)?,
                conversation: ConversationId::new(conversation.clone())
                    .map_err(|e| NotANode(e.to_string()))?,
                at: fpl::parse_iso(at)
                    .map_err(|e| NotANode(e.to_string()))
                    .and_then(|at| fpl::datetime_of(at).map_err(|e| NotANode(e.to_string())))?,
                parents: node
                    .parents
                    .iter()
                    .map(|parent| name(parent))
                    .collect::<Result<_, _>>()?,
                keys: keys
                    .iter()
                    .map(|key| KeyName::new(key.clone()).map_err(|e| NotANode(e.to_string())))
                    .collect::<Result<_, _>>()?,
                step: step.clone(),
                writer: *writer,
            }),
        };
        Ok(Parsed { given: node, read })
    }
}

impl Parsed {
    /// The node's name.
    #[must_use]
    pub fn name(&self) -> &Hash {
        match &self.read {
            Read::Line(line) => &line.name,
            Read::Structure(name, _) => name,
        }
    }

    /// What the node rests on.
    #[must_use]
    pub fn parents(&self) -> &[Hash] {
        match &self.read {
            Read::Line(line) => &line.parents,
            Read::Structure(_, parents) => parents,
        }
    }

    /// The node as it was given.
    #[must_use]
    pub fn given(&self) -> &wire::SkeletonNode {
        &self.given
    }

    fn line(&self) -> Option<&Node> {
        match &self.read {
            Read::Line(line) => Some(line),
            Read::Structure(..) => None,
        }
    }
}

/// A node of the lines' replica: a genesis is its own prodrome, and a line rests on its genesis
/// through its parents.
impl suzatary_store::replica::held::Node for Parsed {
    fn name(&self) -> &Hash {
        Parsed::name(self)
    }

    fn rests_on(&self) -> &[Hash] {
        self.parents()
    }

    fn prodrome(&self) -> Option<&Hash> {
        match &self.given.header {
            wire::NodeHeader::Structure { kind } => (kind == "Genesis").then(|| self.name()),
            wire::NodeHeader::Line { .. } => None,
        }
    }
}

/// One conversation's lines, its heads and what its lines rest on that is not held, and its
/// outline once folded, emptied whenever a line comes or goes.
#[derive(Default)]
struct Conversation {
    lines: BTreeSet<Hash>,
    heads: BTreeSet<Hash>,
    lacking: BTreeSet<Hash>,
    outline: OnceCell<Outline>,
}

impl Outlines {
    /// The nodes `add` held and the names `drop` let go: each conversation a line of which came
    /// or went is folded again at its next reading, and named in the answer.
    pub fn hold(&mut self, add: Vec<Parsed>, drop: &[Hash]) -> BTreeSet<ConversationId> {
        let mut moved = BTreeSet::new();
        // Each name whose standing as a head or as lacking in a conversation may have changed.
        let mut touched: BTreeSet<(Hash, ConversationId)> = BTreeSet::new();
        for name in drop {
            self.given.remove(name);
            self.parents.remove(name);
            if let Some(node) = self.nodes.remove(name) {
                if let Some(conversation) = self.conversations.get_mut(&node.conversation) {
                    conversation.lines.remove(name);
                }
                for parent in &node.parents {
                    if let Some(children) = self.children.get_mut(parent) {
                        children.remove(name);
                        if children.is_empty() {
                            self.children.remove(parent);
                        }
                    }
                    touched.insert((parent.clone(), node.conversation.clone()));
                }
                touched.insert((name.clone(), node.conversation.clone()));
                moved.insert(node.conversation);
            }
            touched.extend(self.resting_on(name));
        }
        for Parsed { given, read } in add {
            let node = match read {
                Read::Line(node) => node,
                Read::Structure(name, parents) => {
                    if !self.given.contains_key(&name) {
                        touched.extend(self.resting_on(&name));
                    }
                    self.parents.entry(name.clone()).or_insert(parents);
                    self.given.entry(name).or_insert(given);
                    continue;
                }
            };
            if self.given.contains_key(&node.name) {
                continue;
            }
            self.given.insert(node.name.clone(), given);
            self.parents.insert(node.name.clone(), node.parents.clone());
            self.conversations
                .entry(node.conversation.clone())
                .or_default()
                .lines
                .insert(node.name.clone());
            for parent in &node.parents {
                self.children
                    .entry(parent.clone())
                    .or_default()
                    .insert(node.name.clone());
                touched.insert((parent.clone(), node.conversation.clone()));
            }
            touched.insert((node.name.clone(), node.conversation.clone()));
            touched.extend(self.resting_on(&node.name));
            moved.insert(node.conversation.clone());
            self.nodes.insert(node.name.clone(), node);
        }
        self.restand(touched);
        for id in &moved {
            match self.conversations.get_mut(id) {
                Some(conversation) if conversation.lines.is_empty() => {
                    self.conversations.remove(id);
                }
                Some(conversation) => conversation.outline = OnceCell::new(),
                None => {}
            }
        }
        moved
    }

    /// `name` beside the conversation of each line held that rests on it.
    fn resting_on(&self, name: &Hash) -> Vec<(Hash, ConversationId)> {
        self.children
            .get(name)
            .into_iter()
            .flatten()
            .filter_map(|child| self.nodes.get(child))
            .map(|child| (name.clone(), child.conversation.clone()))
            .collect()
    }

    /// Each of `touched` placed again among its conversation's heads and what it lacks.
    fn restand(&mut self, touched: BTreeSet<(Hash, ConversationId)>) {
        let Outlines {
            nodes,
            conversations,
            given,
            children,
            ..
        } = self;
        for (name, id) in touched {
            let Some(conversation) = conversations.get_mut(&id) else {
                continue;
            };
            let line = nodes.get(&name).is_some_and(|node| node.conversation == id);
            let held = given.contains_key(&name);
            // Never asked of the genesis, which every line rests on.
            let rested_on = (line || !held)
                && children.get(&name).is_some_and(|children| {
                    children.iter().any(|child| {
                        nodes
                            .get(child)
                            .is_some_and(|child| child.conversation == id)
                    })
                });
            place(&mut conversation.heads, &name, line && !rested_on);
            place(&mut conversation.lacking, &name, !held && rested_on);
        }
    }

    /// Conversation `id`'s frontier; none where no line of it is held.
    #[must_use]
    pub fn frontier(&self, id: &ConversationId) -> Option<Frontier<'_>> {
        let conversation = self.conversations.get(id)?;
        Some(if conversation.lacking.is_empty() {
            Frontier::Closed {
                heads: &conversation.heads,
                lines: conversation.lines.len(),
            }
        } else {
            Frontier::Open(&conversation.lacking)
        })
    }

    /// Every node `scope` reads of the skeleton held and `beside` (nodes not held yet), by name.
    #[must_use]
    pub fn scope<'s>(&'s self, scope: &Scope, beside: &'s [Parsed]) -> BTreeSet<&'s Hash> {
        let beside: BTreeMap<&Hash, &Parsed> =
            beside.iter().map(|node| (node.name(), node)).collect();
        let lines = || {
            self.nodes.values().chain(
                beside
                    .values()
                    .filter_map(|node| node.line())
                    .filter(|line| !self.nodes.contains_key(&line.name)),
            )
        };
        let closure = |ids: &BTreeSet<ConversationId>| {
            let mut under = BTreeSet::new();
            let mut walk: Vec<&Hash> = lines()
                .filter(|line| ids.contains(&line.conversation))
                .map(|line| &line.name)
                .collect();
            while let Some(name) = walk.pop() {
                let parents = match self.parents.get_key_value(name) {
                    Some((name, parents)) => Some((name, parents.as_slice())),
                    None => beside
                        .get_key_value(name)
                        .map(|(name, node)| (*name, node.parents())),
                };
                let Some((name, parents)) = parents else {
                    continue;
                };
                if under.insert(name) {
                    walk.extend(parents);
                }
            }
            under
        };
        match scope {
            Scope::Whole => self.given.keys().chain(beside.keys().copied()).collect(),
            Scope::Objects(names) => names
                .iter()
                .filter_map(|name| {
                    self.given
                        .get_key_value(name)
                        .map(|(name, _)| name)
                        .or_else(|| beside.get_key_value(name).map(|(name, _)| *name))
                })
                .collect(),
            Scope::Conversations(ids) => closure(ids),
            Scope::Naming(keys) => closure(&naming(lines(), keys)),
        }
    }

    /// The conversations whose lines held name one of `keys`.
    #[must_use]
    pub fn naming(&self, keys: &BTreeSet<KeyName>) -> BTreeSet<ConversationId> {
        naming(self.nodes.values(), keys)
    }

    /// The keys the lines held and the line nodes of `beside` retire, given which keys `holds`.
    #[must_use]
    pub fn retired(
        &self,
        beside: &[Parsed],
        holds: &dyn Fn(&KeyName) -> bool,
    ) -> BTreeSet<KeyName> {
        let more = beside
            .iter()
            .filter_map(Parsed::line)
            .filter(|line| !self.nodes.contains_key(&line.name));
        retired(self.nodes.values().chain(more), holds)
    }

    /// The nodes of the newest conversations, each whole with every node its lines rest on, newest
    /// first, up to the first conversation that would take the bytes past `budget`.
    #[must_use]
    pub fn plan(&self, budget: usize) -> Vec<&Hash> {
        let mut newest: Vec<(&ConversationId, Datetime)> = self
            .conversations
            .iter()
            .filter_map(|(id, conversation)| {
                let at = conversation
                    .lines
                    .iter()
                    .filter_map(|name| self.nodes.get(name))
                    .map(|node| node.at)
                    .max()?;
                Some((id, at))
            })
            .collect();
        newest.sort_by(|(a, x), (b, y)| y.cmp(x).then_with(|| a.cmp(b)));
        let mut kept = BTreeSet::new();
        let mut out = Vec::new();
        let mut bytes = 0;
        for (id, _) in newest {
            let more: Vec<&Hash> = self
                .scope(&Scope::Conversations(BTreeSet::from([id.clone()])), &[])
                .into_iter()
                .filter(|name| !kept.contains(*name))
                .collect();
            let size: usize = more
                .iter()
                .filter_map(|name| self.given.get(*name))
                .map(|node| node.size)
                .sum();
            if bytes + size > budget {
                break;
            }
            bytes += size;
            kept.extend(more.iter().copied());
            out.extend(more);
        }
        out
    }

    /// Whether the skeleton holds a node named `name`.
    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.given.contains_key(name)
    }

    /// The name of every node held.
    pub fn names(&self) -> impl Iterator<Item = &Hash> {
        self.given.keys()
    }

    /// The node named `name`, as it was given.
    #[must_use]
    pub fn node(&self, name: &Hash) -> Option<&wire::SkeletonNode> {
        self.given.get(name)
    }

    /// Every conversation's outline, by id, each folded again only if a line of it moved.
    pub fn outlines(&self) -> impl Iterator<Item = (&ConversationId, &Outline)> {
        self.conversations.iter().map(|(id, conversation)| {
            let outline = conversation.outline.get_or_init(|| {
                let lines: Vec<outline::Line<'_>> = conversation
                    .lines
                    .iter()
                    .filter_map(|name| self.nodes.get(name))
                    .map(|node| outline::Line {
                        name: &node.name,
                        at: node.at,
                        parents: &node.parents,
                        keys: &node.keys,
                        step: &node.step,
                        writer: &node.writer,
                    })
                    .collect();
                outline::of_steps(id, &lines)
            });
            (id, outline)
        })
    }
}

/// `name` in `set` exactly when `is`.
fn place(set: &mut BTreeSet<Hash>, name: &Hash, is: bool) {
    if is {
        set.insert(name.clone());
    } else {
        set.remove(name);
    }
}

/// The keys `lines` retire, of those their own conversation names, given which keys `holds`: each
/// key an erasure names, and each key of a reseal of a slot it names; and each seal a reseal was
/// sealed again from, where the reseal's own key is held or a later reseal of it supersedes that
/// key in turn. The box's rule (`transcript::Retired`) read from the steps alone.
fn retired<'n>(
    lines: impl IntoIterator<Item = &'n Node>,
    holds: &dyn Fn(&KeyName) -> bool,
) -> BTreeSet<KeyName> {
    /// One conversation's keys: those it names, those its erasures name, and its reseals, each
    /// its slot's first key, the seal it was sealed from and its own keys.
    #[derive(Default)]
    struct Seen<'n> {
        named: BTreeSet<&'n KeyName>,
        erased: BTreeSet<&'n KeyName>,
        reseals: Vec<(&'n KeyName, &'n KeyName, &'n [KeyName])>,
    }
    let mut by: BTreeMap<&ConversationId, Seen<'n>> = BTreeMap::new();
    for line in lines {
        let seen = by.entry(&line.conversation).or_default();
        seen.named.extend(&line.keys);
        match &line.step {
            Step::Erased { keys } => seen.erased.extend(keys),
            Step::Resealed { of, from } => seen.reseals.push((of, from, &line.keys)),
            Step::Opened
            | Step::Asks
            | Step::Noted
            | Step::Answers
            | Step::Rewound { .. }
            | Step::Proposed
            | Step::Coding { .. }
            | Step::Titled
            | Step::Still
            | Step::Began { .. }
            | Step::Again { .. } => {}
        }
    }
    let mut out = BTreeSet::new();
    for seen in by.values() {
        let erased = seen.erased.iter().copied().chain(
            seen.reseals
                .iter()
                .filter(|(of, _, _)| seen.erased.contains(of))
                .flat_map(|(_, _, keys)| keys.iter()),
        );
        out.extend(erased.filter(|key| seen.named.contains(key)).cloned());
        let mut sealed_from: BTreeMap<&KeyName, Vec<&KeyName>> = BTreeMap::new();
        for (_, from, keys) in &seen.reseals {
            for key in *keys {
                sealed_from.entry(key).or_default().push(from);
            }
        }
        let mut walking: Vec<&KeyName> = seen
            .reseals
            .iter()
            .filter(|(_, _, keys)| keys.iter().any(holds))
            .map(|(_, from, _)| *from)
            .collect();
        let mut superseded = BTreeSet::new();
        while let Some(key) = walking.pop() {
            if superseded.insert(key) {
                walking.extend(sealed_from.get(key).into_iter().flatten());
            }
        }
        out.extend(
            superseded
                .into_iter()
                .filter(|key| seen.named.contains(key))
                .cloned(),
        );
    }
    out
}

/// The conversations whose `lines` name one of `keys`.
fn naming<'n>(
    lines: impl IntoIterator<Item = &'n Node>,
    keys: &BTreeSet<KeyName>,
) -> BTreeSet<ConversationId> {
    lines
        .into_iter()
        .filter(|line| line.keys.iter().any(|key| keys.contains(key)))
        .map(|line| line.conversation.clone())
        .collect()
}
