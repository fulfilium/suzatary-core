//! The step every change to the redaction rules is followed by: each slot of the conversations'
//! lines whose words the box can open under older rules only is sealed again, scrubbed under the
//! current rules, under a key of its own, and the key of the seal it replaces is destroyed.
//!
//! Nothing is deleted: a reseal that opens with a key held here supersedes `from`, so a reader
//! never opens the seal it replaced and the one sweep destroys that key; a store restored without
//! the reseals' keys destroys no old key. The store only grows and its genesis does not move, so a
//! tab keeps its replica and forgets the superseded keys as it forgets erased ones. A run stopped
//! at any point leaves every slot readable with the same words; the cli tests `migrate_scrub_*`
//! are the laws.
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

/// How many reseals one batch writes: a crash loses at most one batch's work.
pub const BATCH: std::num::NonZeroUsize = std::num::NonZeroUsize::new(1000).expect("not zero");

/// How the lines' slots stand against the current rules: a seal under the current rules whose
/// held key does not open it is counted current, and reads as missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    /// The lines that hold words of their own (every line but a reseal).
    pub lines: usize,
    /// The slots a seal under the current rules opens.
    pub current: usize,
    /// The slots a seal under older rules opens that nothing here supersedes.
    pub stale: usize,
    pub erased: usize,
    /// The slots no seal opens: their keys are lost.
    pub unopened: Vec<Place>,
    /// The slots whose words open under older rules and do not pass the current rules' door.
    pub withheld: Vec<Place>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub conversation: ConversationId,
    pub at: Datetime,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub census: Census,
    pub resealed: usize,
    /// The keys its sweep destroyed: superseded seals', and any erasure owed.
    pub destroyed: usize,
}

/// One slot as the keys held say it stands, before any of its words is opened.
enum Stands<'g> {
    Erased,
    Current,
    /// Every seal that opens is superseded, by one whose words the current rules withhold.
    Withheld,
    Unopened,
    /// Sealed again from each of `from`: the seals under older rules that nothing here supersedes.
    Stale {
        name: &'g Hash,
        seal: &'g LineEvent,
        slot: usize,
        key: TextKey,
        from: Vec<KeyName>,
    },
}

/// A stale slot opened once and scrubbed once.
enum Opened {
    /// Its words through the current rules' door, and the blocks its seal's words were rendered
    /// to, which the sweep forgets.
    Sealable {
        words: Scrubbed<LineWords>,
        fed: Vec<Renderable>,
    },
    Withheld,
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

/// Stale slots already opened: a seal's name hashes its words as locked and its key's name, so an
/// opening is reused only for the very seal it was opened from.
type Openings = BTreeMap<(Hash, usize), Opened>;

/// The lines as one run reads them, once.
struct Reading<'g> {
    lines: usize,
    /// How many lines each conversation holds: what its next line is written holding.
    held: BTreeMap<ConversationId, u32>,
    slots: Vec<(Place, Stands<'g>)>,
}

impl<'g> Reading<'g> {
    /// Every slot `graph` holds: stale while any seal of it that opens here is under older rules
    /// and no reseal that opens here supersedes it. No slot's words are opened here.
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
                    // Every seal of the slot that opens, in a reader's order.
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

    /// Count every slot into a census, and open each stale one once unless `opened` holds it.
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

/// How the lines `conversations` holds stand against the current rules, read under no lock.
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
/// A reseal the door refuses.
pub fn reseal(conversations: Conversations<'_>, at: Datetime) -> Result<usize, StoreError> {
    resealed(conversations, at, BATCH).map(|(_, resealed, _)| resealed)
}

/// [`reseal`]: the slots are opened and scrubbed first, under no lock, since a reseal's words are
/// a function of its seal, its key and the rules alone; the run trusts only the reading under the
/// lock, and drops a slot settled between the two readings. Each batch's keys are durable before
/// any of its lines is placed; the keys a stopped batch staged and never placed are removed first.
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
/// A reseal the door refuses, and a sweep that kept keys (the next run destroys them).
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
