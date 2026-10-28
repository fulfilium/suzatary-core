//! The triage's bookkeeping on disk: `state.json`, the pushed dedup authority, and the
//! box-local `mail.json` beside the other files no push carries, each a [`Doc`] whose value
//! and text are [`suzatary_core::mail`]'s, read together through the dedup key as one
//! [`Known`].
//!
//! Neither can be rebuilt, so each is replaced atomically: a truncated `state.json` would read
//! as nothing decided and re-triage the corpus.
//!
//! [`Doc`]: suzatary_core::store::Doc

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use suzatary_core::mail::dedup_key::{DEDUP_KEY, DedupKey};
use suzatary_core::mail::known::Known;
use suzatary_core::mail::local_state::LocalState;
use suzatary_core::mail::state::TriageState;
use suzatary_core::store::{Doc as _, Gets as _};

use crate::docs::{DocCodec, Docs};
use crate::secrets::Resolves;

/// The dedup authority, one document; absent is a box that has triaged nothing.
pub type State = Docs<(), TriageState>;

/// The box-local rest, one document; absent is a box that has kept nothing.
pub type Local = Docs<(), LocalState>;

impl State {
    /// The document at `path`.
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        Docs::new("triage state", move |()| path.clone(), &PUSHED)
    }
}

impl Local {
    /// The document at `path`.
    #[must_use]
    pub fn open(path: PathBuf) -> Self {
        Docs::new("the mail's box-local state", move |()| path.clone(), &LOCAL).private()
    }
}

const PUSHED: DocCodec<(), TriageState> = DocCodec {
    print: |(), state| Ok(state.print().into_bytes()),
    parse: |(), bytes| {
        let text = String::from_utf8(bytes).map_err(|failed| failed.to_string())?;
        TriageState::parse(&text)
            .map(Some)
            .map_err(|failed| failed.to_string())
    },
};

const LOCAL: DocCodec<(), LocalState> = DocCodec {
    print: |(), state| Ok(state.print().into_bytes()),
    parse: |(), bytes| {
        let text = String::from_utf8(bytes).map_err(|failed| failed.to_string())?;
        LocalState::parse(&text)
            .map(Some)
            .map_err(|failed| failed.to_string())
    },
};

/// Where the key comes from: asked once it is first needed, and the first answer kept, so a
/// store locked at startup costs nothing until a read needs the key.
type Source = Box<dyn Fn() -> Result<DedupKey, String> + Send + Sync>;

/// Both documents and the key that joins them. Cloning shares the key.
#[derive(Clone)]
pub struct Ledger {
    pushed: State,
    local: Local,
    key: Arc<(OnceLock<DedupKey>, Source)>,
}

impl std::fmt::Debug for Ledger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ledger")
            .field("pushed", &self.pushed.path(&()))
            .field("local", &self.local.path(&()))
            .finish_non_exhaustive()
    }
}

