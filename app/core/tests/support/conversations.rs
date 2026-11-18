//! The conversations held in memory, for tests: never a store anyone relies on.

use suzatary_conversations::secretary::model_sessions::ModelSessions;
use suzatary_lines::nothing::Counting;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::opened::Opened;
use suzatary_todo::render_cache::RenderCache;

/// The conversations held in memory, for tests.
#[derive(Default)]
pub struct InMemory {
    /// The lines.
    pub lines: crate::stores::InMemory<LineEvent>,
    /// The keys.
    pub keys: suzatary_memory::secretary::locked_text::InMemoryKeys,
    /// Where keys come from.
    pub entropy: Counting,
    /// The blocks and sessions a sweep forgot.
    pub forgot: Forgot,
    /// The words opened.
    pub opened: Opened,
    /// The runs that claim its rounds.
    pub runs: suzatary_lines::nothing::InMemory,
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
    fn forget(&self, block: &suzatary_base::answer::Renderable) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(block.as_str().to_owned());
        Ok(())
    }
}

impl ModelSessions for Forgot {
    fn forget(
        &self,
        id: &suzatary_conversations::secretary::model::SessionId,
        cwd: &str,
    ) -> Result<(), String> {
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
            runs: &self.runs,
        }
    }
}
