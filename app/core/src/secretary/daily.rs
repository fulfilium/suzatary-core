//! The box's own daily rounds: one conversation a day for each, continued rather than reopened,
//! and opened only on a day that has work for it.

use crate::capability::Secretary;
use crate::refusal::{Refusal, Status};
use crate::secretary::store::{ConversationId, Kind, Stored};
use crate::secretary::words::Words;
use crate::zone::Zone;

/// A round the box opens for itself, once a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Daily {
    /// The review of the triage agent's open cards.
    Review,
    /// The sweep of the open list.
    Sweep,
}

impl Daily {
    /// What its conversation's id ends with, after the day.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Daily::Review => "review",
            Daily::Sweep => "sweep",
        }
    }

    /// What its conversation is called in a list.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Daily::Review => "Review",
            Daily::Sweep => "Sweep",
        }
    }

    /// What one of its rounds is called in the stream, in place of the prompt the box wrote.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Daily::Review => "Review of triage's batches",
            Daily::Sweep => "Sweep of later mail",
        }
    }
}

/// Today's round of one kind: its conversation, and that conversation as it stands when it was
/// opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Today's conversation of the round.
    pub id: ConversationId,
    daily: Daily,
    held: Option<Stored>,
}

/// Today's round of `daily`, in `zone`'s day: its conversation read when it exists.
///
/// # Errors
/// A conversation that exists and does not read, which is never read as one never opened.
pub fn daily_round(caps: &Secretary<'_>, zone: Zone, daily: Daily) -> Result<Round, Refusal> {
    let today = zone.day(prodrome_core::fpl::instant_of(caps.clock.now()));
    let id = ConversationId::daily(daily, today);
    let held = match super::read(caps.conversations, &id) {
        Ok(stored) => Some(stored),
        Err(refusal) if refusal.status() == Status::NotFound => None,
        Err(refusal) => return Err(refusal),
    };
    Ok(Round { id, daily, held })
}

impl Round {
    /// The conversation as it stands, or as one never opened reads, writing nothing: what a day
    /// with nothing to do answers.
    #[must_use]
    pub fn idle(self, caps: &Secretary<'_>) -> Stored {
        let Round { id, daily, held } = self;
        held.unwrap_or_else(|| Stored {
            title: Words::kept(daily.title().to_owned()),
            model: caps.model.name(),
            ..Stored::empty(&id, Kind::Review, caps.clock.now())
        })
    }

    /// The conversation, opened first when it was not: what a day with work goes on in.
    ///
    /// # Errors
    /// The store's own.
    pub fn open(self, caps: &Secretary<'_>) -> Result<Stored, Refusal> {
        match self.held {
            Some(stored) => Ok(stored),
            None => super::start(
                caps.conversations,
                caps.model,
                &self.id,
                self.daily.title(),
                Kind::Review,
                caps.clock.now(),
            ),
        }
    }
}
