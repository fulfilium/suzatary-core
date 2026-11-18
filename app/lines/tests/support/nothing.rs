//! The inert conversations, and an entropy that counts: test support, never production.

use std::sync::LazyLock;

use suzatary_base::capability::{Entropy, Nothing};

use crate::secretary::conversations::Conversations;
use crate::secretary::opened::Opened;

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
