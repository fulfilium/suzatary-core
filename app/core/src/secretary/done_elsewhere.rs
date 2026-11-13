//! Close each waiting card a rule finds needs nothing more: done another way, or past its
//! moment; idempotent.

use crate::secretary::inbox::{self, ProposalId};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposal::Proposal;
use crate::secretary::responded::Withdrawal;
use crate::store::Prodrome;
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;

/// What a rule sees of one waiting card.
pub enum Seen {
    /// Needs nothing more: why, as the card's withdrawal says it.
    Closed(Withdrawal),
    /// Still to do, or not this rule's card.
    Waits,
    /// Not looked at, and why.
    Unread(String),
}

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Swept {
    pub closed: Vec<ProposalId>,
    /// The cards not looked at, and why.
    pub unread: Vec<(ProposalId, String)>,
}

/// Close every waiting card that `seen` finds needs nothing more.
pub fn sweep(
    inbox: &dyn Prodrome<InboxEvent>,
    at: Datetime,
    mut seen: impl FnMut(&Proposal) -> Result<Seen, Refusal>,
) -> Result<Swept, Refusal> {
    let waiting: Vec<(ProposalId, Proposal)> = inbox::fold(inbox)?
        .objects()
        .filter(|object| object.reading.performs())
        .map(|object| (object.id.clone(), object.asked.proposal.clone()))
        .collect();
    let mut swept = Swept::default();
    for (id, proposal) in waiting {
        match seen(&proposal)? {
            Seen::Closed(withdrawal) => {
                inbox::withdraw(
                    inbox,
                    &id,
                    &crate::secretary::transcript::writer(),
                    at,
                    withdrawal,
                )?;
                swept.closed.push(id);
            }
            Seen::Waits => {}
            Seen::Unread(why) => swept.unread.push((id, why)),
        }
    }
    Ok(swept)
}
