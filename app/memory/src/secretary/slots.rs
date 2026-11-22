//! How a line's slots of words are read; every reader of a line reads through this one door.

use crate::secretary::line_words::LineWords;
use crate::secretary::locked_text::{KeyName, TextKey};
use crate::secretary::words::{Gone, Words};

/// The words of a slot locked under the key `name` names; `open` returns `None` for a key that does
/// not open them.
/// The slot's associated data is made inside `open`, so a reader that holds no key makes none.
pub trait Slots {
    fn words(
        &self,
        name: &KeyName,
        open: &dyn Fn(&TextKey) -> Option<Words<LineWords>>,
    ) -> Words<LineWords>;

    fn opens(
        &self,
        name: &KeyName,
        read: &dyn Fn() -> Words<LineWords>,
        unlocks: &dyn Fn(&TextKey) -> bool,
    ) -> bool {
        // A reader that keeps the words asks through `read`, so nothing it keeps went around the
        // scrub.
        let _ = (name, unlocks);
        !matches!(
            read(),
            Words::Gone {
                gone: Gone::Missing
            }
        )
    }
}

/// A key store read as itself: a key that does not open its slot reads as missing.
impl<F: Fn(&KeyName) -> Words<TextKey>> Slots for F {
    fn words(
        &self,
        name: &KeyName,
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
        // A key store answers by its key alone, so a slot under older rules is not scrubbed to
        // answer.
        match self(name) {
            Words::Kept { kept } => unlocks(&kept),
            Words::Gone { gone } => !matches!(gone, Gone::Missing),
        }
    }
}
