//! The mail corpus as the agent operates it: the interpreter of `Corpus` on this box.
//!
//! Its answers are a versioned contract the stateless agent re-binds to every run. Nothing here
//! sends.

use std::path::PathBuf;

use suzatary_base::capability::Clock;
use suzatary_core::mail::corpus::{Corpus, CorpusError};
use suzatary_core::mail::hash_prefix::{HashPrefix, Resolved};
use suzatary_core::mail::known::Known;
use suzatary_core::mail::local_state::Kept;
use suzatary_core::mail::mailbox::Query;
use suzatary_core::mail::state::Decision;
use suzatary_core::mail::verbs;

use crate::mailbox::Notmuch;
use crate::sync::Syncer;

/// The corpus operations, run against this box.
pub struct Operator {
    sync_status: PathBuf,
    pulse: crate::pulse::Pulse,
    /// Read-only.
    mail: Notmuch,
    ledger: crate::state::Ledger,
    syncer: Syncer,
    health: crate::source_health::Health,
    clock: crate::clock::SystemClock,
    /// The owner's zone, whose day the sync window counts back from.
    zone: suzatary_base::zone::Zone,
    watches: crate::watches::Watches,
}

impl std::fmt::Debug for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operator")
            .field("ledger", &self.ledger)
            .finish_non_exhaustive()
    }
}

impl Operator {
    #[must_use]
    pub fn new(
        mail: Notmuch,
        ledger: crate::state::Ledger,
        syncer: Syncer,
        heartbeat: impl Into<PathBuf>,
        sync_status: impl Into<PathBuf>,
        zone: suzatary_base::zone::Zone,
        watches: crate::watches::Watches,
    ) -> Self {
        let heartbeat = heartbeat.into();
        Self {
            sync_status: sync_status.into(),
            health: crate::source_health::beside(&heartbeat),
            pulse: crate::pulse::Pulse::at(&heartbeat),
            mail,
            ledger,
            syncer,
            clock: crate::clock::SystemClock,
            zone,
            watches,
        }
    }

    fn state(&self) -> Result<Known, CorpusError> {
        self.ledger.read().map_err(CorpusError::Refused)
    }

    fn write(&self, known: &Known) -> Result<(), CorpusError> {
        self.ledger.write(known).map_err(CorpusError::Refused)
    }

    fn today(&self) -> chrono::NaiveDate {
        self.zone
            .day(prodrome_core::fpl::instant_of(self.clock.now()))
    }

    /// A one-off widening of the corpus; the CLI's alone, since the routine run must stay cheap.
    pub fn backfill(
        &self,
        email: &str,
        folder: &str,
        subdir: Option<&str>,
        limit: Option<usize>,
    ) -> Result<verbs::Backfilled, CorpusError> {
        let account = self.syncer.accounts().microsoft(email).ok_or_else(|| {
            let known: Vec<&str> = self
                .syncer
                .accounts()
                .accounts()
                .iter()
                .filter(|a| a.microsoft().is_some())
                .map(|a| a.email.as_str())
                .collect();
            CorpusError::Refused(format!(
                "{email} is not a Microsoft account in the roster (have: {})",
                known.join(", ")
            ))
        })?;
        self.syncer
            .backfill(account, folder, subdir, limit)
            .map_err(|failed| CorpusError::Refused(failed.to_string()))
    }

    /// Ring each open watch the corpus now matches. A failure goes to stderr and fails nothing:
    /// the next sync asks again.
    fn ring(&self) {
        let own: Vec<&str> = self
            .syncer
            .accounts()
            .accounts()
            .iter()
            .map(|account| account.email.as_str())
            .collect();
        let now = prodrome_core::fpl::instant_of(self.clock.now());
        match suzatary_core::mail::watch::ring(&self.watches, self, &own, now) {
            Ok(failed) => {
                for refused in failed {
                    eprintln!("sync: a watch did not ring: {}", refused.message());
                }
            }
            Err(refused) => eprintln!("sync: no watch rang: {}", refused.message()),
        }
    }

    #[must_use]
    pub fn syncer(&self) -> &Syncer {
        &self.syncer
    }
}

