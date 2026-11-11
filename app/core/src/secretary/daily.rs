//! The box's own daily rounds: one conversation a day for each, continued rather than reopened,
//! and opened only on a day that has work for it.

use crate::conversation_id::ConversationId;
use crate::conversation_id::Daily;
use crate::refusal::{Refusal, Status};
use crate::secretary::Secretary;
use crate::secretary::conversations;
use crate::secretary::store::{Kind, Stored};
use crate::secretary::words::Words;
use crate::zone::Zone;

/// Today's round of one kind: its conversation, and that conversation as it stands when it was
/// opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    /// Today's conversation of the round.
    pub id: ConversationId,
    daily: Daily,
    held: Option<Stored>,
}

/// What a daily round came to: its work, or no start while the model is under a limit, in which
/// case the round wrote nothing and asked the model nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run<T> {
    /// It ran, to this.
    Done(T),
    /// It did not start.
    Paused(crate::health::Limited),
}

/// Today's round of `daily`, unless the model is under a limit now: the one check every box
/// round makes before it writes its question, so a limit fills no conversation with failures.
///
/// # Errors
/// As [`daily_round`].
pub fn begin(caps: &Secretary<'_>, zone: Zone, daily: Daily) -> Result<Run<Round>, Refusal> {
    match caps.model.limited() {
        Some(limited) => Ok(Run::Paused(limited)),
        None => daily_round(caps, zone, daily).map(Run::Done),
    }
}

/// Today's round of `daily`, in `zone`'s day: its conversation read when it exists.
///
/// # Errors
/// A conversation that exists and does not read, which is never read as one never opened.
pub fn daily_round(caps: &Secretary<'_>, zone: Zone, daily: Daily) -> Result<Round, Refusal> {
    let today = zone.day(prodrome_core::fpl::instant_of(caps.clock.now()));
    let id = ConversationId::daily(daily, today);
    let held = match conversations::read(caps.conversations, &id) {
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
            None => conversations::start(
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
