//! The step every change to the redaction rules is followed by: each slot of the conversations'
//! lines whose words the box can open under older rules only is sealed again, scrubbed under the
//! current rules, under a key of its own, and the key of the seal it replaces is destroyed.
//!
//! Nothing is deleted. A reseal is one more line of its conversation, `Resealed(of, from)`, whose
//! one slot holds the words of the slot `of` names, sealed again from the seal locked under
//! `from` ([`LineEvent::reseal`]). A reseal that opens with a key held here supersedes `from`: a
//! reader reads the slot from that reseal and never opens the seal it replaced, whether or not
//! its key is still held ([`transcript::Through`]), and the one sweep destroys that key as it
//! destroys an erased one, forgetting first what its words were rendered to ([`transcript::dead`]).
//! A slot no reseal supersedes is read from its own seal while its key opens it. A store
//! restored with its reseal lines and without their keys destroys no old key: it reads the old
//! seals, and its next run seals them again. The store only grows, and its genesis does
//! not move, so a tab keeps its replica and forgets the superseded keys as it forgets erased ones.
//!
//! A run appends a reseal for each stale slot, then sweeps. A run stopped at any point leaves
//! every slot readable with the same words: a slot not yet resealed reads its old seal, scrubbed
//! under the current rules as it opens, and a slot resealed whose old key still stands opens to
//! the same words either way. The next run reseals only what is still stale, and its sweep
//! destroys what the last one left. The laws are the cli tests
//! `migrate_scrub_reseals_every_stale_slot_once`, `migrate_scrub_stopped_and_run_again_reads_the_same`
//! and `migrate_scrub_restored_without_the_reseals_keys_destroys_no_old_key`.
//!
//! Its one residue: the old ciphertext stays in the data repository's history, opened by no key
//! the box keeps once its sweep has run; a key kept in an earlier vault snapshot still opens it.

use std::collections::BTreeMap;

use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;

use crate::secretary::conversations::Conversations;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::KeyName;
use crate::secretary::store::ConversationId;
use crate::secretary::transcript;
use crate::secretary::words::Words;
use crate::store::StoreError;

/// How the lines' slots stand against the current rules.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    /// The lines that hold words of their own (every line but a reseal).
    pub lines: usize,
    /// The slots a seal under the current rules opens.
    pub current: usize,
    /// The slots a seal under older rules opens that nothing here supersedes: what a run seals
    /// again.
    pub stale: usize,
    /// The slots an erasure names.
    pub erased: usize,
    /// The slots no seal opens: their keys are lost.
    pub unopened: Vec<Place>,
    /// The slots whose words open under older rules and do not pass the current rules' door, so
    /// they are withheld and cannot be sealed again.
    pub withheld: Vec<Place>,
}

/// Where a slot is: its conversation, its line's instant, its place in the line and its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The conversation.
    pub conversation: ConversationId,
    /// The line's instant.
    pub at: Datetime,
    /// The slot in the line.
    pub slot: usize,
    /// The key of the slot as first written.
    pub key: KeyName,
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::literal::Literal as _;
        write!(
            f,
            "{} at {}, slot {}, key {}",
            self.conversation,
            prodrome_core::literal::print_literal(&self.at.to_value()),
            self.slot,
            self.key
        )
    }
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The lines as they stood before it.
    pub census: Census,
    /// The reseals it appended.
    pub resealed: usize,
    /// The keys its sweep destroyed: superseded seals', and any erasure owed.
    pub destroyed: usize,
}

/// One slot as it stands, and, when it is stale, the words to seal again and the key of each
/// seal they supersede.
enum Stands {
    Erased,
    Current,
    Stale {
        from: Vec<KeyName>,
        words: LineWords,
    },
    Withheld,
    Unopened,
}

