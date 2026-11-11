use schemars::JsonSchema;
use serde::Deserialize;

use crate::mail::sync_now::{self, Report, SyncNow};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::{Refusal, shown};

/// Start the timer's mail sync now and answer its report.
pub struct SyncMail;

#[derive(Deserialize, JsonSchema)]
pub struct SyncMailArgs {
    account: Option<String>,
}

impl Verb for SyncMail {
    type Surface = SecretarySurface;
    type Args = SyncMailArgs;
    type Answer = SyncNow;
    const NAME: &'static str = "sync_mail";
    const DESCRIPTION: &'static str = "Sync the mail NOW instead of waiting for the five-minute \
        timer: the timer's own sync, started early, so it reaches no server the timer does not. \
        Use it after the owner sends something, to confirm the copy is in Sent, or when they say \
        mail has just arrived.\n\nIt answers the sync's report: each account with `ok` and the \
        last line it said, and what `notmuch new` found. `sync` says which sync that is: `fresh` \
        (one finished in the last minute, so none was started), `ran` (started and finished), or \
        `pending` (started, not finished within the wait; ask again in a minute). Every account \
        is pulled whichever you name: `account` narrows the answer to that one and must be one \
        of this deployment's accounts. Then call `search` (for example `folder:Sent` and the \
        subject) to see what arrived.";

    fn run(call: &Call<'_>, args: SyncMailArgs) -> Result<SyncNow, Refusal> {
        let account = args.account.as_deref().map(str::trim);
        if let Some(asked) = account {
            let roster = call.caps.mail.accounts();
            if !roster.iter().any(|known| known.eq_ignore_ascii_case(asked)) {
                return Err(Refusal::bad_request(format!(
                    "account must be one of this deployment's accounts ({}), got {}",
                    roster.join(", "),
                    shown(asked)
                )));
            }
        }
        let now = call.caps.clock.now();
        let last = call
            .caps
            .sync
            .last()
            .map_err(|failed| Refusal::internal(failed.to_string()))?;
        if let Some(synced) = sync_now::fresh(last, now) {
            return Ok(SyncNow::Fresh(Report::of(&synced, account)));
        }
        Ok(
            match call
                .caps
                .sync
                .start(now)
                .map_err(|failed| Refusal::internal(failed.to_string()))?
            {
                Some(synced) => SyncNow::Ran(Report::of(&synced, account)),
                None => sync_now::pending(now),
            },
        )
    }
}

impl From<SyncNow> for VerbAnswer {
    fn from(answer: SyncNow) -> Self {
        Self::json(&answer)
    }
}
