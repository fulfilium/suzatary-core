//! The mail corpus as the agent operates it: the interpreter of `Corpus` on this box.
//!
//! It only composes the notmuch reads, the state writes and the sync; its answers are a versioned
//! contract the stateless agent re-binds to every run. Nothing here sends.

use std::path::PathBuf;

use suzatary_core::capability::{Clock, Corpus, CorpusError, Query};
use suzatary_core::mail::known::Known;
use suzatary_core::mail::local_state::Kept;
use suzatary_core::mail::state::Decision;
use suzatary_core::mail::verbs;

use crate::mailbox::Notmuch;
use crate::sync::Syncer;

/// The corpus operations, run against this box.
pub struct Operator {
    sync_status: PathBuf,
    /// How the triage passes have ended.
    pulse: crate::pulse::Pulse,
    /// The corpus, read-only.
    mail: Notmuch,
    /// The triage's bookkeeping, for the writes `mark` and `noise add` perform.
    ledger: crate::state::Ledger,
    /// The pull.
    syncer: Syncer,
    /// Each account's sync health, which every sync joins its report into.
    health: crate::source_health::Health,
    /// The clock, for the sync window.
    clock: crate::clock::SystemClock,
    /// The owner's zone, whose day the sync window counts back from.
    zone: suzatary_core::zone::Zone,
    /// The watches every sync rings.
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
    /// The corpus, the dedup authority, the syncer, where a triage pass
    /// marks that it reached its trusted push, where each sync keeps its report, the
    /// owner's zone, and the watches each sync rings.
    #[must_use]
    pub fn new(
        mail: Notmuch,
        ledger: crate::state::Ledger,
        syncer: Syncer,
        heartbeat: impl Into<PathBuf>,
        sync_status: impl Into<PathBuf>,
        zone: suzatary_core::zone::Zone,
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

    /// The triage's bookkeeping, read fresh.
    fn state(&self) -> Result<Known, CorpusError> {
        self.ledger.read().map_err(CorpusError::Refused)
    }

    fn write(&self, known: &Known) -> Result<(), CorpusError> {
        self.ledger.write(known).map_err(CorpusError::Refused)
    }

    /// The owner's today, as the syncer's window is measured from.
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

    /// Ring each open watch that the corpus now holds a match for. A failure is said on stderr
    /// and fails nothing: the pull succeeded, and the next sync asks the same question again.
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

    /// The syncer, for the CLI's own `sync`.
    #[must_use]
    pub fn syncer(&self) -> &Syncer {
        &self.syncer
    }
}

impl Corpus for Operator {
    /// Every sync, the timer's and the one `sync_mail` starts early alike, records each
    /// account's health; one whose health is not recorded fails rather than passing silently.
    /// Its report is kept where [`Operator::new`] was told, readable by this user alone, for the
    /// gate, the briefing and the agent's `last_sync`.
    fn sync(&self) -> Result<verbs::SyncReport, CorpusError> {
        let report = self.syncer.sync(self.today());
        self.ring();
        let at = prodrome_core::fpl::instant_of(self.clock.now());
        self.health.join(&report.healths(at)).map_err(|failed| {
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

    /// One message in full; a miss or an ambiguous prefix is an answer the agent reads, the
    /// latter naming the colliding hashes so it can correct itself.
    fn show(&self, id: &str) -> Result<verbs::Show, CorpusError> {
        let state = self.state()?;
        self.mail
            .full_message(id, &state)
            .map_err(|failed| CorpusError::Refused(failed.to_string()))
    }

    /// Record a decision about a canonical message; without `native` it reuses a recorded
    /// locator, and only then pays for a rescan, which a write may and a read may not.
    fn mark(
        &self,
        hash: &str,
        decision: Decision,
        note: Option<&str>,
        native: Option<&str>,
    ) -> Result<verbs::Marked, CorpusError> {
        let state = self.state()?;
        let known = state.local.messages.get(hash);
        let locator = match native {
            Some(id) => Some(id.to_owned()),
            None => match known.and_then(|kept| kept.native_id.clone()) {
                Some(id) => Some(id),
                None => self
                    .mail
                    .scan_for_hash(hash)
                    .map_err(|failed| CorpusError::Refused(failed.to_string()))?,
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

    fn noise_add(&self, sender: &str) -> Result<verbs::NoiseAdded, CorpusError> {
        let known = self.state()?;
        self.write(&Known {
            local: known.local.add_noise(sender),
            ..known
        })?;
        Ok(verbs::NoiseAdded {
            noise_added: sender.to_lowercase(),
        })
    }

    fn last_sync(&self) -> Option<verbs::SyncReport> {
        crate::mail_sync::read_report(&self.sync_status)
            .ok()
            .flatten()
            .map(|(report, _)| report)
    }

    fn last_triage_pass(&self) -> Option<String> {
        self.pulse.last_pass()
    }

    fn last_triage_outcome(&self) -> suzatary_core::secretary::heartbeat::PassOutcome {
        self.pulse.last_outcome()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use suzatary_core::mail::account::{MailAccount, Roster, Sync};

    fn operator(root: &Path) -> Operator {
        let roster = Roster::new(vec![MailAccount {
            email: suzatary_core::mail::account_email::AccountEmail::new("her@example.test")
                .expect("a lower-case address"),
            subdir: "her".to_owned(),
            sync: Sync::Gmail,
        }])
        .expect("a roster");
        let mail = Notmuch::new(root.join("Mail"), ledger(root), roster.clone())
            .with_binary("/nonexistent/notmuch");
        let syncer = Syncer::new(
            roster,
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
            ledger(root),
            syncer,
            root.join("triage-heartbeat"),
            root.join("last-sync.json"),
            suzatary_core::zone::Zone::UTC,
            crate::watches::Watches::open(root.join("local/watches")),
        )
    }

    fn ledger(root: &Path) -> crate::state::Ledger {
        crate::state::Ledger::new(
            root.join("state.json"),
            root.join("local/mail.json"),
            || {
                Ok(suzatary_core::mail::dedup_key::DedupKey::of(
                    &suzatary_core::capability::Secret::new("12".repeat(32)),
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

    #[test]
    fn adding_a_noise_sender_answers_the_stored_spelling() {
        let root = scratch("noise");
        let operator = operator(&root);
        assert_eq!(
            operator.noise_add("Alerts@Example.TEST").expect("adds"),
            verbs::NoiseAdded {
                noise_added: "alerts@example.test".to_owned()
            }
        );
        assert!(
            ledger(&root)
                .read()
                .expect("reads")
                .is_noise("alerts@example.test")
        );
        let pushed = std::fs::read_to_string(root.join("state.json")).expect("reads");
        assert!(!pushed.contains("alerts"), "a sender is box-local");
        let _ = std::fs::remove_dir_all(&root);
    }
}
