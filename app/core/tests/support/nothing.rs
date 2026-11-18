use crate::agent::Agent;
use crate::secretary::Secretary;
use crate::secretary::briefings::{BriefingError, Briefings};
use suzatary_base::capability::Nothing;
use suzatary_lines::secretary::conversations;

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
            memory: suzatary_digest::secretary::recollection::Recollection::of(
                suzatary_digest::secretary::memories::Memories::nothing().reads(),
                0,
            ),
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
            memories: suzatary_digest::secretary::memories::Memories::nothing(),
            decisions: suzatary_todo::decisions::Decisions { store: &Nothing },
            topics: suzatary_topics::secretary::topics::stores::TopicStores {
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

impl Briefings for Nothing {
    fn all(&self) -> Result<Vec<(chrono::NaiveDate, String)>, BriefingError> {
        Ok(Vec::new())
    }
    fn read(&self, day: chrono::NaiveDate) -> Result<String, BriefingError> {
        Err(BriefingError::Absent(day.to_string()))
    }
}
