//! What an `Again` line sets aside, read over descent: every line beneath it that descends from
//! its question, and the question too for an edit. A line concurrent with the question, or with
//! the `Again`, is never set aside. It names no type of this application but [`Rewind`].

use std::collections::BTreeSet;

use suzatary_conversations::secretary::store::Rewind;

/// The positions the `Again` at `again` sets aside, `question` asked again `how`, over lines in
/// an order that extends their causal order, each resting on the positions `rests_on` gives.
#[must_use]
pub fn aside<'a>(
    rests_on: impl Fn(usize) -> &'a [usize],
    question: usize,
    again: usize,
    how: Rewind,
) -> BTreeSet<usize> {
    let mut beneath = BTreeSet::new();
    let mut pending: Vec<usize> = rests_on(again).to_vec();
    while let Some(at) = pending.pop() {
        if beneath.insert(at) {
            pending.extend(rests_on(at));
        }
    }
    // A line descends from the question when it rests on it or on a line that does; the order
    // places every line after what it rests on, so one pass forward finds them all.
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
