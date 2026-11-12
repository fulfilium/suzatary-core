//! How a line's slots of words are read: by name, each opened by the key its name answers, or
//! gone as the reading says. A key store read as itself is one; an erasure's reading over it and
//! the box's cache of opened words are others, so every reader of a line reads through one door.

use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::words::{Gone, Words};

/// The words of a slot, locked under the key `name` names as `sealed` and bound to `binding`
/// (what the slot's associated data is), `open` opening them with a key: `None` for a key that
/// does not open them, and words gone only for words that open and are withheld.
pub trait Slots {
    /// The slot's words, or why they are not here.
    fn words(
        &self,
        name: &KeyName,
        binding: &[u8],
        sealed: &[u8],
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords>;

    /// Whether the slot opens.
    fn opens(
        &self,
        name: &KeyName,
        read: &dyn Fn() -> Words<LineWords>,
        unlocks: &dyn Fn(&TextKey) -> bool,
    ) -> bool {
        // Not missing as `read` reads the words, through the door: a reader that keeps the words
        // it reads asks this way, so nothing it keeps went around the scrub.
        let _ = (name, unlocks);
        !matches!(
            read(),
            Words::Gone {
                gone: Gone::Missing
            }
        )
    }
}

/// A key store read as itself: each slot opened by the key its name answers; a key that does not
/// open its slot reads as missing.
impl<F: Fn(&KeyName) -> Words<TextKey>> Slots for F {
    fn words(
        &self,
        name: &KeyName,
        _: &[u8],
        _: &[u8],
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords> {
        match self(name) {
            Words::Kept { kept } => open(&kept).unwrap_or_else(|| Gone::Missing.into()),
            Words::Gone { gone } => gone.into(),
        }
    }

    fn opens(
        &self,
        name: &KeyName,
        _: &dyn Fn() -> Words<LineWords>,
        unlocks: &dyn Fn(&TextKey) -> bool,
    ) -> bool {
        // A key store answers by its key alone, reading no words, so a slot under older rules is
        // not scrubbed to answer.
        match self(name) {
            Words::Kept { kept } => unlocks(&kept),
            Words::Gone { gone } => !matches!(gone, Gone::Missing),
        }
    }
}
