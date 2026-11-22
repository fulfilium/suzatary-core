//! What an `Again` line sets aside, read over descent: every line beneath it that descends from
//! its question, and the question too for an edit; a line concurrent with the question, or with
//! the `Again`, is never set aside, and an edit's `Again` sets nothing aside until its own
//! question takes it up.

use std::collections::BTreeSet;

use suzatary_conversations::secretary::store::Rewind;

/// The positions the `Again` at `again` sets aside, `question` asked again `how`, over `len` lines
/// in causal order, each resting on what `rests_on` gives; `takes_up(line, again)`: whether a line
/// resting on it alone is the edit's own question.
#[must_use]
pub fn aside<'a>(
    rests_on: impl Fn(usize) -> &'a [usize],
    takes_up: impl Fn(usize, usize) -> bool,
    len: usize,
    question: usize,
    again: usize,
    how: Rewind,
) -> BTreeSet<usize> {
    if how == Rewind::Edit
        && !(again + 1..len).any(|at| rests_on(at) == [again] && takes_up(at, again))
    {
        return BTreeSet::new();
    }
    let mut beneath = BTreeSet::new();
    let mut pending: Vec<usize> = rests_on(again).to_vec();
    while let Some(at) = pending.pop() {
        if beneath.insert(at) {
            pending.extend(rests_on(at));
        }
    }
    // The order places every line after what it rests on, so one pass forward finds them all.
    let mut descends: BTreeSet<usize> = BTreeSet::from([question]);
    for at in question + 1..again {
        if rests_on(at).iter().any(|parent| descends.contains(parent)) {
            descends.insert(at);
        }
    }
    if how == Rewind::Regenerate {
        descends.remove(&question);
    }
    descends.intersection(&beneath).copied().collect()
}
