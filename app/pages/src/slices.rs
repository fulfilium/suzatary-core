//! A long list drawn a slice per step: a first reading draws none of it (that step decodes it),
//! the next steps a slice each, newest first, one to a frame (`runtime::Effect::Frame`), and what
//! a later reading adds is drawn with it.
//!
//! A slice's words come to at most `WORDS` bytes, or it holds the one item that alone comes to
//! more, so each frame lays out about as much text whatever the items' sizes.

pub(crate) const WORDS: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Slices {
    shown: usize,
    asked: Asked,
}

/// Whether the next slice is asked for: at most one is, so a reading that lands while one is owed
/// starts no second run of slices beside the first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Asked {
    #[default]
    None,
    Owed,
}

impl Slices {
    /// `before` items were held, `now` are.
    pub(crate) fn read(&mut self, first: bool, before: usize, now: usize) {
        self.shown = if first {
            0
        } else {
            now.min(self.shown + now.saturating_sub(before))
        };
    }

    /// The owed slice, answered: drawn. `words(i)` is the bytes of the words the `i`-th of the
    /// `held` items draws, oldest first.
    pub(crate) fn more(&mut self, held: usize, words: impl Fn(usize) -> usize) {
        self.asked = Asked::None;
        let mut shown = self.shown.min(held);
        let mut spent = 0;
        while shown < held {
            let next = words(held - shown - 1);
            if shown > self.shown && spent + next > WORDS {
                break;
            }
            spent += next;
            shown += 1;
        }
        self.shown = shown;
    }

    /// Whether to ask for the next slice: only when some of `held` is not drawn and none is owed.
    #[must_use]
    pub(crate) fn ask(&mut self, held: usize) -> bool {
        let ask = !self.whole(held) && self.asked == Asked::None;
        if ask {
            self.asked = Asked::Owed;
        }
        ask
    }

    #[must_use]
    pub(crate) const fn whole(self, held: usize) -> bool {
        self.shown >= held
    }

    #[must_use]
    pub(crate) const fn shown(self) -> usize {
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::{Slices, WORDS};

    #[test]
    fn a_reading_landing_while_a_slice_is_owed_asks_for_no_second() {
        let mut s = Slices::default();
        s.read(true, 0, 10);
        assert!(s.ask(10), "the first slice is asked for");
        s.read(false, 10, 12);
        assert!(!s.ask(12), "one is owed already");
        s.more(12, |_| WORDS);
        assert!(
            s.ask(12),
            "the next is asked for once the owed one is answered"
        );
    }

    proptest::proptest! {
        /// Slices draw every item, newest first, each slice at least one item and over `WORDS`
        /// only when it is one item.
        #[test]
        fn each_slice_holds_one_item_or_words_within_the_bound(sizes in proptest::collection::vec(0usize..3 * WORDS, 0..40)) {
            let mut s = Slices::default();
            s.read(true, 0, sizes.len());
            while !s.whole(sizes.len()) {
                let was = s.shown();
                s.more(sizes.len(), |i| sizes[i]);
                let slice = &sizes[sizes.len() - s.shown()..sizes.len() - was];
                proptest::prop_assert!(!slice.is_empty());
                proptest::prop_assert!(slice.len() == 1 || slice.iter().sum::<usize>() <= WORDS);
            }
        }
    }
}