impl Ledger {
    /// The documents at `pushed` and `local`, the key `key` gives.
    #[must_use]
    pub fn new(
        pushed: PathBuf,
        local: PathBuf,
        key: impl Fn() -> Result<DedupKey, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            pushed: State::open(pushed),
            local: Local::open(local),
            key: Arc::new((OnceLock::new(), Box::new(key))),
        }
    }

    /// The documents, the key resolved through `secrets` by [`DEDUP_KEY`].
    #[must_use]
    pub fn resolving(
        pushed: PathBuf,
        local: PathBuf,
        secrets: impl Resolves + Send + Sync + 'static,
    ) -> Self {
        Self::new(pushed, local, move || {
            let secret = secrets
                .get(DEDUP_KEY, "the triage names messages by it")
                .map_err(|locked| format!("{DEDUP_KEY}: {locked}"))?
                .ok_or_else(|| {
                    format!("{DEDUP_KEY} does not resolve: `suzatary device init` mints it")
                })?;
            DedupKey::of(&secret).map_err(|e| e.to_string())
        })
    }

    /// The key, asked of its source until it answers, then kept.
    ///
    /// # Errors
    /// A key that does not resolve.
    pub fn read_key(&self) -> Result<DedupKey, String> {
        if let Some(key) = self.key.0.get() {
            return Ok(key.clone());
        }
        let key = (self.key.1)()?;
        Ok(self.key.0.get_or_init(|| key).clone())
    }

    /// Both documents, read fresh (other processes write them), and the key.
    ///
    /// # Errors
    /// A document that does not read, or a key that does not resolve.
    pub fn read(&self) -> Result<Known, String> {
        let pushed = self.pushed.get(&()).map_err(|e| e.to_string())?;
        let local = self.local.get(&()).map_err(|e| e.to_string())?;
        Ok(Known {
            pushed: pushed.unwrap_or_default(),
            local: local.unwrap_or_default(),
            key: self.read_key()?,
        })
    }

    /// Write both, the box-local one first: a crash between them leaves a message the pushed
    /// file has not recorded, which is triaged again, never a decision with no hash behind it.
    ///
    /// # Errors
    /// A document that does not write.
    pub fn write(&self, known: &Known) -> Result<(), String> {
        let local = self.local.path(&());
        if let Some(tier) = local.parent() {
            crate::tier::ignored(tier).map_err(|e| format!("{}: {e}", tier.display()))?;
        }
        self.local
            .set(&(), &known.local)
            .map_err(|e| e.to_string())?;
        self.pushed
            .set(&(), &known.pushed)
            .map_err(|e| e.to_string())
    }

    /// Where the pushed document is.
    #[must_use]
    pub fn pushed_path(&self) -> PathBuf {
        self.pushed.path(&())
    }

    /// Where the box-local document is.
    #[must_use]
    pub fn local_path(&self) -> PathBuf {
        self.local.path(&())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::capability::Secret;
    use suzatary_core::mail::local_state::Kept;
    use suzatary_core::mail::state::Decision;
    use suzatary_core::store::StoreError;

    fn key() -> DedupKey {
        DedupKey::of(&Secret::new("ef".repeat(32))).expect("a key")
    }

    fn scratch() -> (tempfile::TempDir, Ledger) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let ledger = Ledger::new(
            dir.path().join("state.json"),
            dir.path().join("local/mail.json"),
            || Ok(key()),
        );
        (dir, ledger)
    }

    #[test]
    fn a_saved_ledger_reads_back_and_each_file_is_its_document() {
        let (dir, ledger) = scratch();
        let known = ledger.read().expect("reads").mark(
            "abc123",
            Decision::Skip,
            Kept {
                native_id: Some("msg-1".to_owned()),
                note: String::new(),
            },
        );
        let known = Known {
            local: known.local.add_noise("loud@example.test"),
            ..known
        };
        ledger.write(&known).expect("writes");
        assert_eq!(ledger.read().expect("reads"), known);
        let pushed = std::fs::read_to_string(dir.path().join("state.json")).expect("reads");
        assert_eq!(pushed, known.pushed.print());
        assert!(!pushed.contains("abc123") && !pushed.contains("loud@"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("local/mail.json")).expect("reads"),
            known.local.print()
        );
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(dir.path().join("local/mail.json"))
                .expect("stat")
                .permissions(),
        );
        assert_eq!(
            mode & 0o777,
            0o600,
            "the box-local file is this user's alone"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("local/.gitignore")).expect("reads"),
            "*\n",
            "the tier keeps itself out of every push"
        );
    }

    /// A key that does not resolve is a refusal, never a ledger read as empty; it is asked
    /// again at the next read, and the first answer is kept.
    #[test]
    fn a_key_that_does_not_resolve_refuses_the_read_and_is_asked_again() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        let ledger = Ledger::new(
            dir.path().join("state.json"),
            dir.path().join("mail.json"),
            move || {
                let n = counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Err("locked".to_owned())
                } else {
                    Ok(key())
                }
            },
        );
        assert_eq!(ledger.read().expect_err("locked"), "locked");
        ledger.read().expect("unlocked");
        ledger.read().expect("kept");
        assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn a_file_that_is_not_a_state_document_is_a_refusal_and_not_an_empty_state() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let path = dir.path().join("state.json");
        std::fs::write(&path, "not json at all").expect("writes");
        assert!(matches!(
            State::open(path).get(&()),
            Err(StoreError::Unreadable { .. })
        ));
    }

    /// A temporary left beside the real one would be an unreviewed copy of the dedup authority.
    #[test]
    fn no_temporary_is_left_behind() {
        let (dir, ledger) = scratch();
        ledger
            .write(&ledger.read().expect("reads"))
            .expect("writes");
        assert_eq!(std::fs::read_dir(dir.path()).expect("lists").count(), 2);
    }
}
