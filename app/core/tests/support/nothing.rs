use crate::agent::Agent;
use crate::notify::{Delivered, Notify, NotifyError, Push};
use crate::secretary::Secretary;
use crate::secretary::answers::{AnswerError, Answers};
use crate::secretary::briefings::{BriefingError, Briefings};
use suzatary_base::capability::Nothing;
use suzatary_inbox::secretary::responded;
use suzatary_lines::secretary::conversations;
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
                lines: conversations::Conversations::nothing(),
                owners: &NO_OWNERS,
                compacting: &crate::secretary::compacting::Compacting::Off,
            },
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
            conversations: conversations::Conversations::nothing(),
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

/// No owner: nothing here believes a retraction or an erasure.
static NO_OWNERS: std::collections::BTreeSet<prodrome_core::event::Actor> =
    std::collections::BTreeSet::new();

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

impl crate::secretary::google_sign_in::Consents for Nothing {
    fn redirect(&self) -> Option<&suzatary_google::google::RedirectUri> {
        None
    }

    fn begin(
        &self,
        _: &suzatary_inbox::secretary::inbox::ProposalId,
        _: &suzatary_mail::mail::address::EmailAddress,
        _: &suzatary_google::google::Scopes,
        _: &prodrome_core::event::Actor,
        _: suzatary_base::capability::Datetime,
        _: crate::secretary::google_sign_in::Then,
    ) -> responded::Performed {
        responded::Performed::Tried(responded::Tried::NotSent {
            reason: "nothing opens a sign-in with Google here".to_owned(),
        })
    }
}
