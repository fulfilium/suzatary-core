//! The rounds the box asks for itself: which one a question of the box's opens, recorded on the
//! question's line when it is written ([`crate::secretary::stamp::Stamp::Asking`]).

use suzatary_base::conversation_id::Daily;

/// A round whose question the box writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnRound {
    /// The daily review of the triage agent's open cards.
    Review,
    /// The daily sweep of the open list.
    Sweep,
    /// A brief over the whole chain.
    Brief,
}

impl OwnRound {
    /// Every round, so a stored word reads back as one.
    pub const ALL: [OwnRound; 3] = [OwnRound::Review, OwnRound::Sweep, OwnRound::Brief];

    /// Its word in a stored stamp.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            OwnRound::Review => "review",
            OwnRound::Sweep => "sweep",
            OwnRound::Brief => "brief",
        }
    }

    /// What one of its rounds is called in the stream, in place of the question the box wrote.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            OwnRound::Review => "Review of triage's batches",
            OwnRound::Sweep => "Sweep of later mail",
            OwnRound::Brief => "The day's brief",
        }
    }
}

impl From<Daily> for OwnRound {
    fn from(daily: Daily) -> Self {
        match daily {
            Daily::Review => OwnRound::Review,
            Daily::Sweep => OwnRound::Sweep,
        }
    }
}
