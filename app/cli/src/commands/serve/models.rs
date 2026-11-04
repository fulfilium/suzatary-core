//! The Claude models this deployment runs.

use suzatary_core::model_name::ModelName;
use suzatary_core::secretary::compacting::Compacting;
use suzatary_core::settings::overview::{Calls, ModelJob};

use crate::instance::{Instance, Reserve};

/// The models `[secretary]` names.
#[derive(Debug, Clone)]
pub struct Models {
    /// `[secretary] model`: the conversation, the reviews and the titles.
    pub chat: ModelName,
    pub compacting: Compacting,
    /// `[secretary] reserve`, the share the compactor leaves.
    pub reserve: Reserve,
}

impl Models {
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

    #[must_use]
    pub fn jobs(&self) -> Vec<ModelJob> {
        let compactor = match &self.compacting {
            Compacting::Off => "Off: the conversations are not summarised.".to_owned(),
            Compacting::On { ladder, .. } => format!(
                "It runs {}, each span by the next model when one cannot say it in a line, and \
                 leaves {:.0}% of the subscription for you.",
                ladder
                    .map(|model| model.as_str().to_owned())
                    .as_slice()
                    .join(", then "),
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
