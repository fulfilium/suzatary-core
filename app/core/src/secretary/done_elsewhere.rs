//! Close each waiting card a rule finds done another way; idempotent.

use crate::secretary::inbox::{self, ProposalId};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposal::Proposal;
use crate::secretary::responded::Withdrawal;
use crate::store::Prodrome;
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;

/// What a rule sees of one waiting card.
pub enum Seen {
    /// Done another way, in the box's words.
    Done(String),
    /// Still to do, or not this rule's card.
    Waits,
    /// Not looked at, and why.
    Unread(String),
}

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Swept {
    /// The cards closed.
    pub closed: Vec<ProposalId>,
    /// The cards not looked at, and why.
    pub unread: Vec<(ProposalId, String)>,
}

/// Close every waiting card that `seen` finds done another way.
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
            Seen::Done(what) => {
                inbox::withdraw(
                    inbox,
                    &id,
                    &crate::secretary::transcript::writer(),
                    at,
                    Withdrawal::DoneElsewhere { what },
                )?;
                swept.closed.push(id);
            }
            Seen::Waits => {}
            Seen::Unread(why) => swept.unread.push((id, why)),
        }
    }
    Ok(swept)
}
