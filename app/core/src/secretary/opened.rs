//! The words the box has opened, kept in memory by the name of the key that opened them and the
//! digest of the slot's binding and sealed bytes, the very inputs opening reads: a cache of a
//! pure function, so it never answers otherwise than opening would. A sealed slot moved to
//! another line has another binding, so it is another entry and opens to nothing, as it does
//! uncached. It is one of the copies an erasure clears: the sweep forgets a name before and
//! after it destroys the key, and an erasure's reading answers before the cache is asked.
//!
//! It is bounded by the bytes of the words it keeps ([`Opened::BOUND`]); past it, the oldest
//! entries go first, and a read past the bound opens again.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, PoisonError};

use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::salted_text::Words as _;
use crate::secretary::slots::Slots;
use crate::secretary::words::{Gone, Words};
use crate::store::Gets;

/// One slot as the cache knows it: its key's name, and the digest of its binding and sealed
/// bytes.
type Slot = (KeyName, [u8; 32]);

/// Opened words, oldest first, within [`Opened::BOUND`] bytes.
#[derive(Debug, Default)]
pub struct Opened(Mutex<Kept>);

#[derive(Debug, Default)]
struct Kept {
    words: HashMap<Slot, (LineWords, usize)>,
    order: VecDeque<Slot>,
    bytes: usize,
}

impl Opened {
    /// The most bytes of words kept at once.
    pub const BOUND: usize = 32 << 20;

    fn kept(&self) -> std::sync::MutexGuard<'_, Kept> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forget the words `name` opened, if any are kept.
    pub fn forget(&self, name: &KeyName) {
        let mut kept = self.kept();
        let Kept {
            words,
            order,
            bytes,
        } = &mut *kept;
        words.retain(|(kept, _), (_, size)| {
            let stays = kept != name;
            if !stays {
                *bytes -= *size;
            }
            stays
        });
        order.retain(|(kept, _)| kept != name);
    }

    /// Whether words `name` opened are kept.
    #[must_use]
    pub fn holds(&self, name: &KeyName) -> bool {
        self.kept().words.keys().any(|(kept, _)| kept == name)
    }

    fn get(&self, slot: &Slot) -> Option<LineWords> {
        self.kept().words.get(slot).map(|(words, _)| words.clone())
    }

    fn keep(&self, slot: Slot, words: &LineWords) {
        let size = words.print().len();
        if size > Self::BOUND {
            return;
        }
        let mut kept = self.kept();
        if kept.words.contains_key(&slot) {
            return;
        }
        while kept.bytes + size > Self::BOUND {
            let Some(oldest) = kept.order.pop_front() else {
                break;
            };
            if let Some((_, gone)) = kept.words.remove(&oldest) {
                kept.bytes -= gone;
            }
        }
        kept.bytes += size;
        kept.order.push_back(slot.clone());
        kept.words.insert(slot, (words.clone(), size));
    }
}

/// The slots `keys` opens, read through `opened`: words kept are answered from memory, and words
/// opened are kept; what is gone is never kept. Its fields are private and its slots are always
/// opened by a key from `keys`, so the cache holds only words a key opened.
pub struct Cached<'a> {
    keys: &'a dyn Gets<KeyName, TextKey>,
    opened: &'a Opened,
}

impl<'a> Cached<'a> {
    /// The slots `keys` opens, cached in `opened`.
    pub(crate) fn new(keys: &'a dyn Gets<KeyName, TextKey>, opened: &'a Opened) -> Self {
        Self { keys, opened }
    }

    /// The key named `name`, or why its words are not here: a key the store does not hold, or
    /// cannot read, is missing.
    #[must_use]
    pub fn key(&self, name: &KeyName) -> Words<TextKey> {
        match self.keys.get(name) {
            Ok(Some(key)) => Words::kept(key),
            Ok(None) | Err(_) => Gone::Missing.into(),
        }
    }
}

impl Slots for Cached<'_> {
    fn words(
        &self,
        name: &KeyName,
        binding: &[u8],
        sealed: &[u8],
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        use sha2::Digest as _;
        let digest = sha2::Sha256::new()
            .chain_update((binding.len() as u64).to_be_bytes())
            .chain_update(binding)
            .chain_update(sealed)
            .finalize();
        let slot = (name.clone(), digest.into());
        if let Some(words) = self.opened.get(&slot) {
            return Words::kept(words);
        }
        let words = (|name: &KeyName| self.key(name)).words(name, binding, sealed, open);
        if let Some(kept) = words.get() {
            self.opened.keep(slot, kept);
        }
        words
    }
}
