use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::mail::junk::{DEFAULT_DAYS, Window, digest};
use crate::mail::junk_folder::JunkFailure;
use crate::secretary::verbs::not_a_junk_account;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// The Junk folders, swept.
pub struct JunkDigest;

#[derive(Deserialize, JsonSchema)]
pub struct JunkDigestArgs {
    account: Option<String>,
    days: Option<usize>,
}

impl Verb for JunkDigest {
    type Surface = SecretarySurface;
    type Args = JunkDigestArgs;
    type Answer = Value;
    const NAME: &'static str = "junk_digest";
    const DESCRIPTION: &'static str = "SWEEP A JUNK FOLDER: the mail the triage queue never sees. For each \
        Microsoft account (or only `account`), the last `days` (default 30) of \
        Junk as a count per sender domain (the 50 commonest), and in full the \
        50 newest headers NOT from a churn domain: from, subject, date and the \
        `uid` `junk_show` opens; `domains_left_out` and `listed_left_out` count \
        the rest. `churn` names this deployment's own entries and the seed's size. \
        Read-only: nothing is marked read, moved or deleted, and nothing here \
        can. An account whose sign-in lapsed answers `needs_login`, a sentence \
        to pass on or turn into `propose_act` `auth`; never a prompt.";

    fn run(call: &Call<'_>, args: JunkDigestArgs) -> Result<Value, Refusal> {
        let known = call.caps.junk.junk_accounts();
        let accounts = match args.account {
            None => known,
            Some(account) if known.contains(&account) => vec![account],
            Some(account) => return Err(not_a_junk_account(&account, &known)),
        };
        let today = call
            .deployment
            .zone
            .day(prodrome_core::fpl::instant_of(call.caps.clock.now()));
        let window = Window::ending(today, args.days.unwrap_or(DEFAULT_DAYS))
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let churn = call.caps.junk.churn();
        let digests: Vec<Value> = accounts
            .iter()
            .map(
                |account| match call.caps.junk.junk_headers(account, window.since) {
                    Ok(headers) => json!(digest(account, headers, &churn, window)),
                    Err(JunkFailure::NeedsLogin(needs)) => {
                        json!({"account": account, "needs_login": needs.to_string()})
                    }
                    Err(failed) => json!({"account": account, "failed": failed.to_string()}),
                },
            )
            .collect();
        Ok(json!({
            "window": window,
            "churn": churn.summary(),
            "accounts": digests,
        }))
    }
}
