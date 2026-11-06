//! Where one topic stands on one target: the join of (presence, provenance) over the frontier
//! writes of its names there (`docs/TOPICS-PLAN-2026-10-21.md` §3). Only the cases the secretary
//! may change carry a token its planners take.

use prodrome_core::literal::Datetime;
use prodrome_core::tags::membership;

use crate::secretary::topics::reading::{MembershipEvent, Reading};
use crate::secretary::topics::target::Target;
use crate::secretary::topics::topic::Topic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing<'r> {
    Kept,
    Suggested(Suggestion<'r>),
    Declined,
    Open(Opening<'r>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion<'r> {
    pub(super) reading: &'r Reading,
    pub(super) target: Target,
    pub(super) topic: Topic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening<'r> {
    pub(super) target: Target,
    pub(super) topic: Topic,
    pub(super) of: std::marker::PhantomData<&'r Reading>,
}

impl Suggestion<'_> {
    #[must_use]
    pub fn topic(&self) -> &Topic {
        &self.topic
    }

    /// The secretary's `Removed` for every name of the topic on the target.
    #[must_use]
    pub fn drop(&self, at: Datetime) -> Vec<MembershipEvent> {
        self.reading.set.remove(
            &self.target,
            &self.topic,
            at,
            &crate::secretary::writes_as(),
        )
    }
}

impl Opening<'_> {
    #[must_use]
    pub fn topic(&self) -> &Topic {
        &self.topic
    }

    #[must_use]
    pub fn suggest(&self, at: Datetime) -> MembershipEvent {
        membership::added(
            self.target.clone(),
            self.topic.clone(),
            at,
            crate::secretary::writes_as(),
        )
    }
}
