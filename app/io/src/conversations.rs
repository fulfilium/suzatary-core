//! The conversations on disk as one value: the lines, their keys (box-local, never pushed), the
//! render cache, the model's sessions, and the generator for new keys.

use std::path::PathBuf;
use std::sync::Arc;

use suzatary_core::capability::{ModelSessions, RenderCache};
use suzatary_core::secretary::conversations::Conversations;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::opened::Opened;

use crate::objects;
use crate::text_keys::{self, TextKeys};
use crate::texts::SystemEntropy;

/// The lines and their keys, at their two directories.
pub struct ConversationStores {
    /// The lines, pushed.
    pub lines: objects::Objects<LineEvent>,
    /// The keys, box-local, vaulted, never pushed.
    pub keys: TextKeys,
    keys_at: PathBuf,
    /// The rendered answer blocks the server draws through, so an erasure empties it.
    renders: Arc<dyn RenderCache + Send + Sync>,
    sessions: Arc<dyn ModelSessions + Send + Sync>,
    /// One cache per keys directory in the process, so a sweep by any holder forgets the words
    /// for every holder.
    opened: Arc<Opened>,
}

impl ConversationStores {
    #[must_use]
    pub fn open(
        lines: impl Into<PathBuf>,
        keys: PathBuf,
        renders: Arc<dyn RenderCache + Send + Sync>,
        sessions: Arc<dyn ModelSessions + Send + Sync>,
    ) -> Self {
        Self {
            lines: objects::Objects::open(lines),
            keys: text_keys::open(keys.clone()),
            opened: opened_at(&keys),
            keys_at: keys,
            renders,
            sessions,
        }
    }

    /// The same stores opened again, for a second holder of them.
    #[must_use]
    pub fn reopened(&self) -> Self {
        Self::open(
            self.lines.inner().root().to_path_buf(),
            self.keys_at.clone(),
            Arc::clone(&self.renders),
            Arc::clone(&self.sessions),
        )
    }

    /// The stores as one value, keys drawn from the system's generator.
    #[must_use]
    pub fn conversations(&self) -> Conversations<'_> {
        Conversations {
            lines: &self.lines,
            keys: &self.keys,
            entropy: &SystemEntropy,
            renders: &*self.renders,
            sessions: &*self.sessions,
            opened: &self.opened,
        }
    }
}

/// The process-wide cache for the keys at `keys`, shared by every holder so all of them forget an
/// erased slot together.
fn opened_at(keys: &std::path::Path) -> Arc<Opened> {
    static OPENED: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, Arc<Opened>>>,
    > = std::sync::LazyLock::new(std::sync::Mutex::default);
    let at = std::path::absolute(keys).unwrap_or_else(|_| keys.to_path_buf());
    Arc::clone(
        OPENED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(at)
            .or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two holders of the stores at one keys directory hold one cache; another directory's is
    /// another.
    #[test]
    fn one_cache_per_keys_directory() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let renders: Arc<dyn RenderCache + Send + Sync> =
            Arc::new(suzatary_core::capability::Nothing);
        let open = |keys: &str| {
            ConversationStores::open(
                dir.path().join("lines"),
                dir.path().join(keys),
                Arc::clone(&renders),
                Arc::new(suzatary_core::capability::Nothing),
            )
        };
        let (one, two, other) = (open("keys"), open("keys"), open("other-keys"));
        assert!(Arc::ptr_eq(&one.opened, &two.opened));
        assert!(Arc::ptr_eq(&one.opened, &one.reopened().opened));
        assert!(!Arc::ptr_eq(&one.opened, &other.opened));
    }
}
