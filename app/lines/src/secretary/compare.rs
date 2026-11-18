//! What a deploy of the lines' new readings changes for a store, read on a copy of it before the
//! deploy (`suzatary secretary owed --compare`). It opens no words, so it needs no key.

use prodrome_core::schema::Schema as _;

use crate::secretary::claim;
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

/// One conversation as the readings before and after the claim see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compared {
    /// The turns owed before the claim, by index: every live turn no live close rests on.
    pub owed: Vec<usize>,
    /// The turns a start before the claim sent: every turn owed before the claim, in a
    /// conversation where a person's, a model's or a verb's owed turn was said within the bound.
    pub resumed: Vec<usize>,
    /// The turns the claims' reading owes ([`Transcript::owed`]).
    pub owed_by_claims: Vec<usize>,
    /// The turns the first claim step at `now` claims, every run of the store read as gone, as it
    /// is at a first start.
    pub claimed: Vec<usize>,
    /// The waking turns queued past the bound, none within it, so the next claim step, at the
    /// next write that asks nothing, writes the box's sentence under them.
    pub past: Vec<usize>,
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
    Ok(read
        .fold(HeldLines::transcripts)?
        .into_iter()
        .map(|(id, read)| (id, read.map(|read| compared(&read, now))))
        .collect())
}

/// One conversation compared at `now`.
fn compared(read: &Transcript, now: Datetime) -> Compared {
    let owed = owed_before_claims(read);
    let since = prodrome_core::fpl::instant_of(now) - transcript::ANSWERED_WITHIN;
    let interrupted = owed.iter().any(|&turn| {
        let said = &read.stored().turns[turn];
        prodrome_core::fpl::instant_of(said.at) >= since && answered(&said.turn)
    });
    let covers = read.resumes(now, |_| true, claim::Retry::Allowed);
    let turns = |lines: &[usize]| -> Vec<usize> {
        (0..read.stored().turns.len())
            .filter(|&turn| read.line_of(turn).is_some_and(|line| lines.contains(&line)))
            .collect()
    };
    let claimed = turns(&covers);
    let asked = read.asked(now);
    let past = match read.claims().due(&asked, |_| true, claim::Retry::Allowed) {
        claim::Due::Close => turns(
            &asked
                .iter()
                .filter(|asked| {
                    asked.wakes == claim::Wakes::Model
                        && asked.bound == claim::Bound::Past
                        && read.claims().standing(asked.at) == claim::Standing::Queued
                })
                .map(|asked| asked.at)
                .collect::<Vec<_>>(),
        ),
        claim::Due::Nothing | claim::Due::Claim(_) | claim::Due::Retry(_) => Vec::new(),
    };
    Compared {
        resumed: if interrupted {
            owed.clone()
        } else {
            Vec::new()
        },
        owed,
        owed_by_claims: read.owed().into_iter().map(|(turn, _)| turn).collect(),
        claimed,
        past,
        order: order(read),
    }
}

/// The turns owed before the claim: every live turn no live closing turn (an answer with no
/// calls, or a stop) rests on or is.
fn owed_before_claims(read: &Transcript) -> Vec<usize> {
    let stored = read.stored();
    let lines = read.lines();
    let mut settled = vec![false; lines.len()];
    let mut pending: Vec<usize> = (0..stored.turns.len())
        .filter(|&turn| stored.is_live(turn) && stored.closes(turn))
        .filter_map(|turn| read.line_of(turn))
        .collect();
    while let Some(line) = pending.pop() {
        if !settled[line] {
            settled[line] = true;
            pending.extend(&lines[line].rests_on);
        }
    }
    stored
        .live()
        .into_iter()
        .filter(|(turn, _)| read.line_of(*turn).is_some_and(|line| !settled[line]))
        .map(|(turn, _)| turn)
        .collect()
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

/// Whether a start before the claim answers an owed `turn` as the box reads it, with the key of every slot no
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
