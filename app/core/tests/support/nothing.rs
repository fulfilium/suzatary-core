use crate::agent::Agent;
use crate::notify::{Delivered, Notify, NotifyError, Push};
use crate::secretary::Secretary;
use crate::secretary::answers::{AnswerError, Answers};
use crate::secretary::briefings::{BriefingError, Briefings};
use crate::secretary::model::{Conversation, Model, ModelError, Reply, Sessions};
use suzatary_base::capability::Nothing;
use suzatary_todo::render::Html;

impl Agent<'static> {
    /// The agent's record, every field inert.
    #[must_use]
    pub fn nothing() -> Self {
        Agent {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            memory: crate::secretary::recollection::Recollection::of(
                crate::secretary::memories::Memories::nothing().reads(),
                0,
            ),
        }
    }
}

impl crate::secretary::memories::Memories<'static> {
    /// The memory and the digest, inert: no fact, no line and no summary.
    #[must_use]
    pub fn nothing() -> Self {
        crate::secretary::memories::Memories {
            history: &Nothing,
            texts: &Nothing,
            cards: &Nothing,
            entropy: &Nothing,
            digests: crate::secretary::digests::Digests {
                history: &Nothing,
                texts: &Nothing,
                lines: crate::secretary::conversations::Conversations::nothing(),
                owners: &NO_OWNERS,
                compacting: &crate::secretary::compacting::Compacting::Off,
            },
        }
    }
}

impl crate::secretary::conversations::Conversations<'static> {
    /// The conversations, inert: no line and no key.
    #[must_use]
    pub fn nothing() -> Self {
        crate::secretary::conversations::Conversations {
            lines: &Nothing,
            keys: &Nothing,
            entropy: &Nothing,
            renders: &Nothing,
            sessions: &Nothing,
            opened: &OPENED,
        }
    }
}

impl Secretary<'static> {
    /// The secretary's record, every field inert.
    #[must_use]
    pub fn nothing() -> Self {
        Secretary {
            chain: &Nothing,
            corpus: &Nothing,
            mail: &Nothing,
            repository: &Nothing,
            manuals: &Nothing,
            render: &Nothing,
            clock: &Nothing,
            model: &Nothing,
            conversations: crate::secretary::conversations::Conversations::nothing(),
            inbox: &Nothing,
            memories: crate::secretary::memories::Memories::nothing(),
            decisions: suzatary_todo::decisions::Decisions { store: &Nothing },
            topics: crate::secretary::topics::stores::TopicStores {
                memberships: &Nothing,
                aliases: &Nothing,
            },
            briefings: &Nothing,
            weather: &Nothing,
            calendar: &Nothing,
            secrets: &Nothing,
            web: &Nothing,
            repositories: &Nothing,
            drive: &Nothing,
            junk: &Nothing,
            sync: &Nothing,
            health: &Nothing,
            older: &Nothing,
            uploads: &Nothing,
            upload_index: &Nothing,
            kept: &Nothing,
            pdf: &Nothing,
            tapped: &Nothing,
            configuration: &Nothing,
            watches: &Nothing,
        }
    }
}

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
    fn pick(&self) -> crate::secretary::model::Pick<'_> {
        crate::secretary::model::Pick::Turn(Box::new(self))
    }
}

/// No owner: nothing here believes a retraction or an erasure.
static NO_OWNERS: std::collections::BTreeSet<prodrome_core::event::Actor> =
    std::collections::BTreeSet::new();

/// No session is kept, so none is forgotten.
impl crate::secretary::model_sessions::ModelSessions for Nothing {
    fn forget(&self, _: &crate::secretary::model::SessionId, _: &str) -> Result<(), String> {
        Ok(())
    }
}

impl Briefings for Nothing {
    fn all(&self) -> Result<Vec<(chrono::NaiveDate, String)>, BriefingError> {
        Ok(Vec::new())
    }
    fn read(&self, day: chrono::NaiveDate) -> Result<String, BriefingError> {
        Err(BriefingError::Absent(day.to_string()))
    }
}

impl Notify for Nothing {
    fn push(&self, _message: &Push) -> Result<Delivered, NotifyError> {
        Err(NotifyError::Unreachable("nothing is pushed".into()))
    }
}

impl Answers for Nothing {
    fn block(&self, block: &suzatary_base::answer::Renderable) -> Result<Html, AnswerError> {
        Err(AnswerError(format!("nothing renders {block:?}")))
    }
}

/// Nothing opened: [`Nothing::conversations`] holds no line to open.
static OPENED: std::sync::LazyLock<crate::secretary::opened::Opened> =
    std::sync::LazyLock::new(crate::secretary::opened::Opened::default);

impl crate::secretary::google_sign_in::Consents for Nothing {
    fn redirect(&self) -> Option<&suzatary_google::google::RedirectUri> {
        None
    }

    fn begin(
        &self,
        _: &crate::secretary::inbox::ProposalId,
        _: &suzatary_mail::mail::address::EmailAddress,
        _: &suzatary_google::google::Scopes,
        _: &prodrome_core::event::Actor,
        _: suzatary_base::capability::Datetime,
        _: crate::secretary::google_sign_in::Then,
    ) -> crate::secretary::responded::Performed {
        crate::secretary::responded::Performed::Tried(crate::secretary::responded::Tried::NotSent {
            reason: "nothing opens a sign-in with Google here".to_owned(),
        })
    }
}
