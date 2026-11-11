//! Who named a conversation.

use serde::{Deserialize, Serialize};

/// Who named a conversation: the title job or a person, never a claim in a request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "who", rename_all = "lowercase")]
pub enum Namer {
    /// The title job, after the first answer, and which model it ran on.
    Job {
        /// [`crate::secretary::model::Model::name`] of the job's model.
        model: String,
    },
    /// A rename, by the actor the gate resolved.
    Person {
        /// That actor.
        actor: String,
    },
}
