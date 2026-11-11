use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::mail::imap::window_start;
use crate::mail::older::{Older, OlderQuery, Range};
use crate::refusal::{Refusal, shown};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Search the mail servers for mail older than the routine sync keeps, and pull what matched.
pub struct SearchOlder;

#[derive(Deserialize, JsonSchema)]
pub struct SearchOlderArgs {
    account: Option<String>,
    query: String,
    since: String,
}

impl Verb for SearchOlder {
    type Surface = SecretarySurface;
    type Args = SearchOlderArgs;
    type Answer = Older;
    const NAME: &'static str = "search_older";
    const DESCRIPTION: &'static str = "Search the MAIL SERVERS for mail older than the box \
        keeps, and pull what matched onto the box. Use it only after `search` found nothing and \
        what you look for is older than the routine sync's window: it reaches each account's \
        own server, which is slow, and what it finds is somebody else's words.\n\n`query` is \
        this verb's own small language, not notmuch's: `from:` and `to:` (an address or a name), \
        `subject:` (a word, or a \"quoted phrase\"), a bare word or \"phrase\" to find in the \
        body, and `before:YYYY-MM-DD`. Every term must match. `since` (YYYY-MM-DD) is the first \
        day searched; the last is the day before `before`, or before the routine sync's window \
        begins, whichever is earlier. `account` narrows it to one of the owner's accounts; \
        without it every account is searched.\n\nIt answers `searched`, a sentence naming the \
        accounts, folders and dates covered and what was not, which you pass on when you say \
        what you found; then per account `matched`, `already_here` (on the box already: \
        `search` finds those), `pulled` (rows as `search` answers them; `show` takes each \
        `id`), `beyond_cap` (at most 25 are pulled per account; narrow the query or the dates \
        for the rest), and `local`, the `search` query for everything older searches have \
        pulled; or `not_searched` and why. Pulled mail lands already read and never enters the \
        triage queue.";

    fn run(call: &Call<'_>, args: SearchOlderArgs) -> Result<Older, Refusal> {
        let query: OlderQuery =
            args.query
                .parse()
                .map_err(|failed: crate::mail::older::QueryError| {
                    Refusal::bad_request(format!("query: {failed}"))
                })?;
        let since = NaiveDate::parse_from_str(args.since.trim(), "%Y-%m-%d").map_err(|_| {
            Refusal::bad_request(format!(
                "since is the first day to search, as YYYY-MM-DD, got {}",
                shown(&args.since)
            ))
        })?;
        let today = call
            .deployment
            .zone
            .day(prodrome_core::fpl::instant_of(call.caps.clock.now()));
        let range = Range::older(since, query.before(), window_start(today))
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let roster = call.caps.mail.accounts();
        let accounts = match args.account.as_deref().map(str::trim) {
            None => roster,
            Some(asked) => vec![
                roster
                    .iter()
                    .find(|known| known.eq_ignore_ascii_case(asked))
                    .cloned()
                    .ok_or_else(|| {
                        Refusal::bad_request(format!(
                            "account must be one of this deployment's accounts ({}), got {}",
                            roster.join(", "),
                            shown(asked)
                        ))
                    })?,
            ],
        };
        Ok(Older::of(
            range,
            call.caps.older.search_older(&accounts, &query, range),
        ))
    }
}

impl From<Older> for VerbAnswer {
    fn from(answer: Older) -> Self {
        Self::json(&answer)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use prodrome_core::event::Actor;
    use prodrome_core::policy::Untrusted;
    use serde_json::{Value, json};

    use super::*;
    use crate::capability::{Nothing, Secretary};
    use crate::chains::{ChainName, ChainPolicy, Chains};
    use crate::conversation_id::ConversationId;
    use crate::deployment::Deployment;
    use crate::mail::mailbox::{MailError, Mailbox, Query};
    use crate::mail::older::{AccountOlder, Found, Matched, Outcome};
    use crate::mail::older_mail::OlderMail;

    /// A server that records what it was asked and finds one message per account.
    #[derive(Default)]
    struct Asked(Mutex<Vec<(Vec<String>, Range)>>);

    impl OlderMail for Asked {
        fn search_older(
            &self,
            accounts: &[String],
            _query: &OlderQuery,
            range: Range,
        ) -> Vec<AccountOlder> {
            self.0
                .lock()
                .expect("the log")
                .push((accounts.to_vec(), range));
            accounts
                .iter()
                .map(|account| AccountOlder {
                    account: account.clone(),
                    outcome: Outcome::Searched(Found {
                        folders: vec!["INBOX".to_owned()],
                        matched: Matched::Exactly(1),
                        already_here: 0,
                        pulled: Vec::new(),
                        unnamed: 1,
                        too_large: 0,
                        beyond_cap: 0,
                        local: "path:a/older-inbox/**".to_owned(),
                    }),
                })
                .collect()
        }
    }

    struct Roster;

    impl Mailbox for Roster {
        fn accounts(&self) -> Vec<String> {
            vec!["ada@example.test".to_owned(), "bo@example.test".to_owned()]
        }
        fn inbox(
            &self,
            _: Option<&str>,
            _: usize,
            _: Option<crate::capability::Datetime>,
        ) -> Result<Vec<crate::mail::MailRow>, MailError> {
            Ok(Vec::new())
        }
        fn search(
            &self,
            _: &Query,
            _: Option<&str>,
            _: usize,
            _: Option<crate::capability::Datetime>,
        ) -> Result<Vec<crate::mail::MailRow>, MailError> {
            Ok(Vec::new())
        }
        fn message(&self, id: &str) -> Result<crate::mail::MailMessage, MailError> {
            Err(MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
        }
        fn thread(&self, _: &str, _: usize) -> Result<Vec<crate::mail::MailMessage>, MailError> {
            Ok(Vec::new())
        }
        fn attachments(&self, _: &str) -> Result<Vec<crate::mail::AttachmentMeta>, MailError> {
            Ok(Vec::new())
        }
        fn stored(&self, id: &str, _: u64) -> Result<crate::mail::Stored, MailError> {
            Err(MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
        }
        fn attachment(&self, id: &str, _: &str) -> Result<crate::mail::AttachmentFile, MailError> {
            Err(MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
        }
        fn read_attachment(
            &self,
            id: &str,
            _: &str,
        ) -> Result<crate::file_read::FileRead, MailError> {
            Err(MailError::Absent {
                kind: "message",
                name: id.to_owned(),
            })
        }
    }

    fn called(asked: &Asked, args: Value) -> Result<Older, Refusal> {
        let policy = ChainPolicy {
            name: ChainName::new("life").expect("a slug"),
            untrusted: Untrusted::of([Actor::new("triage").expect("an actor")]),
            roster: std::collections::BTreeSet::new(),
            root: crate::cast::keys::root(),
        };
        let deployment = Deployment {
            viewers: std::collections::BTreeSet::new(),
            actors: std::collections::BTreeMap::new(),
            chains: Chains::of(vec![policy.clone()]).expect("one chain"),
            categories: std::collections::BTreeMap::new(),
            app_url: "https://example.test".to_owned(),
            coding: None,
            zone: crate::zone::Zone::UTC,
        };
        let caps = Secretary {
            older: asked,
            mail: &Roster,
            ..Nothing::secretary()
        };
        SearchOlder::run(
            &Call {
                caps: &caps,
                deployment: &deployment,
                settables: &crate::settings::Settables::default(),
                chain: &policy,
                chains: &[],
                conversation: &ConversationId::new("default").expect("a slug"),
            },
            serde_json::from_value(args).expect("arguments"),
        )
    }

    /// A query, a date or an account that is not one is refused before any server is asked.
    #[test]
    fn a_bad_call_asks_no_server() {
        let asked = Asked::default();
        for args in [
            json!({"query": "label:work", "since": "2023-01-01"}),
            json!({"query": "budget", "since": "last year"}),
            json!({"query": "budget", "since": "2999-01-01"}),
            json!({"query": "budget before:2022-01-01", "since": "2023-01-01"}),
            json!({"account": "stranger@example.test", "query": "budget", "since": "2023-01-01"}),
        ] {
            assert!(called(&asked, args.clone()).is_err(), "{args}");
        }
        assert!(asked.0.lock().expect("the log").is_empty());
    }
}
