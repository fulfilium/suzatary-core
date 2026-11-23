//! A calendar card whose event has ended closes as no longer needed.

use crate::secretary::done_elsewhere::{self, Seen, Swept};
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;
use suzatary_base::zone::Zone;
use suzatary_google::calendar::When;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_inbox::secretary::responded::Withdrawal;
use suzatary_store::store::Prodrome;

/// Close every waiting calendar card whose event ended by `at`; an all-day event ends when the
/// owner's day after its last begins, in `zone`.
pub fn sweep(inbox: &dyn Prodrome<InboxEvent>, zone: Zone, at: Datetime) -> Result<Swept, Refusal> {
    let now = prodrome_data::instant::instant_of(at);
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