impl Corpus for Operator {
    /// Every sync records each account's health and fails when it cannot; its report is kept
    /// private for the gate, the briefing and `last_sync`.
    fn sync(&self) -> Result<suzatary_core::mail::sync_report::SyncReport, CorpusError> {
        let report = self.syncer.sync(self.today());
        self.ring();
        let at = prodrome_core::fpl::instant_of(self.clock.now());
        self.health
            .join(&suzatary_core::health::Healths::of_sync(&report, at))
            .map_err(|failed| {
                CorpusError::Refused(format!(
                    "the sync ran, and its accounts' health was not recorded: {failed}"
                ))
            })?;
        let answer = serde_json::to_string(&report).map_err(|failed| {
            CorpusError::Refused(format!("the sync report did not serialise: {failed}"))
        })?;
        crate::durable::replace(
            &self.sync_status,
            answer.as_bytes(),
            crate::durable::Mode::Private,
        )
        .map_err(|failed| {
            CorpusError::Refused(format!(
                "the sync ran, and its report was not kept at {}: {failed}",
                self.sync_status.display()
            ))
        })?;
        Ok(report)
    }

    fn queue(&self, limit: Option<usize>) -> Result<Vec<verbs::Summary>, CorpusError> {
        let state = self.state()?;
        self.mail
            .queue(&state, limit)
            .map_err(|failed| CorpusError::Refused(failed.to_string()))
    }

    fn search(
        &self,
        query: &Query,
        limit: Option<usize>,
    ) -> Result<Vec<verbs::Summary>, CorpusError> {
        self.mail
            .summaries(query.as_str(), limit)
            .map_err(|failed| CorpusError::Refused(failed.to_string()))
    }

    /// One message in full; a miss or an ambiguous prefix is an answer the agent reads.
    fn show(&self, id: &str) -> Result<verbs::Show, CorpusError> {
        self.mail
            .full_message(id)
            .map_err(|failed| CorpusError::Refused(failed.to_string()))
    }

