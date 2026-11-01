//! The skeleton's lines by conversation, each conversation's outline kept until a line of it comes
//! or goes: what the stream counts and chooses its rounds by, with no line's object read. Beside
//! them, every node the skeleton holds as it was given, which says what the store holds and is
//! what a snapshot of it prints.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;

use crate::secretary::locked_text::KeyName;
use crate::secretary::outline::{self, Outline};
use crate::secretary::step::Step;
use crate::secretary::store::ConversationId;
use crate::wire;

/// The skeleton's lines held, by conversation, and every node it holds as it was given.
#[derive(Default)]
pub struct Outlines {
    nodes: BTreeMap<Hash, Node>,
    conversations: BTreeMap<ConversationId, Conversation>,
    given: BTreeMap<Hash, wire::SkeletonNode>,
}

/// One skeleton node, parsed once where it arrives, beside the node as it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    given: wire::SkeletonNode,
    read: Read,
}

/// What a node is: a line, or a node that carries none, such as the genesis, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    Line(Node),
    Structure(Hash),
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
            wire::NodeHeader::Structure { .. } => Read::Structure(name(&node.hash)?),
            wire::NodeHeader::Line {
                conversation,
                at,
                keys,
                step,
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
            }),
        };
        Ok(Parsed { given: node, read })
    }
}

/// One conversation's lines, and its outline once folded, emptied whenever a line comes or goes.
struct Conversation {
    lines: BTreeSet<Hash>,
    outline: OnceCell<Outline>,
}

impl Outlines {
    /// The nodes `add` held and the names `drop` let go: each conversation a line of which came
    /// or went is folded again at its next reading.
    pub fn hold(&mut self, add: Vec<Parsed>, drop: &[Hash]) {
        let mut moved = BTreeSet::new();
        for name in drop {
            self.given.remove(name);
            if let Some(node) = self.nodes.remove(name) {
                if let Some(conversation) = self.conversations.get_mut(&node.conversation) {
                    conversation.lines.remove(name);
                }
                moved.insert(node.conversation);
            }
        }
        for Parsed { given, read } in add {
            let node = match read {
                Read::Line(node) => node,
                Read::Structure(name) => {
                    self.given.entry(name).or_insert(given);
                    continue;
                }
            };
            if self.given.contains_key(&node.name) {
                continue;
            }
            self.given.insert(node.name.clone(), given);
            self.conversations
                .entry(node.conversation.clone())
                .or_insert_with(|| Conversation {
                    lines: BTreeSet::new(),
                    outline: OnceCell::new(),
                })
                .lines
                .insert(node.name.clone());
            moved.insert(node.conversation.clone());
            self.nodes.insert(node.name.clone(), node);
        }
        for id in moved {
            match self.conversations.get_mut(&id) {
                Some(conversation) if conversation.lines.is_empty() => {
                    self.conversations.remove(&id);
                }
                Some(conversation) => conversation.outline = OnceCell::new(),
                None => {}
            }
        }
    }

    /// Whether the skeleton holds a node named `name`.
    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.given.contains_key(name)
    }

    /// Every node held, as it was given, by name.
    pub fn given(&self) -> impl Iterator<Item = &wire::SkeletonNode> {
        self.given.values()
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
                    })
                    .collect();
                outline::of_steps(id, &lines)
            });
            (id, outline)
        })
    }
}
