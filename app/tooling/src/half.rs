//! One half of the merge gate.

use crate::Error;

/// One half of the merge gate: `Tree` is `merge-gate.sh --no-live`, `Live` is `--only-live`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Half {
    Tree,
    Live,
}

impl Half {
    /// Both halves, in the order the train gates and reports them.
    pub const BOTH: [Half; 2] = [Half::Tree, Half::Live];

    /// Its name on the command line and in the table's paths.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Half::Tree => "tree",
            Half::Live => "live",
        }
    }

    pub fn parse(word: &str) -> Result<Half, Error> {
        Half::BOTH
            .into_iter()
            .find(|half| half.name() == word)
            .ok_or_else(|| Error::Usage(format!("\"{word}\" is no half: tree or live")))
    }
}
