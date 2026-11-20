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
//! A run opens and scrubs each stale slot once, appends its reseals in batches, each batch's
//! keys durable before any of its lines is placed, then sweeps. A run stopped at any point leaves
//! every slot readable with the same words: a slot not yet resealed reads its old seal, scrubbed
//! under the current rules as it opens, and a slot resealed whose old key still stands opens to
//! the same words either way. The next run reseals only what is still stale, and its sweep
//! destroys what the last one left. The laws are the cli tests
//! `migrate_scrub_reseals_every_stale_slot_once`, `migrate_scrub_stopped_and_run_again_reads_the_same`,
//! `migrate_scrub_stopped_at_any_step_of_a_batch_and_run_again_reads_the_same`,
//! `migrate_scrub_scrubs_each_stale_slot_once`, `migrate_scrub_trusts_the_reading_under_the_lock`
//! and `migrate_scrub_restored_without_the_reseals_keys_destroys_no_old_key`.
//!
//! Its one residue: the old ciphertext stays in the data repository's history, opened by no key
//! the box keeps once its sweep has run; a key kept in an earlier vault snapshot still opens it.

use std::collections::BTreeMap;

use prodrome_core::event::Hash;
use prodrome_core::literal::Datetime;
use suzatary_base::answer::Renderable;
use suzatary_held::redact::Scrubbed;

use crate::secretary::conversations::Conversations;
use crate::secretary::line_event::LineEvent;
use crate::secretary::line_shape::LineShape;
use crate::secretary::transcript;
use suzatary_base::conversation_id::ConversationId;
use suzatary_memory::secretary::line_words::LineWords;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_memory::secretary::words::{Gone, Words};
use suzatary_store::store::{Graph, StoreError};

/// How many reseals one batch writes: the command's. A batch's keys are made durable together
/// before its lines are placed, so a crash loses at most one batch's work.
pub const BATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(1000).expect("not zero");

/// How the lines' slots stand against the current rules. A seal counts as opening where its key
/// is held, and only a stale slot's seal is decrypted: a seal under the current rules whose held
/// key does not open it is counted current, and reads as missing.
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
        use prodrome_core::literal::codec::Literal as _;
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

/// One slot as the keys held say it stands, before any of its words is opened: a seal opens
/// where its key is held.
enum Stands<'g> {
    Erased,
    Current,
    /// Every seal that opens is superseded, by one whose words the current rules withhold.
    Withheld,
    Unopened,
    /// Read from slot `slot` of `seal`, the line named `name`, under `key`, as a reader reads it,
    /// and sealed again from each of `from`: the seals under older rules that nothing here
    /// supersedes.
    Stale {
        name: &'g Hash,
        seal: &'g LineEvent,
        slot: usize,
        key: TextKey,
        from: Vec<KeyName>,
    },
}

/// A stale slot opened once and scrubbed once: all a reseal of it is a function of, beside its
/// key and the rules.
enum Opened {
    /// Its words through the current rules' door, and the blocks its seal's words were rendered
    /// to, which the sweep forgets.
    Sealable {
        words: Scrubbed<LineWords>,
        fed: Vec<Renderable>,
    },
    /// Its words do not pass the current rules' door.
    Withheld,
    /// Its key does not open it.
    Unopened,
}

impl Opened {
    fn of(seal: &LineEvent, slot: usize, key: &TextKey) -> Self {
        let Some(words) = seal.unscrubbed(slot, key) else {
            return Self::Unopened;
        };
        match Scrubbed::of(&words) {
            Ok(scrubbed) => Self::Sealable {
                fed: transcript::rendered([&words, scrubbed.get()]),
                words: scrubbed,
            },
            Err(_) => Self::Withheld,
        }
    }
}

impl Census {
    /// Count a slot as it stands; a stale one is counted once it is opened ([`Census::opened`]).
    fn stands(&mut self, place: &Place, stands: &Stands<'_>) {
        match stands {
            Stands::Erased => self.erased += 1,
            Stands::Current => self.current += 1,
            Stands::Withheld => self.withheld.push(place.clone()),
            Stands::Unopened => self.unopened.push(place.clone()),
            Stands::Stale { .. } => {}
        }
    }

    fn opened(&mut self, place: &Place, opened: &Opened) {
        match opened {
            Opened::Sealable { .. } => self.stale += 1,
            Opened::Withheld => self.withheld.push(place.clone()),
            Opened::Unopened => self.unopened.push(place.clone()),
        }
    }
}

/// Stale slots already opened, by the name of their seal's line and the slot: a seal's name is
/// the hash of its words as locked and of its key's name, so an opening is reused only for the
/// very seal it was opened from.
type Openings = BTreeMap<(Hash, usize), Opened>;

/// The lines as one run reads them, once: the census and the reseal both read this value.
struct Reading<'g> {
    /// The lines that hold words of their own.
    lines: usize,
    /// How many lines each conversation holds: what its next line is written holding.
    held: BTreeMap<ConversationId, u32>,
    /// Every slot of every line holding words of its own, in the store's order.
    slots: Vec<(Place, Stands<'g>)>,
}

