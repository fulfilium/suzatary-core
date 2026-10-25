//! What the box keeps of a plugin's syncs: the last run, and each link's results joined.
//!
//! Cache-tier bookkeeping beside the plugin's cursor, never pushed. Every field is the host's
//! own observation or the binary's own summary, never a plugin's say-so about itself, so the
//! Settings page's "last synced" means the host saw a 200 and the store took what came of it.
//! A link's results are the semilattice a mail account's are (`mail::health`), so how long it
//! has failed, and of what class, is a function of the set of its fetches.

use prodrome_core::fpl::Instant;
use serde::{Deserialize, Serialize};

use super::Refusal;
use crate::connect::Failure;
use crate::mail::health::{Healths, Outcome};

/// A plugin's ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    /// The last run, whatever came of it.
    pub last: Option<LastRun>,
    /// Each credential the plugin's fetches designated, by name: every fetch's result, joined.
    pub links: Healths,
}

/// One run, summarised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LastRun {
    /// When it ran.
    pub at: Instant,
    /// What came of it.
    pub came: Came,
}

/// What one run came to, each with the journal's line for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Came {
    /// What it learnt was stored.
    Learnt {
        /// The journal's line.
        summary: String,
    },
    /// Nothing was stored.
    Failed {
        /// The journal's line, which says why.
        why: String,
    },
}

/// What one fetch came to, as the host recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// It was answered with this status.
    Status(u16),
    /// It was refused, or its answer withheld, for this reason.
    Refused(Refusal),
}

impl Answer {
    /// What a fetch that came to this means for its link, in a run that `came` to what it did:
    /// a success only where it answered 200 and the store took the run, else a failure of the
    /// class the answer is, with words of the box's.
    #[must_use]
    pub fn outcome(&self, came: &Came) -> (Outcome, String) {
        match (self, came) {
            (Answer::Status(200), Came::Learnt { .. }) => (Outcome::Succeeded, String::new()),
            (Answer::Status(200), Came::Failed { .. }) => (
                Outcome::Failed(Failure::Unclassified),
                "it answered, and what it said was not stored".to_owned(),
            ),
            (Answer::Status(status), _) => (
                Outcome::Failed(Failure::of_status(*status)),
                format!("it answered {status}"),
            ),
            (Answer::Refused(refusal), _) => (
                Outcome::Failed(Failure::of_refusal(refusal)),
                refused(refusal),
            ),
        }
    }
}

/// What a refused fetch says on its line: an address not reached in the owner's words, the
/// host's own words for every other refusal.
fn refused(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Unreachable | Refusal::Lost => "its address could not be reached".to_owned(),
        Refusal::TimedOut => "its address did not answer in time".to_owned(),
        Refusal::BadUrl(_)
        | Refusal::OriginNotAllowed(_)
        | Refusal::CredentialNotDeclared(_)
        | Refusal::CredentialUnset(_)
        | Refusal::CredentialUnavailable(_)
        | Refusal::CredentialNotAUrl(_)
        | Refusal::InTheClear(_)
        | Refusal::Echoed(_)
        | Refusal::TooManyFetches => refusal.to_string(),
    }
}

impl Ledger {
    /// The ledger after `run`, whose fetches came to `fetches`: each designated link's result
    /// joined into what it held.
    #[must_use]
    pub fn after(mut self, run: LastRun, fetches: &[(String, Answer)]) -> Self {
        for (name, answer) in fetches {
            let (outcome, words) = answer.outcome(&run.came);
            self.links = self
                .links
                .join(&Healths::one(name, run.at, outcome, &words));
        }
        self.last = Some(run);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::health::Reading;

    fn at(minute: u32) -> Instant {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 5)
            .and_then(|day| day.and_hms_opt(10, minute, 0))
            .expect("an instant")
    }

    fn run(minute: u32, came: Came) -> LastRun {
        LastRun {
            at: at(minute),
            came,
        }
    }

    fn learnt() -> Came {
        Came::Learnt {
            summary: String::new(),
        }
    }

    fn reading(ledger: &Ledger, link: &str) -> Reading {
        ledger.links.reading(link).expect("a link fetched")
    }

    /// A link is synced only when it answered 200 and the store took the run; every other
    /// answer is a failure of its class since the last success, which a later run moves only
    /// by succeeding.
    #[test]
    fn a_link_is_synced_only_when_it_answered_and_the_store_took_the_run() {
        let home = || "PLUGIN_ICS_HOME".to_owned();
        let ledger = Ledger::default().after(run(0, learnt()), &[(home(), Answer::Status(200))]);
        assert_eq!(reading(&ledger, &home()), Reading::Ok { at: at(0) });
        let failed = ledger.clone().after(
            run(
                15,
                Came::Failed {
                    why: "the run failed".to_owned(),
                },
            ),
            &[(home(), Answer::Status(200))],
        );
        assert_eq!(
            reading(&failed, &home()),
            Reading::Failing {
                at: at(15),
                class: Failure::Unclassified,
                words: "it answered, and what it said was not stored".to_owned(),
                since: at(0),
            }
        );
        let gone = failed.after(
            run(30, learnt()),
            &[(
                home(),
                Answer::Refused(Refusal::CredentialUnset(
                    crate::plugin::CredentialName::parse(
                        &crate::plugin::PluginName::parse("ics").expect("a name"),
                        &home(),
                    )
                    .expect("a credential"),
                )),
            )],
        );
        assert_eq!(
            reading(&gone, &home()),
            Reading::Failing {
                at: at(30),
                class: Failure::Unresolved,
                words: "PLUGIN_ICS_HOME is declared and not set".to_owned(),
                since: at(0),
            }
        );
        let revoked = gone.after(run(45, learnt()), &[(home(), Answer::Status(401))]);
        assert_eq!(
            reading(&revoked, &home()),
            Reading::Failing {
                at: at(45),
                class: Failure::of_status(401),
                words: "it answered 401".to_owned(),
                since: at(0),
            },
            "the episode is the one since the last success, however often it is seen"
        );
    }

    /// An instant reads back as the box's clock printed it, with or without its fraction, so a
    /// ledger's text keeps the spelling it was kept in.
    #[test]
    fn an_instant_reads_as_the_clock_printed_it() {
        let printed = serde_json::to_value(run(0, learnt())).expect("prints");
        assert_eq!(printed["at"], "2026-10-05T10:00:00");
        assert_eq!(
            printed["came"],
            serde_json::json!({"learnt": {"summary": ""}})
        );
        let read: LastRun = serde_json::from_value(serde_json::json!({
            "at": "2026-10-05T10:15:00.000000", "came": {"learnt": {"summary": ""}}
        }))
        .expect("a run");
        assert_eq!(read.at, at(15));
    }
}
