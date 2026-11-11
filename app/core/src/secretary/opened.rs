//! The box's lines read through the fold a tab reads through ([`HeldLines`]), fed the store's
//! graph as its heads move and each new line's keys. It is the holder an erasure empties
//! ([`crate::secretary::holder::Holder::Opened`]): [`Opened::forget`] drops a key and what it
//! opened at once, and a line that kills a key drops it at the next feed, in any process. Beside
//! the lines it keeps the digest's history read per node ([`Kept`]), which holds summaries'
//! words, so a forgetting drops it too.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use prodrome_core::event::Hash;

use crate::secretary::digest::Kept;
use crate::secretary::held_lines::{HeldLines, TheBox};
use crate::secretary::key_ring::spelled;
use crate::secretary::line_event::LineEvent;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::store::{Gets, Graph, StoreError};

#[derive(Default)]
pub struct Opened {
    lines: Mutex<Fold>,
    said: Mutex<Kept>,
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

    /// `read` of the digest's history as kept per node.
    pub(crate) fn said<T>(&self, read: impl FnOnce(&mut Kept) -> T) -> T {
        read(&mut self.said.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Whether the key `name` is held.
    #[must_use]
    pub fn holds(&self, name: &KeyName) -> bool {
        self.fold().lines.key_ring().holds(name)
    }

    /// `read` of the fold once it holds `graph`'s objects.
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
        if fold.heads != *graph.heads() {
            fold.feed(graph, keys)?;
        }
        Ok(read(&mut fold.lines))
    }
}

impl Fold {
    /// What `graph` holds and the fold does not, with the keys it names; every key a line kills
    /// dropped.
    fn feed(
        &mut self,
        graph: &Graph<LineEvent>,
        keys: &dyn Gets<KeyName, TextKey>,
    ) -> Result<(), StoreError> {
        let fresh: Vec<_> = graph
            .objects()
            .filter(|(name, _)| !self.lines.holds(name))
            .map(|(name, object)| (name.clone(), object.clone()))
            .collect();
        let mut given = BTreeMap::new();
        let named = fresh
            .iter()
            .filter_map(|(_, object)| object.event())
            .flat_map(LineEvent::keys);
        for name in named {
            if !self.lines.key_ring().holds(name)
                && !given.contains_key(name)
                && let Some(key) = keys.get(name)?
            {
                given.insert(name.clone(), spelled(&key));
            }
        }
        let gone: Vec<Hash> = self
            .lines
            .names()
            .filter(|name| graph.dag().get(name).is_none())
            .cloned()
            .collect();
        self.lines.keys(&given, &[]);
        self.lines.hold_objects(fresh, &gone);
        let dead: Vec<KeyName> = self
            .lines
            .dead()
            .into_iter()
            .filter(|name| self.lines.key_ring().holds(name))
            .collect();
        self.lines.keys(&BTreeMap::new(), &dead);
        self.heads.clone_from(graph.heads());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{HeldLines, Opened};
    use crate::conversation_id::ConversationId;
    use crate::secretary::conversations::{Conversations, InMemory};
    use crate::secretary::erasure::Erasure;
    use crate::secretary::model::Turn;
    use crate::secretary::store::{Entry, Kind};
    use crate::secretary::transcript;
    use crate::secretary::words::Words;
    use prodrome_core::literal::Datetime;

    const SAID: &str = "a word only the owner says";

    fn at(second: u32) -> Datetime {
        Datetime::new(2026, 11, 9, 9, 0, second, 0).expect("an instant")
    }

    /// What every reader of the fold answers, printed.
    fn read_by_all(conversations: Conversations<'_>, id: &ConversationId) -> String {
        let reads = conversations.reads();
        format!(
            "{:?} {:?} {:?}",
            reads.fold(HeldLines::gathered).expect("reads"),
            reads.fold(HeldLines::transcripts).expect("reads"),
            reads.fold(|lines| lines.transcript(id)).expect("reads"),
        )
    }

    /// Law (a privacy boundary): an erasure's sweep leaves its fold holding neither the key nor
    /// the words, before any read, and no reader of the fold answers them after it. A fold the
    /// sweep never told, as in another process, drops both at its next read.
    #[test]
    fn an_erasure_leaves_no_words_in_the_fold_or_its_readers() {
        let id = ConversationId::new("2026-11-09-1").expect("a slug");
        for told in [true, false] {
            let held = InMemory::default();
            let elsewhere = Opened::default();
            let fold = if told { &held.opened } else { &elsewhere };
            let reading = Conversations {
                opened: fold,
                ..held.conversations()
            };
            let opening = Entry::Opened {
                at: at(0),
                title: Words::kept("Notes".to_owned()),
                model: "a-cli/small".to_owned(),
                kind: Kind::Topic,
            };
            transcript::start(held.conversations(), &id, &opening).expect("started");
            let said = Entry::Said {
                at: at(1),
                turn: Turn::Person {
                    text: SAID.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            };
            transcript::push(held.conversations(), &id, &said).expect("said");
            assert!(read_by_all(reading, &id).contains(SAID), "told: {told}");
            assert!(crate::secretary::held_lines::says(
                &fold.lines.lock().expect("held").lines,
                SAID
            ));
            let keys: std::collections::BTreeSet<_> =
                transcript::read(reading, &id).expect("reads").lines()[1]
                    .line
                    .keys()
                    .cloned()
                    .collect();
            let erasure = Erasure::Asked {
                by: transcript::writer(),
            };
            transcript::erase(held.conversations(), &id, keys.clone(), erasure, at(2))
                .expect("erased");
            let holds = || {
                keys.iter().any(|key| fold.holds(key))
                    || crate::secretary::held_lines::says(
                        &fold.lines.lock().expect("held").lines,
                        SAID,
                    )
            };
            assert_eq!(holds(), !told, "before a read, told: {told}");
            assert!(!read_by_all(reading, &id).contains(SAID), "told: {told}");
            assert!(!holds(), "after a read, told: {told}");
        }
    }
}
