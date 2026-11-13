//! What one of the secretary's verbs runs against.

use super::Bound;
use crate::deployment::Deployment;
use crate::secretary::Secretary;
use suzatary_base::chains::ChainPolicy;
use suzatary_base::conversation_id::ConversationId;

/// One call's world: the record over the chain the call named, every chain, and the conversation.
#[derive(Clone, Copy)]
pub struct Call<'a> {
    /// The effects, none of which sends.
    pub caps: &'a Secretary<'a>,
    pub deployment: &'a Deployment,
    /// The secret names the instance declares as of this call, the one live list a secret
    /// card mints its name against.
    pub settables: &'a crate::settings::Settables,
    /// The policy of the store `caps.chain` holds.
    pub chain: &'a ChainPolicy,
    /// Every chain, for the verbs that answer for all of them.
    pub chains: &'a [Bound<'a>],
    pub conversation: &'a ConversationId,
}
