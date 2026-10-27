//! The two topic stores on disk, box-local and never pushed: a topic name is words.

use suzatary_core::secretary::topics::reading::AliasEvent;
use suzatary_core::secretary::topics::reading::MembershipEvent;
use suzatary_core::secretary::topics::stores::TopicStores;
use suzatary_core::store::StoreError;

use crate::objects::Objects;

/// The memberships and the aliases, opened together.
#[derive(Debug)]
pub struct Topics {
    /// `secretary-topics/` by default.
    pub memberships: Objects<MembershipEvent>,
    /// `secretary-topic-aliases/` by default.
    pub aliases: Objects<AliasEvent>,
}

impl Topics {
    /// Both stores, at their directories. Nothing is read until something asks.
    #[must_use]
    pub fn open(
        memberships: impl Into<std::path::PathBuf>,
        aliases: impl Into<std::path::PathBuf>,
    ) -> Self {
        Self {
            memberships: Objects::open(memberships),
            aliases: Objects::open(aliases),
        }
    }

    /// Both begun, so a tab's first write has a prodrome to rest on.
    ///
    /// # Errors
    /// Either store unreadable or not begun.
    pub fn begin(&self) -> Result<(), StoreError> {
        self.memberships.begin()?;
        self.aliases.begin()
    }

    /// The record a reader or a writer holds.
    #[must_use]
    pub fn stores(&self) -> TopicStores<'_> {
        TopicStores {
            memberships: &self.memberships,
            aliases: &self.aliases,
        }
    }
}
