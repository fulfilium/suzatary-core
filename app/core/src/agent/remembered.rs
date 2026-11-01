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

/// `memory` and `record` as one string costing at most `room` bytes on the wire: the newest facts
/// that fit, then the newest lines of the record's view within `room` that fit beside them. The
/// view is built once; the searches for how many facts and lines fit only print text.
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
    let views = crate::secretary::digest::views();
    // The view is built once, within all the room there is; each probe below fits its lines.
    let lines = record.map(|record| record.lines(room));
    let mut newest = memory.facts.clone();
    newest.sort_by_key(|fact| std::cmp::Reverse(fact.id));
    let told = |kept: usize, view: usize| {
        let shown = Memory {
            facts: newest[..kept].to_vec(),
            forgotten: Vec::new(),
            merged: Vec::new(),
            version: memory.version,
        };
        let told = told(&shown, lines.as_deref().map_err(|refused| *refused), view);
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
    let answer = told(kept, view);
    crate::timings::counted(
        crate::timings::Phase::Remembered,
        "views",
        crate::secretary::digest::views() - views,
    );
    answer
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::secretary::live_shape::{Shape, Summaries};

    fn nothing_remembered() -> Memory {
        Memory {
            facts: Vec::new(),
            forgotten: Vec::new(),
            merged: Vec::new(),
            version: 0,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 16, ..ProptestConfig::default() })]

        /// THE RECORD IS ONE VIEW AND ITS NEWEST LINES THAT FIT: over a generated memory, partly
        /// or wholly summarised, the answer builds one view within its room, fits the room, and
        /// shows the newest of that view's lines, as many as fit and no fewer.
        #[test]
        fn the_record_is_one_view_and_its_newest_lines_that_fit(
            lines in 4usize..120,
            conversations in 1usize..4,
            answer_bytes in 10usize..2_000,
            summarised in prop::option::of(0usize..40),
            room in 100usize..12_000,
        ) {
            let stores = Shape {
                lines,
                conversations,
                answer_bytes,
                summaries: summarised.map_or(Summaries::Every, Summaries::First),
            }
            .build();
            let memory = stores.recollection(room);
            let (facts, record) = (memory.facts(), memory.record());
            let record = record.expect("the record reads");
            let before = crate::secretary::digest::views();
            let answer = remembered(facts.as_ref(), Ok(&record), room);
            prop_assert_eq!(crate::secretary::digest::views() - before, 1, "one view");
            prop_assert!(wire_len(&answer) <= room, "{} bytes in {room}", wire_len(&answer));

            let all = record.lines(room);
            let told_with = |newest: usize| {
                let bytes = all[all.len() - newest..].iter().map(|line| line.len() + 1).sum();
                told(&nothing_remembered(), Ok(&all), bytes).text()
            };
            if answer == TOO_BIG {
                prop_assert!(wire_len(&told_with(0)) > room);
                return Ok(());
            }
            let shown = (0..=all.len())
                .rev()
                .find(|&newest| {
                    newest == 0
                        || answer.ends_with(&format!("\n{}", all[all.len() - newest..].join("\n")))
                })
                .expect("none is always a suffix");
            if shown < all.len() {
                prop_assert!(wire_len(&told_with(shown + 1)) > room, "one more line fits");
            }
        }
    }
}
