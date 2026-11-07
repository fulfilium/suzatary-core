//! A long list drawn a slice per step: a first reading draws none of it (that step decodes it),
//! the next steps a slice each, one to a frame (`runtime::Effect::Frame`), and what a later
//! reading adds is drawn with it.

pub(crate) const SLICE: usize = 2;

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

    /// The owed slice, answered: drawn.
    pub(crate) fn more(&mut self, held: usize) {
        self.asked = Asked::None;
        self.shown = held.min(self.shown + SLICE);
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
    use super::Slices;

    #[test]
    fn a_reading_landing_while_a_slice_is_owed_asks_for_no_second() {
        let mut s = Slices::default();
        s.read(true, 0, 10);
        assert!(s.ask(10), "the first slice is asked for");
        s.read(false, 10, 12);
        assert!(!s.ask(12), "one is owed already");
        s.more(12);
        assert!(
            s.ask(12),
            "the next is asked for once the owed one is answered"
        );
    }
}
