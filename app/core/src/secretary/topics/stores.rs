//! The two topic stores, read together, since a membership's topic reads through the aliases.

use crate::secretary::topics::reading::AliasEvent;
use crate::secretary::topics::reading::MembershipEvent;
use crate::secretary::topics::reading::Reading;
use crate::store::{Prodrome, StoreError};

#[derive(Clone, Copy)]
pub struct TopicStores<'a> {
    pub memberships: &'a dyn Prodrome<MembershipEvent>,
    pub aliases: &'a dyn Prodrome<AliasEvent>,
}

impl TopicStores<'_> {
    /// # Errors
    /// Either store unreadable.
    pub fn reading(&self) -> Result<Reading, StoreError> {
        super::reading::of(&self.memberships.nodes()?, &self.aliases.nodes()?)
            .map_err(|several| StoreError::unreadable("topics", several))
    }
}
