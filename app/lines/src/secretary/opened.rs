//! The box's lines read through the fold a tab reads through, and the holder an
//! erasure empties ([`suzatary_memory::secretary::holder::Holder::Opened`]): [`Opened::forget`]
//! drops a key and what it opened at once, and a line that kills a key drops it at the next feed.
//! What readers derive from the words is dropped by a forgetting too.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use prodrome_data::event::Hash;

use crate::secretary::derived::Derived;
use crate::secretary::held_lines::{HeldLines, TheBox};
use crate::secretary::line_event::LineEvent;
use suzatary_memory::secretary::key_ring::spelled;
use suzatary_memory::secretary::locked_text::{KeyName, TextKey};
use suzatary_store::store::{Gets, Graph, StoreError};

#[derive(Default)]
pub struct Opened {
    lines: Mutex<Fold>,
    said: Mutex<Derived>,
}

struct Fold {
    lines: HeldLines<TheBox>,
    heads: BTreeSet<Hash>,
}

impl Default for Fold {
    fn default() -> Self {
        Self {
            lines: HeldLines::for_the_box(),
            heads: BTreeSet::new(),
        }
    }
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened")
            .field("heads", &self.fold().heads)
            .finish_non_exhaustive()
    }
}

impl Opened {
    fn fold(&self) -> MutexGuard<'_, Fold> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forget the key `name` and every word it opened, and every summary's words kept.
    pub fn forget(&self, name: &KeyName) {
        self.fold()
            .lines
            .keys(&BTreeMap::new(), std::slice::from_ref(name));
        self.said
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// `read` of the value of type `T` a reader derives from the words, kept until a forgetting.
    pub fn said<T: Default + Send + 'static, R>(&self, read: impl FnOnce(&mut T) -> R) -> R {
        self.said
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .with(read)
    }

    /// Whether the fold keeps words that say `text`: test support.
    #[cfg(any(test, feature = "nothing"))]
    #[must_use]
    pub fn says(&self, text: &str) -> bool {
        crate::secretary::held_lines::says(&mut self.fold().lines, text)
    }

    #[must_use]
    pub fn holds(&self, name: &KeyName) -> bool {
        self.fold().lines.key_ring().holds(name)
    }

    /// `read` of the fold once it holds `graph`'s objects. A feed that drops a key forgets every
    /// summary's words kept too, before the fold is read.
    ///
    /// # Errors
    /// The key store unreadable; nothing is fed then.
    pub(crate) fn read<T>(
        &self,
        graph: &Graph<LineEvent>,
        keys: &dyn Gets<KeyName, TextKey>,
        read: impl FnOnce(&mut HeldLines<TheBox>) -> T,
    ) -> Result<T, StoreError> {
        let mut fold = self.fold();
        if fold.heads != *graph.heads() && !fold.feed(graph, keys)?.is_empty() {
            drop(fold);
            self.said
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
            fold = self.fold();
        }
        Ok(read(&mut fold.lines))
    }
}

impl Fold {
    /// What `graph` holds and the fold does not, with the keys it names; every key a line kills
    /// dropped. The keys dropped.
    fn feed(
        &mut self,
        graph: &Graph<LineEvent>,
        keys: &dyn Gets<KeyName, TextKey>,
    ) -> Result<Vec<KeyName>, StoreError> {
        let fresh: Vec<_> = crate::parts::part("fresh", || {
            graph
                .objects()
                .filter(|(name, _)| !self.lines.holds(name))
                .map(|(name, object)| (name.clone(), object.clone()))
                .collect()
        });
        let mut given = BTreeMap::new();
        let named = fresh
            .iter()
            .filter_map(|(_, object)| object.event())
            .flat_map(LineEvent::keys);
        for name in named {
            if !self.lines.key_ring().holds(name)
                && !given.contains_key(name)
                && let Some(key) = crate::parts::part("keys", || keys.get(name))?
            {
                given.insert(name.clone(), spelled(&key));
            }
        }
        let gone: Vec<Hash> = crate::parts::part("gone", || {
            self.lines
                .names()
                .filter(|name| graph.dag().get(name).is_none())
                .cloned()
                .collect()
        });
        crate::parts::part("ring", || self.lines.keys(&given, &[]));
        crate::parts::part("hold", || self.lines.hold_objects(fresh, &gone));
        let ring = self.lines.key_ring();
        let dead: Vec<KeyName> =
            crate::parts::part("dead", || self.lines.dead(&|name| ring.holds(name)))
                .into_iter()
                .collect();
        self.lines.keys(&BTreeMap::new(), &dead);
        self.heads.clone_from(graph.heads());
        Ok(dead)
    }
}