impl<'g> Reading<'g> {
    /// Every slot `graph` holds, each as the keys `conversations` holds say it stands.
    ///
    /// A slot is stale while any seal of it that opens here is under older rules and no reseal
    /// that opens here supersedes it: each such seal is sealed again, from the words a reader
    /// reads (its own seal while that opens, else the first reseal that does), so a seal a sweep
    /// kept is superseded at last whatever the rules did since. No slot's words are opened here;
    /// whether a reseal supersedes is asked as every reader asks it, by opening the reseal.
    fn of(
        conversations: Conversations<'_>,
        graph: &'g Graph<LineEvent>,
    ) -> Result<Self, StoreError> {
        let mut by: BTreeMap<&ConversationId, Vec<_>> = BTreeMap::new();
        for (name, object) in graph.objects() {
            if let Some(line) = object.event() {
                by.entry(line.conversation())
                    .or_default()
                    .push((name, line));
            }
        }
        let mut reading = Reading {
            lines: 0,
            held: BTreeMap::new(),
            slots: Vec::new(),
        };
        for (id, lines) in by {
            reading.held.insert(
                id.clone(),
                u32::try_from(lines.len())
                    .map_err(|_| StoreError::unreadable("lines", "too many"))?,
            );
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
            // The line holding each key: a seal is named by its line and its slot.
            let named: BTreeMap<&KeyName, &Hash> = lines
                .iter()
                .flat_map(|(name, line)| line.keys().map(move |key| (key, *name)))
                .collect();
            let superseded = transcript::superseded(lines.iter().copied(), &|name: &KeyName| {
                held.get(name)
                    .and_then(|key: &TextKey| TextKey::parse(name, key.bytes()))
                    .map_or_else(|| Gone::Missing.into(), Words::kept)
            });
            for (_, line) in &lines {
                if matches!(line.shape(), LineShape::Resealed { .. }) {
                    continue;
                }
                reading.lines += 1;
                for (slot, key) in line.keys().enumerate() {
                    let place = Place {
                        conversation: line.conversation().clone(),
                        at: line.at(),
                        slot,
                        key: key.clone(),
                    };
                    if erased.contains_key(key) {
                        reading.slots.push((place, Stands::Erased));
                        continue;
                    }
                    // Every seal of the slot that opens, in a reader's order: its own, then its
                    // reseals, each with whether it is under the current rules.
                    let seals: Vec<(&Hash, &LineEvent, usize, &KeyName, bool)> =
                        std::iter::once((*line, slot, key))
                            .chain(reseals.get(key).into_iter().flatten().filter_map(|reseal| {
                                reseal.keys().next().map(|own| (*reseal, 0, own))
                            }))
                            .filter_map(|(seal, at, own)| {
                                let current = seal
                                    .scrubbed_under()
                                    .nth(at)
                                    .is_some_and(|under| under.is_current(()));
                                let name = named.get(own)?;
                                held.contains_key(own)
                                    .then_some((*name, seal, at, own, current))
                            })
                            .collect();
                    let from: Vec<KeyName> = seals
                        .iter()
                        .filter(|(.., current)| !current)
                        .map(|(_, _, _, own, _)| (*own).clone())
                        .filter(|own| !superseded.contains(own))
                        .collect();
                    let stands = match seals.first() {
                        None => Stands::Unopened,
                        Some(_) if from.is_empty() => {
                            if seals.iter().any(|(.., current)| *current) {
                                Stands::Current
                            } else {
                                Stands::Withheld
                            }
                        }
                        // The key leaves the map: it opens this slot's seal and no other's.
                        Some((name, seal, at, own, _)) => match held.remove(*own) {
                            Some(key) => Stands::Stale {
                                name,
                                seal,
                                slot: *at,
                                key,
                                from,
                            },
                            None => Stands::Unopened,
                        },
                    };
                    reading.slots.push((place, stands));
                }
            }
        }
        Ok(reading)
    }

    /// Count every slot into a census of this reading, and open each stale one, once, in the
    /// store's order, unless `opened` already holds it: what the census and a run both read.
    fn opened<'r>(
        &'r self,
        census: &'r mut Census,
        mut opened: Openings,
    ) -> impl Iterator<Item = (&'r Place, &'r TextKey, &'r [KeyName], Opened)> + 'r {
        self.slots.iter().filter_map(move |(place, stands)| {
            census.stands(place, stands);
            let Stands::Stale {
                name,
                seal,
                slot,
                key,
                from,
            } = stands
            else {
                return None;
            };
            let opened = opened
                .remove(&((*name).clone(), *slot))
                .unwrap_or_else(|| Opened::of(seal, *slot, key));
            census.opened(place, &opened);
            Some((place, key, from.as_slice(), opened))
        })
    }

    /// Every stale slot of this reading opened, by its seal's line and slot.
    fn openings(&self) -> Openings {
        self.slots
            .iter()
            .filter_map(|(_, stands)| match stands {
                Stands::Stale {
                    name,
                    seal,
                    slot,
                    key,
                    ..
                } => Some((((*name).clone(), *slot), Opened::of(seal, *slot, key))),
                Stands::Erased | Stands::Current | Stands::Withheld | Stands::Unopened => None,
            })
            .collect()
    }

    fn census(&self) -> Census {
        Census {
            lines: self.lines,
            ..Census::default()
        }
    }
}

