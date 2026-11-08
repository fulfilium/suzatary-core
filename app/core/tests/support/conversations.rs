//! The conversations held in memory, for tests: never a store anyone relies on.

use super::{Conversations, LineEvent, ModelSessions, Opened, RenderCache};
use crate::capability::Entropy;

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

/// The conversations held in memory, for tests.
#[derive(Default)]
pub struct InMemory {
    /// The lines.
    pub lines: crate::store::InMemory<LineEvent>,
    /// The keys.
    pub keys: crate::store::InMemoryKeys,
    /// Where keys come from.
    pub entropy: Counting,
    /// The blocks and sessions a sweep forgot.
    pub forgot: Forgot,
    /// The words opened.
    pub opened: Opened,
}

/// A render cache and a session store that keep nothing and remember what they were told to
/// forget, for tests.
#[derive(Debug, Default)]
pub struct Forgot(std::sync::Mutex<Vec<String>>);

impl Forgot {
    /// Everything forgotten, in order.
    #[must_use]
    pub fn sources(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl RenderCache for Forgot {
    fn forget(&self, block: &crate::answer::Renderable) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(block.as_str().to_owned());
        Ok(())
    }
}

impl ModelSessions for Forgot {
    fn forget(&self, id: &crate::capability::SessionId, cwd: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!("session {id} in {cwd}"));
        Ok(())
    }
}

impl InMemory {
    /// The stores as one value.
    #[must_use]
    pub fn conversations(&self) -> Conversations<'_> {
        Conversations {
            lines: &self.lines,
            keys: &self.keys,
            entropy: &self.entropy,
            renders: &self.forgot,
            sessions: &self.forgot,
            opened: &self.opened,
        }
    }
}
