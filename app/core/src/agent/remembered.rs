//! The secretary's memory as the agent's `context` carries it, within what the answer has left:
//! the newest facts that fit, a sentence counting the rest, then the record in the owner's words
//! alone (`shown_record::told`), as much of it as fits beside them. The answer always leaves it
//! room for [`TOO_BIG`], so it never goes silent.

use crate::refusal::Refusal;
use crate::secretary::memory::Memory;
use crate::secretary::shown_record::{OwnersRecord, told};

use super::reads::wire_len;

/// What an empty memory reads as.
const NOTHING: &str = "The secretary remembers nothing yet.";

/// What a memory that does not fit at all reads as, the least room the answer leaves it.
pub(crate) const TOO_BIG: &str =
    "Nothing of the memory fits this answer: recall(query) and zoom(handle) read it.";

/// What a memory that did not read reads as, when why does not fit.
const UNREAD: &str = "The memory could not be read.";

/// `memory` and `record` as one string costing at most `room` bytes on the wire.
pub(crate) fn remembered(
    memory: Result<&Memory, &Refusal>,
    record: Result<&OwnersRecord, &Refusal>,
    room: usize,
) -> String {
    let fits = |text: &String| wire_len(text) <= room;
    let memory = match memory {
        Ok(memory) => memory,
        Err(refused) => {
            let why = format!("The memory could not be read: {}", refused.message());
            return if fits(&why) { why } else { UNREAD.to_owned() };
        }
    };
    let mut newest = memory.facts.clone();
    newest.sort_by_key(|fact| std::cmp::Reverse(fact.id));
    let told = |kept: usize, view: usize| {
        let shown = Memory {
            facts: newest[..kept].to_vec(),
            forgotten: Vec::new(),
            version: memory.version,
        };
        let told = told(&shown, record, view);
        let all = newest.len();
        match (told.is_empty(), all - kept) {
            (true, 0) => NOTHING.to_owned(),
            (false, 0) => told.text(),
            (_, left) if left == all => format!(
                "(None of the {all} facts fits here: recall(query) finds them.)\n\n{}",
                told.text()
            ),
            (_, left) => format!(
                "(The {left} oldest of the {all} facts are not shown here: recall(query) finds \
                 them.)\n\n{}",
                told.text()
            ),
        }
    };
    if !fits(&told(0, 0)) {
        return TOO_BIG.to_owned();
    }
    // The facts come first: the most of the newest that fit beside no record at all.
    let kept = largest(newest.len(), |kept| fits(&told(kept, 0)));
    // Then the record, in what they leave.
    let view = largest(room, |view| fits(&told(kept, view)));
    told(kept, view)
}

/// The largest `n` in `0..=most` that `holds`, by bisection, for a `holds` that holds at 0: the
/// answer always holds, and is the largest when `holds` is monotone.
fn largest(most: usize, holds: impl Fn(usize) -> bool) -> usize {
    let (mut low, mut high) = (0, most);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if holds(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}