/// How the lines `conversations` holds stand against the current rules, read under no lock: what
/// a run would see now.
///
/// # Errors
/// The stores' own.
pub fn census(conversations: Conversations<'_>) -> Result<Census, StoreError> {
    let graph = conversations.lines.graph()?;
    let reading = Reading::of(conversations, &graph)?;
    let mut census = reading.census();
    reading.opened(&mut census, Openings::new()).for_each(drop);
    Ok(census)
}

/// Seal every stale slot again under the current rules at `at`, one reseal line per seal it
/// supersedes, and answer how many. The keys it supersedes stand until [`run`]'s sweep.
///
/// # Errors
/// The stores' own, and a reseal the door refuses.
pub fn reseal(conversations: Conversations<'_>, at: Datetime) -> Result<usize, StoreError> {
    resealed(conversations, at, BATCH).map(|(_, resealed, _)| resealed)
}

/// [`reseal`]. The slots are opened and scrubbed first, under no lock, since a reseal's words are
/// a function of its seal, its key and the rules alone. The lines' lock is then taken and the
/// store read again: that reading is the one the run trusts, and the census it answers is of the
/// store it sealed. A slot stale in it whose seal was opened before is not opened again; one
/// that is not (a line written between the two readings) is opened under the lock, and a slot an
/// erasure or another run settled between them is no longer stale, so its opening is dropped.
/// Each batch's keys are durable before any of its lines is placed, so no line is ever durable
/// before its key; the keys a stopped batch staged and never placed are removed first.
fn resealed(
    conversations: Conversations<'_>,
    at: Datetime,
    batch_of: std::num::NonZeroUsize,
) -> Result<(Census, usize, transcript::Known), StoreError> {
    let openings = Reading::of(conversations, &*conversations.lines.graph()?)?.openings();
    let _held = conversations.lines.locked()?;
    conversations.keys.tidy()?;
    let refused = |why: &dyn std::fmt::Display| StoreError::not_appended("lines", why);
    let graph = conversations.lines.graph()?;
    let reading = Reading::of(conversations, &graph)?;
    let mut held = reading.held.clone();
    let mut census = reading.census();
    let mut known = transcript::Known::default();
    let mut resealed = 0;
    let mut stale = reading.opened(&mut census, openings);
    loop {
        let mut batch: Vec<(LineEvent, TextKey)> = Vec::new();
        while batch.len() < batch_of.get() {
            let Some((place, key, from, opened)) = stale.next() else {
                break;
            };
            let Opened::Sealable { words, fed } = opened else {
                continue;
            };
            if from.contains(&key.name()) {
                known.fed.insert(key.name(), fed);
            }
            for from in from {
                let count = held.entry(place.conversation.clone()).or_default();
                batch.push(
                    LineEvent::reseal(
                        place.conversation.clone(),
                        at,
                        transcript::writer(),
                        *count,
                        place.key.clone(),
                        from.clone(),
                        &words,
                        conversations.entropy,
                    )
                    .map_err(|why| refused(&why))?,
                );
                *count += 1;
            }
        }
        if batch.is_empty() {
            break;
        }
        let keys: Vec<(KeyName, &TextKey)> =
            batch.iter().map(|(_, key)| (key.name(), key)).collect();
        conversations.keys.set_all(
            &keys
                .iter()
                .map(|(name, key)| (name, *key))
                .collect::<Vec<_>>(),
        )?;
        known.sealed.extend(keys.into_iter().map(|(name, _)| name));
        let mut lines = batch.into_iter().map(|(line, _)| line);
        resealed += conversations
            .lines
            .append_each(&mut || Ok(lines.next()))?
            .len();
    }
    drop(stale);
    Ok((census, resealed, known))
}

/// Seal every stale slot again ([`reseal`]), `batch` reseals at a time, then sweep, destroying
/// each key a reseal supersedes. Running it again appends nothing and destroys nothing more.
///
/// # Errors
/// The stores' own, a reseal the door refuses, and a sweep that kept keys (the next run destroys
/// them).
pub fn run(
    conversations: Conversations<'_>,
    at: Datetime,
    batch: std::num::NonZeroUsize,
) -> Result<Done, StoreError> {
    let (census, resealed, known) = resealed(conversations, at, batch)?;
    let destroyed = transcript::sweep_knowing(conversations, &known)?;
    Ok(Done {
        census,
        resealed,
        destroyed,
    })
}