/// Every slot of every line holding words of its own, by its conversation, with its place.
///
/// A slot is stale while any seal of it that opens here is under older rules and no reseal that
/// opens here supersedes it: each such seal is sealed again, from the words a reader reads (its
/// own seal while that opens, else the first reseal that does), so a seal a sweep kept is
/// superseded at last whatever the rules did since.
fn slots(conversations: Conversations<'_>) -> Result<Vec<(Place, Stands)>, StoreError> {
    let objects = conversations.lines.objects()?;
    let mut by: BTreeMap<&ConversationId, Vec<(&Hash, &LineEvent)>> = BTreeMap::new();
    for (name, object) in &objects {
        if let Some(line) = object.event() {
            by.entry(line.conversation())
                .or_default()
                .push((name, line));
        }
    }
    let mut found = Vec::new();
    for lines in by.into_values() {
        let erased = transcript::erasures(lines.iter().copied());
        let reseals = transcript::reseals(lines.iter().copied());
        let mut held = BTreeMap::new();
        for (_, line) in &lines {
            for own in line.keys() {
                if let Some(key) = conversations.keys.get(own)? {
                    held.insert(own.clone(), key);
                }
            }
        }
        let superseded = transcript::superseded(lines.iter().copied(), &|name: &KeyName| {
            conversations.key(name)
        });
        for (_, line) in &lines {
            if matches!(line.shape(), LineShape::Resealed { .. }) {
                continue;
            }
            for (slot, key) in line.keys().enumerate() {
                let place = Place {
                    conversation: line.conversation().clone(),
                    at: line.at(),
                    slot,
                    key: key.clone(),
                };
                if erased.contains_key(key) {
                    found.push((place, Stands::Erased));
                    continue;
                }
                // Every seal of the slot that opens, in a reader's order: its own, then its
                // reseals, each with whether it is under the current rules.
                let seals =
                    std::iter::once((*line, slot, key))
                        .chain(
                            reseals.get(key).into_iter().flatten().filter_map(|reseal| {
                                reseal.keys().next().map(|own| (*reseal, 0, own))
                            }),
                        )
                        .filter_map(|(seal, at, own)| {
                            let opened = seal.read(at, held.get(own)?)?;
                            let current = seal
                                .scrubbed_under()
                                .nth(at)
                                .is_some_and(|under| under.is_current(()));
                            Some((seal, at, own, opened, current))
                        })
                        .collect::<Vec<_>>();
                let from: Vec<KeyName> = seals
                    .iter()
                    .filter(|(.., current)| !current)
                    .map(|(_, _, own, ..)| (*own).clone())
                    .filter(|own| !superseded.contains(own))
                    .collect();
                let stands = match seals.first() {
                    None => Stands::Unopened,
                    Some(_) if from.is_empty() => {
                        if seals.iter().any(|(.., current)| *current) {
                            Stands::Current
                        } else {
                            // Every seal that opens is superseded by one that opens and withholds.
                            Stands::Withheld
                        }
                    }
                    // The words as locked, which `LineEvent::reseal` scrubs once, as a reader of
                    // this seal does: the reseal then reads exactly as the seal it replaces.
                    Some((seal, at, own, Words::Kept { .. }, _)) => {
                        match seal.unscrubbed(*at, &held[*own]) {
                            Some(words) => Stands::Stale { from, words },
                            None => Stands::Unopened,
                        }
                    }
                    Some((_, _, _, Words::Gone { .. }, _)) => Stands::Withheld,
                };
                found.push((place, stands));
            }
        }
    }
    Ok(found)
}

/// How the lines `conversations` holds stand against the current rules.
///
/// # Errors
/// The stores' own.
pub fn census(conversations: Conversations<'_>) -> Result<Census, StoreError> {
    let slots = slots(conversations)?;
    let mut census = Census {
        lines: conversations
            .lines
            .events()?
            .iter()
            .filter(|line| !matches!(line.shape(), LineShape::Resealed { .. }))
            .count(),
        ..Census::default()
    };
    for (place, stands) in &slots {
        match stands {
            Stands::Erased => census.erased += 1,
            Stands::Current => census.current += 1,
            Stands::Stale { .. } => census.stale += 1,
            Stands::Withheld => census.withheld.push(place.clone()),
            Stands::Unopened => census.unopened.push(place.clone()),
        }
    }
    Ok(census)
}

/// Seal every stale slot again under the current rules at `at`, one reseal line per seal it
/// supersedes, holding
/// the lines' write lock; answers how many it appended. Each reseal's key is kept before its line
/// is appended, as every write does. The keys it supersedes stand until [`run`]'s sweep.
///
/// # Errors
/// The stores' own, and a reseal the door refuses.
pub fn reseal(conversations: Conversations<'_>, at: Datetime) -> Result<usize, StoreError> {
    let _held = conversations.lines.locked()?;
    let refused = |why: &dyn std::fmt::Display| StoreError::not_appended("lines", why);
    let mut held: BTreeMap<ConversationId, u32> = BTreeMap::new();
    for line in conversations.lines.events()? {
        *held.entry(line.conversation().clone()).or_default() += 1;
    }
    let mut resealed = 0;
    for (place, stands) in slots(conversations)? {
        let Stands::Stale { from, words } = stands else {
            continue;
        };
        let count = held.entry(place.conversation.clone()).or_default();
        for from in from {
            let (line, key) = LineEvent::reseal(
                place.conversation.clone(),
                at,
                transcript::writer(),
                *count,
                place.key.clone(),
                from,
                &words,
                conversations.entropy,
            )
            .map_err(|why| refused(&why))?;
            conversations.keys.set(&key.name(), &key)?;
            conversations.lines.append(line)?;
            *count += 1;
            resealed += 1;
        }
    }
    Ok(resealed)
}

/// Seal every stale slot again ([`reseal`]), then sweep, destroying each key a reseal
/// supersedes. Running it again appends nothing and destroys nothing more.
///
/// # Errors
/// The stores' own, a reseal the door refuses, and a sweep that kept keys (the next run destroys
/// them).
pub fn run(conversations: Conversations<'_>, at: Datetime) -> Result<Done, StoreError> {
    let census = census(conversations)?;
    let resealed = reseal(conversations, at)?;
    let destroyed = transcript::sweep(conversations)?;
    Ok(Done {
        census,
        resealed,
        destroyed,
    })
}
