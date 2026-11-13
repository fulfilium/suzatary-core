//! A calendar card whose event has ended needs nothing more, so it closes as no longer needed,
//! by the rule that closes a card done another way ([`done_elsewhere`]).

use crate::calendar::When;
use crate::secretary::acts::Act;
use crate::secretary::done_elsewhere::{self, Seen, Swept};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposal::Proposal;
use crate::secretary::responded::Withdrawal;
use crate::store::Prodrome;
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;
use suzatary_base::zone::Zone;

/// Close every waiting calendar card whose event ended by `at`; an all-day event ends when the
/// owner's day after its last begins, in `zone`.
///
/// # Errors
/// The inbox's own failures.
pub fn sweep(inbox: &dyn Prodrome<InboxEvent>, zone: Zone, at: Datetime) -> Result<Swept, Refusal> {
    let now = prodrome_core::fpl::instant_of(at);
    done_elsewhere::sweep(inbox, at, |proposal| {
        let Proposal::Act(Act::CalendarEvent { event, .. }) = proposal else {
            return Ok(Seen::Waits);
        };
        let ended = match event.when {
            When::Timed { end, .. } => end,
            When::AllDay { end, .. } => zone.start(end),
        };
        Ok(if ended <= now {
            Seen::Closed(Withdrawal::NoLongerNeeded {
                why: format!(
                    "the event ended at {} UTC, so there is nothing to add",
                    ended.format("%Y-%m-%d %H:%M")
                ),
            })
        } else {
            Seen::Waits
        })
    })
}
