//! The inert model and its sessions: test support, never production.

use suzatary_base::capability::Nothing;

use crate::secretary::model::{Conversation, Model, ModelError, Pick, Picker, Reply, SessionId};
use crate::secretary::model_sessions::ModelSessions;

impl Model for Nothing {
    fn name(&self) -> String {
        "nothing".to_owned()
    }
    fn streamed(
        &self,
        _conversation: &Conversation,
        _tools: &serde_json::Value,
        _on_delta: &dyn Fn(&str),
        _halt: &crate::secretary::model::Halt,
    ) -> Result<Reply, ModelError> {
        Ok(Reply::default())
    }
}

impl Picker for Nothing {
    fn pick(&self) -> Pick<'_> {
        Box::new(self)
    }
}

/// No session is kept, so none is forgotten.
impl ModelSessions for Nothing {
    fn forget(&self, _: &SessionId, _: &str) -> Result<(), String> {
        Ok(())
    }
}
