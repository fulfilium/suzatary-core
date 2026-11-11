//! What one of the triage agent's verbs runs against.

use crate::agent::Agent;
use crate::deployment::Deployment;
use crate::secretary::filed::TriageCards;
use suzatary_base::chains::ChainPolicy;

/// One call's world: the agent's record over the chain the call named, that chain's policy, and
/// the one door through which its writes reach the owner, as cards.
#[derive(Clone, Copy)]
pub struct Call<'a> {
    /// The effects, none of which reaches outward.
    pub caps: &'a Agent<'a>,
    /// This deployment.
    pub deployment: &'a Deployment,
    /// The policy of the store `caps.chain` holds.
    pub chain: &'a ChainPolicy,
    /// Where its writes go: cards in the Inbox, which only the owner's tap performs.
    pub cards: &'a TriageCards<'a>,
}
