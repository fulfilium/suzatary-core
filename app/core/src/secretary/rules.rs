//! The conversation's rules: what the box owes a conversation's reading, run by one engine
//! over the conversations store.
//!
//! Each rule's outcome is a line of the transcript it reads, so writing it moves the reading
//! past the one it fired on and discharges the rule there. Order is declaration order, so an
//! answer comes before the title that names it.

use crate::conversation_id::ConversationId;
use crate::rule::Rule;
use crate::secretary::transcript::Transcript;

/// One of the box's own rules over a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversational {
    /// A line that wakes a model is owed an answer: run a round, whose closing turn (an answer
    /// with no calls, or "did not finish") is the outcome.
    Answer,
    /// An unnamed topic has its first answer: run the title job, whose `Titled` line is the
    /// outcome.
    Title,
}

/// Every conversation rule, in the order a reading is tried against them.
pub const RULES: [Conversational; 2] = [Conversational::Answer, Conversational::Title];

impl Rule for Conversational {
    type Key = ConversationId;
    type Reading = Transcript;
    type Witness = ();

    fn name(&self) -> &'static str {
        match self {
            Conversational::Answer => "answer",
            Conversational::Title => "title",
        }
    }

    fn holds(&self, reading: &Transcript) -> bool {
        match self {
            Conversational::Answer => reading
                .owed()
                .iter()
                .any(|(_, said)| crate::secretary::queue::wakes(said)),
            Conversational::Title => crate::secretary::title::owed(reading.stored()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::Turn;
    use crate::rule::{Engine, Named};
    use crate::secretary::store::{Entry, Kind, Namer, Title};
    use crate::secretary::transcript;

    fn id() -> ConversationId {
        ConversationId::new("2026-10-05-1").expect("a slug")
    }

    fn store() -> crate::secretary::conversations::InMemory {
        let store = crate::secretary::conversations::InMemory::default();
        transcript::start(
            store.conversations(),
            &id(),
            &Entry::Opened {
                at: crate::capability::Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept(String::new()),
                model: "m".to_owned(),
                kind: Kind::Topic,
            },
        )
        .expect("opens");
        store
    }

    fn say(store: &crate::secretary::conversations::InMemory, turn: Turn) {
        transcript::push(
            store.conversations(),
            &id(),
            &Entry::Said {
                at: crate::capability::Datetime::new(2026, 10, 5, 9, 1, 0, 0).expect("an instant"),
                turn,
                by: None,
            },
        )
        .expect("said");
    }

    fn read(store: &crate::secretary::conversations::InMemory) -> std::sync::Arc<Transcript> {
        transcript::read(store.conversations(), &id()).expect("reads")
    }

    fn person(text: &str) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }
    }

    fn answer(text: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        }
    }

    /// Each rule fires once on the reading that owes it, never again on it, and its outcome
    /// discharges it, so a restarted engine over the store fires nothing: the memo that lasts is
    /// the transcript.
    #[test]
    fn each_rule_is_discharged_by_its_own_outcome() {
        let lines = store();
        let mut engine = Engine::new(RULES.to_vec());
        assert!(engine.due(&id(), &read(&lines)).is_none(), "nothing said");

        say(&lines, person("when is the permit due"));
        let (rule, firing) = engine.due(&id(), &read(&lines)).expect("an answer owed");
        assert_eq!(rule, Conversational::Answer);
        assert_eq!(firing.on(), &read(&lines).name());
        assert!(
            engine.due(&id(), &read(&lines)).is_none(),
            "a second notice of one reading is the first"
        );

        say(&lines, answer("on Friday"));
        let (rule, _) = engine
            .due(&id(), &read(&lines))
            .expect("the answer leaves the topic unnamed");
        assert_eq!(rule, Conversational::Title);
        transcript::push(
            lines.conversations(),
            &id(),
            &Entry::Titled {
                at: crate::capability::Datetime::new(2026, 10, 5, 9, 2, 0, 0).expect("an instant"),
                title: crate::secretary::words::Words::kept(
                    Title::new("The permit").expect("a title"),
                ),
                by: Namer::Job {
                    model: "m".to_owned(),
                },
            },
        )
        .expect("titled");
        assert!(engine.due(&id(), &read(&lines)).is_none());
        assert!(
            Engine::new(RULES.to_vec())
                .due(&id(), &read(&lines))
                .is_none(),
            "a restart refires nothing"
        );
    }

    /// A line written while a round runs is a new reading, owed again.
    #[test]
    fn a_new_line_is_a_new_reading() {
        let lines = store();
        let mut engine = Engine::new(vec![Conversational::Answer]);
        say(&lines, person("first"));
        let first = engine.due(&id(), &read(&lines)).expect("owed").1;
        say(&lines, person("second"));
        let second = engine.due(&id(), &read(&lines)).expect("owed again").1;
        assert_ne!(first.on(), second.on());
    }
}
