//! What this tab asked and the replicas do not hold yet (`web/src/prodrome/intents.ts`), and what
//! a control draws of it.

use serde::Deserialize;
use ts_rs::TS;

use crate::runtime::{Act, Said};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
pub struct Intents {
    /// In the order first asked.
    pub(crate) asked: Vec<Intent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
pub(crate) struct Intent {
    pub(crate) act: Act,
    /// The answer to the last asking, `None` while it is on its way.
    pub(crate) answer: Option<Said>,
    /// Whether a reading of the replicas holds it.
    pub(crate) retracted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Shown {
    /// None, or the history holds it: the control is drawn from the history alone.
    Absent,
    Pending,
    /// Taken, and not yet read back.
    Taken,
    /// The box answered no: nothing was done.
    Refused {
        e: String,
    },
    /// No answer came: the box may or may not have done it, and its next reading says which.
    Unanswered {
        e: String,
    },
}

impl Shown {
    /// Asked and not answered no.
    #[must_use]
    pub(crate) const fn waits(&self) -> bool {
        matches!(self, Shown::Pending | Shown::Taken)
    }
}

impl Intents {
    /// The first still-open intent `matches` picks; `holds` says which the history holds.
    #[must_use]
    pub(crate) fn about(
        &self,
        matches: impl Fn(&Act) -> bool,
        holds: impl Fn(&Act) -> bool,
    ) -> Shown {
        self.asked
            .iter()
            .filter(|i| matches(&i.act))
            .map(|i| match &i.answer {
                _ if i.retracted || holds(&i.act) => Shown::Absent,
                None => Shown::Pending,
                Some(Said::Taken) => Shown::Taken,
                Some(Said::Refused { e }) => Shown::Refused { e: e.clone() },
                Some(Said::Lost { e }) => Shown::Unanswered { e: e.clone() },
            })
            .find(|s| *s != Shown::Absent)
            .unwrap_or(Shown::Absent)
    }

    /// The name a taken rename gives a conversation, until a reading holds it.
    #[must_use]
    pub(crate) fn renamed(&self, conversation: &str) -> Option<&str> {
        self.asked
            .iter()
            .filter(|i| !i.retracted && i.answer == Some(Said::Taken))
            .filter_map(|i| {
                if let Act::Rename {
                    conversation: c,
                    title,
                } = &i.act
                {
                    (c == conversation).then_some(title.as_str())
                } else {
                    None
                }
            })
            .next_back()
    }
}
