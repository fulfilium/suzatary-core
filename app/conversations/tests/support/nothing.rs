//! The inert model and its sessions: test support, never production.

use suzatary_base::capability::Nothing;

use crate::secretary::model::{Conversation, Model, ModelError, Pick, Reply, SessionId, Sessions};
use crate::secretary::model_sessions::ModelSessions;

impl Model for Nothing {
    fn name(&self) -> String {
        "nothing".to_owned()
    }
    fn turn(
        &self,
        _conversation: &Conversation,
        _tools: &serde_json::Value,
    ) -> Result<Reply, ModelError> {
        Ok(Reply::default())
    }
}

impl Sessions for Nothing {
    fn pick(&self) -> Pick<'_> {
        Pick::Turn(Box::new(self))
    }
}

/// No session is kept, so none is forgotten.
impl ModelSessions for Nothing {
    fn forget(&self, _: &SessionId, _: &str) -> Result<(), String> {
        Ok(())
    }
}
