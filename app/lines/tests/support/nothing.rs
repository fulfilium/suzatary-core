//! The inert conversations, and an entropy that counts: test support, never production.

use std::sync::LazyLock;

use suzatary_base::capability::{Entropy, Nothing};

use crate::secretary::conversations::Conversations;
use crate::secretary::opened::Opened;

/// The runs of every test's conversations held in memory.
pub static RUNS: LazyLock<InMemory> = LazyLock::new(InMemory::default);

/// Nothing opened: [`Conversations::nothing`] holds no line to open.
static OPENED: LazyLock<Opened> = LazyLock::new(Opened::default);

impl Conversations<'static> {
    /// The conversations, inert: no line and no key.
    #[must_use]
    pub fn nothing() -> Self {
        Conversations {
            lines: &Nothing,
            keys: &Nothing,
            entropy: &Nothing,
            renders: &Nothing,
            sessions: &Nothing,
            opened: &OPENED,
            runs: &*RUNS,
        }
    }
}

/// Each key drawn one count past the last, so no two are equal: for tests, never a source of
/// keys anyone relies on.
#[derive(Debug, Default)]
pub struct Counting(std::sync::atomic::AtomicU64);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        let count = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = count.to_be_bytes()[at % 8] ^ u8::try_from(at % 251).unwrap_or(0);
        }
        Ok(())
    }
}

/// Runs held in this process's memory, for tests and for a store no other process reads: a run is
/// gone once its [`Held`] drops or [`InMemory::end`] ends it, as a process's death would.
#[derive(Debug, Default, Clone)]
pub struct InMemory {
    live: std::sync::Arc<
        std::sync::Mutex<(
            u64,
            std::collections::BTreeSet<suzatary_conversations::secretary::store::RunId>,
        )>,
    >,
}

impl InMemory {
    /// End `run` as a dead process would: its holder no longer holds it.
    pub fn end(&self, run: &suzatary_conversations::secretary::store::RunId) {
        self.held().1.remove(run);
    }

    fn held(
        &self,
    ) -> std::sync::MutexGuard<
        '_,
        (
            u64,
            std::collections::BTreeSet<suzatary_conversations::secretary::store::RunId>,
        ),
    > {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Ends its run in [`InMemory`] when dropped.
struct Ending {
    live: std::sync::Arc<
        std::sync::Mutex<(
            u64,
            std::collections::BTreeSet<suzatary_conversations::secretary::store::RunId>,
        )>,
    >,
    run: suzatary_conversations::secretary::store::RunId,
}

impl Drop for Ending {
    fn drop(&mut self) {
        self.live
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1
            .remove(&self.run);
    }
}

impl crate::secretary::run::Runs for InMemory {
    fn begin(&self) -> Result<crate::secretary::run::Held, suzatary_store::store::StoreError> {
        let mut held = self.held();
        held.0 += 1;
        let run =
            suzatary_conversations::secretary::store::RunId::of(u128::from(held.0).to_be_bytes());
        held.1.insert(run.clone());
        drop(held);
        Ok(crate::secretary::run::Held::new(
            run.clone(),
            Ending {
                live: std::sync::Arc::clone(&self.live),
                run,
            },
        ))
    }

    fn gone(&self, run: &suzatary_conversations::secretary::store::RunId) -> bool {
        !self.held().1.contains(run)
    }
}
