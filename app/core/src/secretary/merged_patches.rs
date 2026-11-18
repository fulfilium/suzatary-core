//! A repository card whose patch the default branch already holds is done another way.

use crate::secretary::done_elsewhere::{self, Seen, Swept};
use suzatary_base::capability::Datetime;
use suzatary_base::refusal::Refusal;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::Proposal;
use suzatary_inbox::secretary::responded::Withdrawal;
use suzatary_services::repositories::Repositories;
use suzatary_store::store::Prodrome;

/// Close every waiting repository card whose patch `repos` finds merged.
pub fn sweep(
    inbox: &dyn Prodrome<InboxEvent>,
    repos: &dyn Repositories,
    at: Datetime,
) -> Result<Swept, Refusal> {
    done_elsewhere::sweep(inbox, at, |proposal| {
        let Proposal::Act(Act::Patch { repo, patch, .. }) = proposal else {
            return Ok(Seen::Waits);
        };
        Ok(match repos.merged(repo, patch) {
            Ok(Some(held)) => Seen::Closed(Withdrawal::DoneElsewhere {
                what: format!(
                    "its change is already in {repo}'s {} at {}, so nothing is left to publish",
                    held.branch,
                    held.tip.short()
                ),
            }),
            Ok(None) => Seen::Waits,
            Err(failure) => Seen::Unread(format!("{repo}: {failure}")),
        })
    })
}
