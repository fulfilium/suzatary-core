//! A repository card whose patch the default branch already holds is done another way.

use crate::capability::Datetime;
use crate::refusal::Refusal;
use crate::repositories::Repositories;
use crate::secretary::acts::Act;
use crate::secretary::done_elsewhere::{self, Seen, Swept};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposal::Proposal;
use crate::store::Prodrome;

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
            Ok(Some(held)) => Seen::Done(format!(
                "its change is already in {repo}'s {} at {}, so nothing is left to publish",
                held.branch,
                held.tip.short()
            )),
            Ok(None) => Seen::Waits,
            Err(failure) => Seen::Unread(format!("{repo}: {failure}")),
        })
    })
}
