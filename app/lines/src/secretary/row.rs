//! What a conversation's row in the list reads of its lines ([`Part::Row`]): its title, its
//! proposals, and its last live turn that says words, each with the reseals and erasures its
//! words are read through. What a row places it by (its kind, when it opened, its turns, its last
//! instant) is its outline's, read before any of its words. The last turn with words is found
//! newest first: the first step reads the newest live turn beside the title and the proposals,
//! and each later step one turn more, each step a part of its own, so a reading over the same
//! lines names the same parts.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_data::event::Hash;
use prodrome_data::literal::Datetime;
use suzatary_conversations::secretary::search::words;
use suzatary_conversations::secretary::store::{Asking, Entry, Said};
use suzatary_memory::secretary::words::Words;

use crate::secretary::held_lines::Held;
#[cfg(doc)]
use crate::secretary::held_lines::Part;
use crate::secretary::outline::Rounds;

/// A row's lines opened, as the conversation's fold reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct RowLines {
    pub title: Words<String>,
    /// Every proposal, in the order made.
    pub proposals: Vec<Asking>,
    /// The last live turn that says words; none where no live turn does.
    pub last: Option<Said>,
}

/// What a reader holds of a row's lines.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Read(Box<RowLines>),
    /// The lines the row's step reads, of which the reader lacks some; `HeldReplica::listed`
    /// keeps only those it lacks. Exactly the row's lines wherever its newest live turn says
    /// words.
    Lacking(BTreeSet<Hash>),
    /// A print among them is no line of the conversation, or a line is not the line its step says.
    Unread,
}

/// The row of the conversation `rounds` outlines, the lines of its `step`th step (`0` the first)
/// opened by `open`.
pub fn read(
    rounds: &Rounds,
    mut open: impl FnMut(usize, &BTreeSet<Hash>) -> Held<BTreeMap<Hash, Entry>>,
) -> Row {
    let base = std::iter::once(rounds.title())
        .chain(rounds.proposals().iter().map(|proposed| &proposed.line));
    let mut newest = rounds.newest();
    let mut tried: Vec<&Hash> = newest.next().into_iter().collect();
    let mut step = 0;
    loop {
        let objects = rounds.reads(base.clone().chain(tried.iter().copied()));
        let opened = match open(step, &objects) {
            Held::Opened(opened) => opened,
            Held::Unread => return Row::Unread,
            Held::Lacking(_) => return Row::Lacking(objects),
        };
        let said = tried
            .last()
            .and_then(|line| opened.get(*line))
            .and_then(Said::of)
            .filter(|said| words(&said.turn).is_some());
        if said.is_some() {
            return lines(rounds, &opened, said);
        }
        match newest.next() {
            Some(line) => tried.push(line),
            None => return lines(rounds, &opened, None),
        }
        step += 1;
    }
}

/// The row's lines from those `opened`.
fn lines(rounds: &Rounds, opened: &BTreeMap<Hash, Entry>, last: Option<Said>) -> Row {
    let title = opened.get(rounds.title()).and_then(Entry::title);
    let proposals: Option<Vec<Asking>> = rounds
        .proposals()
        .iter()
        .enumerate()
        .map(|(index, proposed)| {
            let Entry::Proposed {
                at,
                object,
                revises,
            } = opened.get(&proposed.line)?
            else {
                return None;
            };
            Some(Asking {
                index,
                at: *at,
                object: object.clone(),
                after_turn: proposed.after_turn,
                revises: *revises,
            })
        })
        .collect();
    match (title, proposals) {
        (Some(title), Some(proposals)) => Row::Read(Box::new(RowLines {
            title,
            proposals,
            last,
        })),
        (None, _) | (_, None) => Row::Unread,
    }
}

/// When a row of `rounds` says its conversation was last spoken in: its last turn, set aside or
/// not, else its opening.
#[must_use]
pub fn last_at(rounds: &Rounds) -> Datetime {
    rounds.said().last().copied().unwrap_or(rounds.opened())
}
