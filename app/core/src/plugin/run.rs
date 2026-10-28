//! What one plugin run came to, and what it says of the plugin and of each link it fetched.
//!
//! Every result is the host's own observation or the binary's own summary, never a plugin's
//! say-so about itself, so the Settings page's "last synced" means the host saw a 200 and the
//! store took what came of it. Each joins the sources' health register under its [`SourceKey`],
//! so how long a link or a plugin has failed, and of what class, is a function of the set of its
//! runs.

use prodrome_core::fpl::Instant;

use super::{CredentialName, Manifest, Refusal};
use crate::connect::Failure;
use crate::health::{Healths, Outcome};
use crate::source_key::SourceKey;

/// One run, summarised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastRun {
    /// When it ran.
    pub at: Instant,
    /// What came of it.
    pub came: Came,
}

/// What one run came to, each with the journal's line for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Came {
    /// What it learnt was stored.
    Learnt,
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
            (Answer::Status(200), Came::Learnt) => (Outcome::Succeeded, String::new()),
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

impl LastRun {
    /// What this run says of `plugin`, whose run is a failure of its words unless what it
    /// learnt was stored, and of each link its `fetches` designated.
    #[must_use]
    pub fn health(
        &self,
        plugin: &Manifest,
        fetches: &[(CredentialName, Answer)],
    ) -> Healths<SourceKey> {
        let ran = match &self.came {
            Came::Learnt => (Outcome::Succeeded, ""),
            Came::Failed { why } => (Outcome::Failed(Failure::Unclassified), why.as_str()),
        };
        let run = Healths::one(plugin.ran(), self.at, ran.0, ran.1);
        fetches
            .iter()
            .map(|(credential, answer)| {
                let (outcome, words) = answer.outcome(&self.came);
                Healths::one(plugin.link(credential), self.at, outcome, &words)
            })
            .fold(run, |sum, one| sum.join(&one))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::Reading;

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
        Came::Learnt
    }

    fn ics() -> Manifest {
        Manifest::declare("ics", "ics.wasm", "ics.schema", &[]).expect("a manifest")
    }

    fn home() -> CredentialName {
        CredentialName::parse(ics().name(), "PLUGIN_ICS_HOME").expect("a credential")
    }

    /// `healths` after `run`, which fetched the home link and was answered `answer`.
    fn after(healths: &Healths<SourceKey>, run: &LastRun, answer: Answer) -> Healths<SourceKey> {
        healths.join(&run.health(&ics(), &[(home(), answer)]))
    }

    fn reading(healths: &Healths<SourceKey>) -> Reading {
        healths
            .reading(&ics().link(&home()))
            .expect("a link fetched")
    }

    /// A link is synced only when it answered 200 and the store took the run; every other
    /// answer is a failure of its class since the last success, which a later run moves only
    /// by succeeding.
    #[test]
    fn a_link_is_synced_only_when_it_answered_and_the_store_took_the_run() {
        let synced = after(&Healths::default(), &run(0, learnt()), Answer::Status(200));
        assert_eq!(reading(&synced), Reading::Ok { at: at(0) });
        let failed = after(
            &synced,
            &run(
                15,
                Came::Failed {
                    why: "the run failed".to_owned(),
                },
            ),
            Answer::Status(200),
        );
        assert_eq!(
            reading(&failed),
            Reading::Failing {
                at: at(15),
                class: Failure::Unclassified,
                words: "it answered, and what it said was not stored".to_owned(),
                since: at(0),
            }
        );
        let gone = after(
            &failed,
            &run(30, learnt()),
            Answer::Refused(Refusal::CredentialUnset(home())),
        );
        assert_eq!(
            reading(&gone),
            Reading::Failing {
                at: at(30),
                class: Failure::Unresolved,
                words: "PLUGIN_ICS_HOME is declared and not set".to_owned(),
                since: at(0),
            }
        );
        let revoked = after(&gone, &run(45, learnt()), Answer::Status(401));
        assert_eq!(
            reading(&revoked),
            Reading::Failing {
                at: at(45),
                class: Failure::of_status(401),
                words: "it answered 401".to_owned(),
                since: at(0),
            },
            "the episode is the one since the last success, however often it is seen"
        );
    }
}
