//! The inert memory and digest: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_lines::secretary::conversations;

/// No owner: nothing here believes a retraction or an erasure.
static NO_OWNERS: std::collections::BTreeSet<prodrome_data::event::Actor> =
    std::collections::BTreeSet::new();

impl crate::secretary::memories::Memories<'static> {
    #[must_use]
    pub fn nothing() -> Self {
        crate::secretary::memories::Memories {
            history: &Nothing,
            texts: &Nothing,
            cards: &Nothing,
            entropy: &Nothing,
            digests: crate::secretary::digests::Digests {
                history: &Nothing,
                texts: &Nothing,
                lines: conversations::Conversations::nothing(),
                owners: &NO_OWNERS,
                compacting: &crate::secretary::compacting::Compacting::Off,
            },
        }
    }
}
