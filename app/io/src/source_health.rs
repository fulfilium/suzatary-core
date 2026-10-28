//! Every source's health on disk, as one [`Register`] keyed by [`SourceKey`]: the value and its
//! join are [`suzatary_core::health`]'s. It lives beside the triage heartbeat, box-local and
//! never pushed, because it is this box's account of its own reads.
//!
//! [`Register`]: suzatary_core::health::Register

use std::path::Path;

use suzatary_core::source_key::SourceKey;

use crate::docs::{DocCodec, Docs};
use crate::health_file::HealthFile;

/// The register; absent is a box that has read from nothing yet.
pub type Health = HealthFile<SourceKey>;

/// Its file's name, beside the heartbeat as the triage outcome is.
pub const FILE: &str = "source-health.json";

/// The register beside `heartbeat`, its lock beside it.
#[must_use]
pub fn beside(heartbeat: &Path) -> Health {
    let path = heartbeat.with_file_name(FILE);
    let lock = path.with_extension("lock");
    let docs = Docs::new("source health", move |()| path.clone(), &DocCodec::JSON);
    HealthFile::new(docs, lock)
}

#[cfg(test)]
mod tests {
    use suzatary_core::connect::Failure;
    use suzatary_core::health::{Healths, Outcome, Reading, Register as _};
    use suzatary_core::mail::account_email::AccountEmail;
    use suzatary_core::mail::verbs::{AccountSync, SyncReport};

    use super::*;

    fn email(name: &str) -> AccountEmail {
        AccountEmail::new(&format!("{name}@example.test")).expect("an address")
    }

    fn at(hour: u32) -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 1)
            .and_then(|day| day.and_hms_opt(hour, 0, 0))
            .expect("an instant")
    }

    fn report(ok: bool) -> SyncReport {
        SyncReport {
            unheld: None,
            synced: vec![AccountSync {
                account: email("ada"),
                outcome: if ok {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed(Failure::NeedsLogin)
                },
                tail: if ok { "done" } else { "IMAP: not connected" }.to_owned(),
            }],
            notmuch: String::new(),
        }
    }

    /// A sync and a Drive probe recorded one after the other read back from the file, each
    /// under its own key, as the join of what was recorded under it.
    #[test]
    fn results_recorded_into_the_file_read_back_as_their_join() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let store = beside(&dir.path().join("triage-heartbeat"));
        for (ok, hour) in [(true, 1), (false, 2)] {
            store.join(&report(ok).healths(at(hour))).expect("recorded");
        }
        store
            .join(&Healths::one(
                SourceKey::Drive,
                at(3),
                Outcome::Succeeded,
                "",
            ))
            .expect("joined");
        assert!(dir.path().join(FILE).is_file());
        let read = store.healths().expect("readable");
        assert_eq!(
            read.reading(&SourceKey::Mail(email("ada"))),
            Some(Reading::Failing {
                at: at(2),
                class: Failure::NeedsLogin,
                words: "IMAP: not connected".to_owned(),
                since: at(1),
            })
        );
        assert_eq!(
            read.reading(&SourceKey::Drive),
            Some(Reading::Ok { at: at(3) })
        );
    }

    /// Writers that join at once, each through its own handle as each process opens its own,
    /// lose none of each other's results: every join's read and write is one critical section.
    #[test]
    fn writers_that_join_at_once_lose_no_result() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let heartbeat = dir.path().join("triage-heartbeat");
        let writers: Vec<_> = (0..4)
            .map(|writer| {
                let store = beside(&heartbeat);
                std::thread::spawn(move || {
                    for n in 0..25 {
                        let key = SourceKey::Mail(email(&format!("w{writer}n{n}")));
                        store
                            .join(&Healths::one(key, at(1), Outcome::Succeeded, ""))
                            .expect("joined");
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().expect("a writer finished");
        }
        let read = beside(&heartbeat).healths().expect("readable");
        assert_eq!(read.readings().count(), 100);
    }
}
