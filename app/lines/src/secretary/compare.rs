//! What a deploy of the lines' new readings changes for a store, read on a copy of it before the
//! deploy (`suzatary secretary owed --compare`). It opens no words, so it needs no key.

use prodrome_core::schema::Schema as _;

use crate::secretary::conversations::ReadConversations;
use crate::secretary::held_lines::HeldLines;
use crate::secretary::line_event::LineEvent;
use crate::secretary::opened::Opened;
use crate::secretary::sequence;
use crate::secretary::transcript::{self, Transcript};
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, NotAConversation};
use suzatary_conversations::secretary::turn_shape::TurnShape;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_memory::secretary::words::Gone;
use suzatary_store::store::{Gets, Reads, StoreError};

/// One conversation as the two readings see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compared {
    /// The turns owed, by index.
    pub owed: Vec<usize>,
    /// The turns a restart now would answer ([`Transcript::interrupted`]), by index.
    pub resumed: Vec<usize>,
    pub order: Order,
}

/// Whether the turns read in [`sequence::causal`]'s order, the one before the box's lines were
/// the spine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Kept,
    /// The first index whose turn another takes the place of.
    Moves {
        from: usize,
    },
}

/// A conversation compared, or why its lines are none.
pub type Read = Result<Compared, NotAConversation>;

/// Every conversation of `lines`, newest id first, compared at `now`.
///
/// # Errors
/// The store unreadable.
pub fn compare(
    lines: &dyn Reads<LineEvent>,
    now: Datetime,
) -> Result<Vec<(ConversationId, Read)>, StoreError> {
    let opened = Opened::default();
    let read = ReadConversations {
        lines,
        keys: &NoKeys,
        opened: &opened,
    };
    let since = prodrome_core::fpl::instant_of(now) - transcript::RESUMED;
    Ok(read
        .fold(HeldLines::transcripts)?
        .into_iter()
        .map(|(id, read)| {
            let compared = read.map(|read| Compared {
                owed: read.owed().into_iter().map(|(turn, _)| turn).collect(),
                resumed: read
                    .owed()
                    .into_iter()
                    .filter(|(_, said)| {
                        prodrome_core::fpl::instant_of(said.at) >= since && answered(&said.turn)
                    })
                    .map(|(turn, _)| turn)
                    .collect(),
                order: order(&read),
            });
            (id, compared)
        })
        .collect())
}

/// Whether `read`'s turns, in its order, are in [`sequence::causal`]'s.
fn order(read: &Transcript) -> Order {
    let lines = read.lines();
    let parents: Vec<Vec<_>> = lines
        .iter()
        .map(|line| {
            line.rests_on
                .iter()
                .map(|&at| lines[at].name.clone())
                .collect()
        })
        .collect();
    let nodes: Vec<_> = lines
        .iter()
        .zip(&parents)
        .map(|(line, parents)| sequence::Node {
            name: &line.name,
            at: line.line.at(),
            parents,
            writer: crate::wire::lines::LineWriter::of(line.line.actor()),
        })
        .collect();
    let turn = |at: &usize| matches!(lines[*at].entry, Entry::Said { .. } | Entry::Stopped { .. });
    let before = sequence::causal(&nodes)
        .into_iter()
        .map(|(at, _)| at)
        .filter(turn);
    let now = (0..lines.len()).filter(turn);
    match before.zip(now).position(|(was, is)| was != is) {
        Some(from) => Order::Moves { from },
        None => Order::Kept,
    }
}

/// Whether a restart answers an owed `turn` as the box reads it, with the key of every slot no
/// erasure names: a person's, a model's or a verb's. A turn whose words are gone here only for
/// want of their key is the turn its shape says.
fn answered(turn: &Turn) -> bool {
    match turn {
        Turn::Person { .. } | Turn::Model { .. } | Turn::Tool { .. } => true,
        Turn::Act { .. } => false,
        Turn::Erased { shape, words } => {
            words
                .iter()
                .all(|words| !matches!(words.gone(), Some(Gone::Erased { .. })))
                && match shape {
                    TurnShape::Person { .. } | TurnShape::Model { .. } | TurnShape::Tool { .. } => {
                        true
                    }
                    TurnShape::Act { .. } => false,
                }
        }
    }
}

/// The keys of a reading that opens no words.
struct NoKeys;

impl Gets<KeyName, TextKey> for NoKeys {
    fn get(&self, _: &KeyName) -> Result<Option<TextKey>, StoreError> {
        Ok(None)
    }
}
