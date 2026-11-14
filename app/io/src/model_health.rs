//! Whether the model each job runs answers, on disk, as a [`Register`] keyed by the model's
//! name; box-local and never pushed.

use std::path::Path;
use std::sync::Arc;

use suzatary_core::health::{Healths, Instant, Outcome, Reading, Register};
use suzatary_store::store::StoreError;

use crate::docs::{DocCodec, Docs};
use crate::health_file::HealthFile;

/// The register, one document (absent is a box that has called no model yet), and what to ring
/// when a call moves an entry's reading between answering and refused.
#[derive(Clone)]
pub struct Answered {
    file: HealthFile<String>,
    moved: Arc<dyn Fn() + Send + Sync>,
}

impl Answered {
    #[must_use]
    pub fn ringing(self, moved: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            moved: Arc::new(moved),
            ..self
        }
    }

    /// Join one call of `entry` at `at`, `outcome` what it came to and `words` why it failed, and
    /// ring when its reading moved between answering and not.
    ///
    /// # Errors
    /// The register's own, when it cannot be read or written.
    pub fn record(
        &self,
        entry: &str,
        at: Instant,
        outcome: Outcome,
        words: &str,
    ) -> Result<(), StoreError> {
        let answering = |healths: &Healths<String>| {
            healths
                .reading(entry)
                .map(|reading| matches!(reading, Reading::Ok { .. }))
        };
        let before = answering(&self.file.healths()?);
        let after = answering(&self.file.join(&Healths::one(entry, at, outcome, words))?);
        if before != after {
            (self.moved)();
        }
        Ok(())
    }
}

impl Register<String> for Answered {
    fn healths(&self) -> Result<Healths<String>, StoreError> {
        self.file.healths()
    }
}

pub const FILE: &str = "model-health.json";

/// The register beside `heartbeat`, its lock beside it, ringing nothing.
#[must_use]
pub fn beside(heartbeat: &Path) -> Answered {
    let path = heartbeat.with_file_name(FILE);
    let lock = path.with_extension("lock");
    let docs = Docs::new("model health", move |()| path.clone(), &DocCodec::JSON);
    Answered {
        file: HealthFile::new(docs, lock),
        moved: Arc::new(|| {}),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use suzatary_core::health::Failure;

    /// A call that answered and later ones that did not read back as their join, from the file,
    /// and the bell rings only when the reading moves.
    #[test]
    fn calls_recorded_into_the_file_read_back_as_their_join() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let rung = Arc::new(AtomicUsize::new(0));
        let ringing = Arc::clone(&rung);
        let store = beside(&dir.path().join("triage-heartbeat")).ringing(move || {
            ringing.fetch_add(1, Ordering::SeqCst);
        });
        let at = |hour| {
            chrono::NaiveDate::from_ymd_opt(2026, 10, 7)
                .and_then(|day| day.and_hms_opt(hour, 0, 0))
                .expect("an instant")
        };
        let entry = "claude-cli/a-model";
        let refused = Outcome::Failed(Failure::KeyRefused);
        store
            .record(entry, at(1), Outcome::Succeeded, "")
            .expect("recorded");
        store
            .record(entry, at(2), refused, "401")
            .expect("recorded");
        store
            .record(entry, at(3), refused, "401")
            .expect("recorded");
        assert!(dir.path().join(FILE).is_file());
        let read = store.healths().expect("readable");
        assert_eq!(
            read.reading(entry),
            Some(Reading::Failing {
                at: at(3),
                class: Failure::KeyRefused,
                words: "401".to_owned(),
                since: at(1),
            })
        );
        assert_eq!(
            rung.load(Ordering::SeqCst),
            2,
            "first answering, then refused; a second refusal moves nothing"
        );
    }
}
