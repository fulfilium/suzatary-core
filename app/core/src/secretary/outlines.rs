//! The skeleton's lines by conversation, each conversation's outline kept until a line of it comes
//! or goes: what the stream counts and chooses its rounds by, with no line's object read.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use prodrome_core::fpl;
use prodrome_core::literal::Datetime;

use crate::secretary::outline::{self, Outline};
use crate::secretary::step::Step;
use crate::secretary::store::ConversationId;
use crate::wire;

/// The skeleton's lines held, by conversation.
#[derive(Default)]
pub struct Outlines {
    nodes: BTreeMap<Hash, Node>,
    conversations: BTreeMap<ConversationId, Conversation>,
}

/// One line node, parsed once where it arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    name: Hash,
    conversation: ConversationId,
    at: Datetime,
    parents: Vec<Hash>,
    step: Step,
}

/// A skeleton node that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a node of the lines does not read: {0}")]
pub struct NotANode(String);

impl Node {
    /// The line `node` is, or `None` for a node that carries no line.
    ///
    /// # Errors
    /// [`NotANode`] for a name, a conversation or an instant that does not read.
    pub fn of(node: wire::SkeletonNode) -> Result<Option<Self>, NotANode> {
        let wire::NodeHeader::Line {
            conversation,
            at,
            step,
            ..
        } = node.header
        else {
            return Ok(None);
        };
        let name = |text: String| Hash::new(text).map_err(|e| NotANode(e.to_string()));
        Ok(Some(Self {
            name: name(node.hash)?,
            conversation: ConversationId::new(conversation).map_err(|e| NotANode(e.to_string()))?,
            at: fpl::parse_iso(&at)
                .map_err(|e| NotANode(e.to_string()))
                .and_then(|at| fpl::datetime_of(at).map_err(|e| NotANode(e.to_string())))?,
            parents: node
                .parents
                .into_iter()
                .map(name)
                .collect::<Result<_, _>>()?,
            step,
        }))
    }
}

/// One conversation's lines, and its outline once folded, emptied whenever a line comes or goes.
struct Conversation {
    lines: BTreeSet<Hash>,
    outline: OnceCell<Outline>,
}

impl Outlines {
    /// The lines `add` held and the names `drop` let go: each conversation a line of which came
    /// or went is folded again at its next reading.
    pub fn hold(&mut self, add: Vec<Node>, drop: &[Hash]) {
        let mut moved = BTreeSet::new();
        for name in drop {
            if let Some(node) = self.nodes.remove(name) {
                if let Some(conversation) = self.conversations.get_mut(&node.conversation) {
                    conversation.lines.remove(name);
                }
                moved.insert(node.conversation);
            }
        }
        for node in add {
            if self.nodes.contains_key(&node.name) {
                continue;
            }
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

    /// Every conversation's outline, by id, each folded again only if a line of it moved.
    pub fn outlines(&self) -> impl Iterator<Item = (&ConversationId, &Outline)> {
        self.conversations.iter().map(|(id, conversation)| {
            let outline = conversation.outline.get_or_init(|| {
                let lines: Vec<outline::Line<'_>> = conversation
                    .lines
                    .iter()
                    .filter_map(|name| self.nodes.get(name))
                    .map(|node| (&node.name, node.at, node.parents.as_slice(), node.step))
                    .collect();
                outline::of_steps(id, &lines)
            });
            (id, outline)
        })
    }
}