    /// Record a decision about a canonical message; without `native` it takes the locator the
    /// listing that printed `hash` recorded, and then the one an earlier mark kept.
    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        note: Option<&str>,
        native: Option<&str>,
    ) -> Result<verbs::Marked, CorpusError> {
        let state = self.state()?;
        let kept = || {
            state
                .local
                .messages
                .get(hash)
                .and_then(|kept| kept.native_id.clone())
        };
        let locator = match (native, HashPrefix::parse(hash)) {
            (Some(id), _) => Some(id.to_owned()),
            (None, None) => kept(),
            (None, Some(prefix)) => match self
                .mail
                .locate(&prefix)
                .map_err(|failed| CorpusError::Refused(failed.to_string()))?
            {
                Resolved::One(_, id) => Some(id),
                Resolved::Unlisted => kept(),
                Resolved::Ambiguous(ambiguous) => {
                    return Err(CorpusError::Refused(ambiguous.to_string()));
                }
            },
        };
        let disposition = decision.disposition();
        self.write(&state.mark(
            hash,
            decision,
            Kept {
                native_id: locator,
                note: note.unwrap_or_default().to_owned(),
            },
        ))?;
        Ok(verbs::Marked {
            marked: hash.to_owned(),
            disposition,
        })
    }

    fn last_sync(&self) -> Option<suzatary_core::mail::sync_report::SyncReport> {
        crate::mail_sync::read_report(&self.sync_status)
            .ok()
            .flatten()
            .map(|(report, _)| report)
    }

    fn last_triage_pass(&self) -> Option<String> {
        self.pulse.last_pass()
    }

    fn last_triage_outcome(&self) -> suzatary_core::mail::heartbeat::PassOutcome {
        self.pulse.last_outcome()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use suzatary_core::mail::account::{MailAccount, Roster, Sync};

    fn operator(root: &Path) -> Operator {
        let mail = Notmuch::new(
            root.join("Mail"),
            ledger(root),
            crate::mailbox::located::Located::open(root.join("located")),
            roster(),
        )
        .with_binary("/nonexistent/notmuch");
        operator_over(root, mail, ledger(root))
    }

    fn roster() -> Roster {
        Roster::new(vec![MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new("her@example.test")
                .expect("a lower-case address"),
            subdir: "her".to_owned(),
            sync: Sync::Gmail,
        }])
        .expect("a roster")
    }

    fn operator_over(root: &Path, mail: Notmuch, ledger: crate::state::Ledger) -> Operator {
        let syncer = Syncer::new(
            roster(),
            root.join("Mail"),
            "/nonexistent/notmuch",
            root.join("mbsyncrc"),
            root.join("imap"),
            (
                crate::oauth::TokenStore::new(root.join("oauth")),
                crate::google::Grants::new(
                    crate::google::Google::new(suzatary_core::google::Endpoints::google()),
                    std::sync::Arc::new(crate::secrets::Secretspec::new(
                        root.join("secretspec.toml"),
                    )),
                ),
            ),
        );
        Operator::new(
            mail,
            ledger,
            syncer,
            root.join("triage-heartbeat"),
            root.join("last-sync.json"),
            suzatary_base::zone::Zone::UTC,
            crate::watches::Watches::open(root.join("local/watches")),
        )
    }

    fn ledger(root: &Path) -> crate::state::Ledger {
        crate::state::Ledger::new(
            root.join("state.json"),
            root.join("local/mail.json"),
            || {
                Ok(suzatary_core::mail::dedup_key::DedupKey::of(
                    &suzatary_base::capability::Secret::new("12".repeat(32)),
                )
                .expect("a key"))
            },
        )
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-corpus-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("a writable temp directory");
        dir
    }

    fn message(sender: &str, subject: &str, id: &str, days: i64) -> String {
        let date = (chrono::Utc::now() - chrono::Duration::days(days)).to_rfc2822();
        format!(
            "From: {sender}\nTo: ada@example.test\nSubject: {subject}\nMessage-ID: <{id}>\n\
             Date: {date}\n\n{subject}, in the body.\n"
        )
    }

    /// Every hash a listing prints, `show` shows, whole or by prefix; `mark` finds the locator;
    /// a hash no listing printed is not found.
    #[test]
    fn every_hash_a_listing_prints_is_shown() {
        if !crate::mailbox::fixture::notmuch_present("the listed-hash law") {
            return;
        }
        let root = tempfile::tempdir().expect("a scratch directory");
        let (mail, ledger) = crate::mailbox::fixture::indexed(
            root.path(),
            "inbox;unread",
            &[
                (
                    "ada/Inbox/cur/1:2,",
                    message("bank@example.test", "statement", "new-1@example.test", 1),
                ),
                (
                    "ada/Inbox/cur/2:2,S",
                    message(
                        "clinic@example.test",
                        "appointment",
                        "new-2@example.test",
                        2,
                    ),
                ),
                (
                    "ada/Inbox/cur/3:2,S",
                    message("counsel@example.test", "filed", "old-1@example.test", 400),
                ),
            ],
        );
        let operator = operator_over(root.path(), mail, ledger.clone());

        let queued = operator.queue(None).expect("the queue");
        let searched = operator
            .search(&Query::new("*").expect("a query"), None)
            .expect("a search");
        assert_eq!(queued.len(), 2, "the two recent inbox messages: {queued:?}");
        assert_eq!(searched.len(), 3, "every message: {searched:?}");
        for row in queued.iter().chain(&searched) {
            for asked in [row.hash.as_str(), &row.hash[..8]] {
                match operator.show(asked).expect("shows") {
                    verbs::Show::Shown(message) => assert_eq!(message.id, row.id, "{asked}"),
                    other => panic!("{asked}, printed for {}, is {other:?}", row.id),
                }
            }
        }

        let first = &queued[0];
        operator
            .mark(
                &first.hash,
                Decision::parse("skip", None).expect("a decision"),
                None,
                None,
            )
            .expect("marks");
        let known = ledger.read().expect("the ledger");
        assert_eq!(
            known.local.messages[&first.hash].native_id.as_deref(),
            Some(first.id.as_str())
        );

        let unlisted = "f".repeat(64);
        assert_eq!(
            operator.show(&unlisted).expect("answers"),
            verbs::Show::NotFound { id: unlisted }
        );
    }

    #[test]
    fn a_mark_records_the_decision_and_the_locator_it_was_given() {
        let root = scratch("mark");
        let operator = operator(&root);
        let answer = operator
            .mark(
                "abc123",
                Decision::parse("todo", Some("renewinsurance")).expect("a decision"),
                Some("why"),
                Some("msg-1"),
            )
            .expect("marks");
        assert_eq!(
            answer,
            verbs::Marked {
                marked: "abc123".to_owned(),
                disposition: suzatary_core::mail::state::Disposition::Todo,
            }
        );

        let known = ledger(&root).read().expect("reads");
        assert_eq!(
            known.decided("abc123"),
            Some(&Decision::parse("todo", Some("renewinsurance")).expect("a decision"))
        );
        let kept = &known.local.messages["abc123"];
        assert_eq!(kept.note, "why");
        assert_eq!(kept.native_id.as_deref(), Some("msg-1"));
        let pushed = std::fs::read_to_string(root.join("state.json")).expect("reads");
        for private in ["abc123", "why", "msg-1"] {
            assert!(!pushed.contains(private), "{private} was pushed");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
