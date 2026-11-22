//! The conversation's rules: what the box owes a conversation's reading, run by one engine
//! over the conversations store.
//!
//! Each rule's outcome is a line of the transcript it reads, so writing it moves the reading
//! past the one it fired on and discharges the rule there. Order is declaration order, so an
//! answer comes before the title that names it.

use suzatary_base::conversation_id::ConversationId;
use suzatary_lines::secretary::transcript::Transcript;
use suzatary_triage::rule::Rule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversational {
    /// A conversation set up from a template is owed the box's opening note
    /// ([`crate::secretary::set_up`]), which is the outcome.
    Opening,
    /// A line that wakes a model is owed an answer: run a round, whose closing turn (an answer
    /// with no calls, or "did not finish") is the outcome.
    Answer,
    /// An unnamed topic has its first answer: run the title job, whose `Titled` line is the
    /// outcome.
    Title,
}

/// Every conversation rule, in the order a reading is tried against them.
pub const RULES: [Conversational; 3] = [
    Conversational::Opening,
    Conversational::Answer,
    Conversational::Title,
];

impl Rule for Conversational {
    type Key = ConversationId;
    type Reading = Transcript;
    type Witness = ();

    fn name(&self) -> &'static str {
        match self {
            Conversational::Opening => "opening",
            Conversational::Answer => "answer",
            Conversational::Title => "title",
        }
    }

    fn holds(&self, reading: &Transcript) -> bool {
        match self {
            Conversational::Opening => crate::secretary::set_up::owed(reading).is_some(),
            Conversational::Answer => reading
                .owed()
                .iter()
                .any(|(_, said)| suzatary_conversations::secretary::queue::wakes(said)),
            Conversational::Title => crate::secretary::title::owed(reading.stored()),
        }
    }
}
