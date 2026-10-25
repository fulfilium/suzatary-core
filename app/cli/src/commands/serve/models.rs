//! The Claude models this deployment runs, as the server names them and the Models group lists
//! them.

use suzatary_core::model_name::ModelName;
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::settings::overview::{Calls, ModelJob};

use crate::instance::{Instance, Reserve};

/// What `[secretary]` names: the model the conversation, the reviews and the titles run, and
/// the compactor's.
#[derive(Debug, Clone)]
pub struct Models {
    /// `[secretary] model`.
    pub chat: ModelName,
    /// `[secretary] compact` and the compactor's model.
    pub compacting: Compacting,
    /// `[secretary] reserve`, the share the compactor leaves.
    pub reserve: Reserve,
}

impl Models {
    /// What `instance`'s `[secretary]` names.
    #[must_use]
    pub fn of(instance: &Instance) -> Self {
        Self {
            chat: instance.secretary.model.clone(),
            compacting: instance.secretary.compacting(),
            reserve: instance.secretary.reserve,
        }
    }

    /// The name the chat's calls are recorded under in the model health.
    #[must_use]
    pub fn recorded(&self) -> String {
        suzatary_io::model::name_of(self.chat.as_str())
    }

    /// One line per job, the compactor's saying how much of the subscription it leaves.
    #[must_use]
    pub fn jobs(&self) -> Vec<ModelJob> {
        let compactor = match &self.compacting {
            Compacting::Off => "Off: the conversations are not summarised.".to_owned(),
            Compacting::On { model, .. } => format!(
                "It runs {} and leaves {:.0}% of the subscription for you.",
                model.as_str(),
                self.reserve.get() * 100.0
            ),
        };
        vec![
            ModelJob {
                label: "Chat, reviews and titles".to_owned(),
                detail: format!("They run {} through the claude CLI.", self.chat.as_str()),
                calls: Calls::Recorded {
                    key: self.recorded(),
                    credential: suzatary_io::model::TOKEN.to_owned(),
                },
            },
            ModelJob {
                label: "Compactor".to_owned(),
                detail: compactor,
                calls: Calls::Unrecorded,
            },
        ]
    }
}
