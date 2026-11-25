//! A whole conversation as a reader gets one: the fold of its lines.

use super::{
    Aside, Asking, Entry, Kept, Kind, Line, Lines, Review, ReviewFailure, Rewind, Said, Scope,
    Standing, Unheld, Unplaced,
};
use suzatary_base::conversation_id::ConversationId;
use suzatary_inbox::secretary::proposal::Proposal;

use crate::secretary::model::Turn;
use crate::secretary::turn_shape::TurnShape;
use suzatary_base::capability::Datetime;
use suzatary_memory::secretary::words::Words;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: ConversationId,
    pub opened: Datetime,
    /// The last [`Entry::Titled`](super::Entry::Titled), else the opening line's; empty for an
    /// untitled topic.
    pub title: Words<String>,
    pub kind: Kind,
    /// The model behind it: the one its opening named, else the first that opened a session in
    /// it; empty until one does.
    pub model: String,
    /// Every turn, oldest first.
    pub turns: Vec<Said>,
    /// Every proposal, in the order made; the index is the tap's.
    pub proposals: Vec<Asking>,
    pub reviews: Vec<Review>,
    pub failures: Vec<ReviewFailure>,
    /// The session a [`SessionModel`](crate::secretary::model::SessionModel) caller should
    /// `resume`, or `None` when none was opened or a stop, a rewind or an
    /// [`Entry::Unheard`](super::Entry::Unheard) line since ended it.
    pub session: Option<Kept>,
    /// The last [`Entry::Scoped`](super::Entry::Scoped) line, a window when there is none.
    pub scope: Scope,
    /// The memory's version this conversation was last told; `None` for one never told.
    pub memory_seen: Option<u64>,
    /// Every span of turns a rewind set aside; `turns` keeps them, so every index stays the one
    /// the page and a render URL name.
    pub aside: Vec<Aside>,
    /// The index of every act an [`Entry::Stopped`](super::Entry::Stopped) folded into, known by
    /// its line and never its words, so [`Stored::rewind`] cannot take another act for one.
    pub stops: Vec<usize>,
    /// Each coding session a tap started, by the proposal's index: the last
    /// [`Entry::Coding`](super::Entry::Coding) line for it.
    pub coding: std::collections::BTreeMap<usize, suzatary_inbox::coding::Session>,
}

/// The name a list draws for a conversation titled `title`: the title, "New topic" while the title
/// job is owed, or the box's sentence for a name whose words are gone.
#[must_use]
pub fn heading(title: &Words<String>) -> std::borrow::Cow<'_, str> {
    match title {
        Words::Kept { kept } if kept.trim().is_empty() => "New topic".into(),
        Words::Kept { kept } => kept.as_str().into(),
        gone @ Words::Gone { .. } => gone.to_string().into(),
    }
}

/// The act an [`Entry::Stopped`](super::Entry::Stopped) line reads as, in the box's words.
#[must_use]
pub fn stopped_by(by: &str) -> String {
    format!(
        "{} stopped the answer",
        suzatary_base::spelling::capitalised(by)
    )
}

/// A conversation's file said something this program does not understand.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAConversation {
    #[error("{0} has no opening line — a conversation starts with one")]
    Unopened(ConversationId),
    #[error("{id} records a rewind to turn {to}, which was never said")]
    Unsaid {
        /// The conversation.
        id: ConversationId,
        to: usize,
    },
    #[error("{id} records a session for proposal {index}, which was never proposed")]
    Phantom {
        /// The conversation.
        id: ConversationId,
        index: usize,
    },
}

impl Stored {
    /// Fold the lines into the conversation: the one function between the file and the value.
    pub fn of(id: &ConversationId, entries: &[Entry]) -> Result<Self, NotAConversation> {
        Self::of_lines(id, entries)
    }

    pub fn of_lines<'e>(
        id: &ConversationId,
        entries: impl IntoIterator<Item = &'e Entry>,
    ) -> Result<Self, NotAConversation> {
        Self::of_read(id, entries.into_iter().map(|entry| (entry, Vec::new())))
    }

    /// [`Stored::of`] over lines in order, each beside the turns it sets aside when it is an
    /// [`Entry::Again`](super::Entry::Again), which only a reader holding the lines' descent can
    /// say; empty for every other line.
    pub fn of_read<'e>(
        id: &ConversationId,
        lines: impl IntoIterator<Item = (&'e Entry, Vec<usize>)>,
    ) -> Result<Self, NotAConversation> {
        let mut lines = lines.into_iter();
        let mut stored = Self::opened(id, lines.next().map(|(entry, _)| entry))?;
        for (entry, aside) in lines {
            stored.fold_read(entry, aside)?;
        }
        Ok(stored)
    }

    /// Fold one more line in; an [`Entry::Again`](super::Entry::Again) folded here sets nothing
    /// aside.
    pub fn fold(&mut self, entry: &Entry) -> Result<(), NotAConversation> {
        self.fold_read(entry, Vec::new())
    }

    fn fold_read(&mut self, entry: &Entry, aside: Vec<usize>) -> Result<(), NotAConversation> {
        match entry {
            // A second opening line is ignored, not refused, so one stray line does not cost a
            // whole transcript; an erasure's slots were read as gone where they stood, a reseal's
            // words where its slot stands, and a claim is never a turn.
            Entry::Opened { .. }
            | Entry::SetUp { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. }
            | Entry::Began { .. } => {}
            Entry::Titled { title, .. } => self.title = super::entry::named(title),
            Entry::Said { .. } => self.turns.extend(Said::of(entry)),
            Entry::Proposed {
                at,
                object,
                revises,
            } => proposed(self, *at, object, *revises),
            Entry::Reviewed {
                at,
                chain,
                batch,
                decision,
                why,
            } => self.reviews.push(Review {
                at: *at,
                chain: chain.clone(),
                batch: batch.clone(),
                decision: decision.clone(),
                why: why.clone(),
            }),
            Entry::ReviewFailed {
                at,
                reason,
                standing,
            } => self.failures.push(ReviewFailure {
                at: *at,
                reason: reason.clone(),
                standing: *standing,
            }),
            Entry::MemorySeen { version, .. } => self.memory_seen = Some(*version),
            Entry::Coding {
                proposal, session, ..
            } => {
                answered(self, &self.id.clone(), *proposal)?;
                self.coding.insert(*proposal, session.clone());
            }
            Entry::Stopped { .. } => {
                self.stops.push(self.turns.len());
                self.turns.extend(Said::of(entry));
                self.session = None;
            }
            Entry::Rewound { to, at, by } => rewound(self, *to, *at, by)?,
            Entry::Again { at, by, .. } => {
                self.aside.push(Aside {
                    turns: aside.into_iter().collect(),
                    at: *at,
                    by: by.clone(),
                });
                self.session = None;
            }
            // The last session line wins: one writer, so overwriting is the register's reading.
            Entry::Session {
                at,
                session,
                model,
                cwd,
                surface,
            } => {
                // What a session line was sent is not known, so it is read as holding every turn.
                self.session = Some(Kept::of(session, model, cwd, *at, surface.clone(), 0));
                self.named(model);
            }
            Entry::Kept {
                at,
                session,
                model,
                cwd,
                surface,
                from,
            } => {
                self.session = Some(Kept::of(session, model, cwd, *at, surface.clone(), *from));
                self.named(model);
            }
            Entry::Scoped { scope, .. } => self.scope = *scope,
            // Only the session that did not hear ends: a later one heard the turn.
            Entry::Unheard { session, .. } => {
                if self
                    .session
                    .as_ref()
                    .is_some_and(|kept| kept.session.id == *session)
                {
                    self.session = None;
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn empty(id: &ConversationId, kind: Kind, opened: Datetime) -> Self {
        Self {
            id: id.clone(),
            opened,
            title: Words::kept(String::new()),
            kind,
            model: String::new(),
            turns: Vec::new(),
            proposals: Vec::new(),
            reviews: Vec::new(),
            failures: Vec::new(),
            session: None,
            scope: Scope::Window,
            memory_seen: None,
            aside: Vec::new(),
            stops: Vec::new(),
            coding: std::collections::BTreeMap::new(),
        }
    }

    /// The conversation as its opening line alone says it is.
    fn opened(id: &ConversationId, first: Option<&Entry>) -> Result<Self, NotAConversation> {
        let Some(start) = first.and_then(Entry::start) else {
            return Err(NotAConversation::Unopened(id.clone()));
        };
        let model = match start.begun {
            super::Begun::Opened { model } => model.to_owned(),
            super::Begun::SetUp(_) => String::new(),
        };
        Ok(Self {
            title: start.title.clone(),
            model,
            ..Self::empty(id, start.kind, start.at)
        })
    }

    /// `model` as the model behind it, where nothing named one before.
    fn named(&mut self, model: &str) {
        if self.model.is_empty() {
            model.clone_into(&mut self.model);
        }
    }

    /// Whether turn `index` is on the live path: no rewind set it aside.
    #[must_use]
    pub fn is_live(&self, index: usize) -> bool {
        !self.aside.iter().any(|aside| aside.turns.contains(&index))
    }

    /// Whether turn `index` closes the question before it: a model turn with no calls answers it,
    /// and a stop ends it unanswered, by its own line and in one write.
    #[must_use]
    pub fn closes(&self, index: usize) -> bool {
        match self
            .turns
            .get(index)
            .map(|said| crate::secretary::turn_shape::shape(&said.turn))
        {
            Some(TurnShape::Model { calls }) => calls.is_empty(),
            Some(TurnShape::Act { .. }) => self.stops.contains(&index),
            Some(TurnShape::Person { .. } | TurnShape::Tool { .. }) | None => false,
        }
    }

    /// Whether position `at` of `live` (this conversation's [`Stored::live`]) opens a round: the
    /// first turn, or the turn after one that [closes](Stored::closes), unless it is the empty
    /// answer a round wrote after its stop.
    #[must_use]
    pub fn opens(&self, live: &[(usize, &Said)], at: usize) -> bool {
        at == 0 || (self.closes(live[at - 1].0) && !self.ends_its_stop(live, at))
    }

    /// Whether position `at` of `live` is the empty answer a round wrote after its stop, which
    /// belongs to its stop: it closes nothing more and opens no round of its own.
    fn ends_its_stop(&self, live: &[(usize, &Said)], at: usize) -> bool {
        at.checked_sub(1)
            .is_some_and(|before| self.stops.contains(&live[before].0))
            && matches!(&live[at].1.turn, Turn::Model { text, calls } if text.is_empty() && calls.is_empty())
    }

    /// The live path: every turn no rewind set aside, with its index.
    #[must_use]
    pub fn live(&self) -> Vec<(usize, &Said)> {
        self.turns
            .iter()
            .enumerate()
            .filter(|(index, _)| self.is_live(*index))
            .collect()
    }

    /// Where a rewind of the last round starts, or why there is none.
    ///
    /// The last round must be finished and opened by a question.
    ///
    /// A rewind never hides a write: only model turns, verbs whose [`Effect`] is [`Effect::Reads`]
    /// or which were refused, a stop, and for an edit the question may go aside. Anything else in
    /// the round refuses the rewind.
    ///
    /// [`Effect`]: crate::secretary::verb_name::Effect
    /// [`Effect::Reads`]: crate::secretary::verb_name::Effect::Reads
    pub fn rewind(&self, how: Rewind) -> Result<usize, suzatary_base::refusal::Refusal> {
        let conflict = suzatary_base::refusal::Refusal::conflict;
        let live = self.live();
        let Some(&(last, _)) = live.last() else {
            return Err(conflict("nothing has been asked here yet"));
        };
        if !self.closes(last) {
            return Err(conflict(
                "the answer is still coming — stop it first, then ask again",
            ));
        }
        let end = live.len() - 1;
        let closing = if self.ends_its_stop(&live, end) {
            end - 1
        } else {
            end
        };
        let start = (0..=closing)
            .rev()
            .find(|&at| self.opens(&live, at))
            .unwrap_or(0);
        let round = &live[start..];
        let (opening, opened) = round[0];
        if !matches!(
            crate::secretary::turn_shape::shape(&opened.turn),
            TurnShape::Person { .. }
        ) {
            return Err(conflict(
                "only an answer to a question can be asked again, and this one followed none",
            ));
        }
        // A question opens the round and an answer ends it, so the round holds a second turn: the
        // first a regenerate sets aside.
        let to = match how {
            Rewind::Edit => opening,
            Rewind::Regenerate => round[1].0,
        };
        let wrote =
            |index: usize, said: &Said| match crate::secretary::turn_shape::shape(&said.turn) {
                TurnShape::Model { .. } => false,
                TurnShape::Tool { verb, outcome, .. } => {
                    !outcome.refused()
                        && verb.verb().is_some_and(|verb| {
                            verb.powers().effect != crate::secretary::verb_name::Effect::Reads
                        })
                }
                TurnShape::Act { .. } => !self.stops.contains(&index),
                TurnShape::Person { .. } => true,
            };
        let hidden = round
            .iter()
            .filter(|(index, _)| *index > opening)
            .any(|(index, said)| wrote(*index, said));
        let proposed = self
            .proposals
            .iter()
            .any(|standing| standing.after_turn > to);
        if hidden || proposed {
            return Err(conflict(
                "this answer wrote something (a todo, a memory, a proposal) or holds a second \
                 message, and a rewind never hides one — ask again instead",
            ));
        }
        Ok(to)
    }

    #[must_use]
    pub fn heading(&self) -> std::borrow::Cow<'_, str> {
        heading(&self.title)
    }

    /// Whether the conversation is untitled: a title whose words are gone was a name.
    #[must_use]
    pub fn untitled(&self) -> bool {
        self.title
            .get()
            .is_some_and(|title| title.trim().is_empty())
    }

    /// Proposal `index`, joined with its inbox object.
    ///
    /// # Errors
    /// [`Refusal::not_found`](suzatary_base::refusal::Refusal::not_found) for an index
    /// nothing proposed, and [`Refusal::conflict`](suzatary_base::refusal::Refusal::conflict)
    /// naming why for a line naming an object the inbox does not read.
    pub fn standing(
        &self,
        index: usize,
        inbox: &suzatary_inbox::secretary::inbox::Fold,
    ) -> Result<Standing, suzatary_base::refusal::Refusal> {
        let asking = self.proposals.get(index).ok_or_else(|| {
            suzatary_base::refusal::Refusal::not_found(format!(
                "conversation {} has no proposal {index} (it has {})",
                self.id,
                self.proposals.len()
            ))
        })?;
        joined(&self.proposals, asking, inbox).map_err(|unplaced| {
            suzatary_base::refusal::Refusal::conflict(format!(
                "conversation {} names proposal {}, which cannot be read: {}",
                self.id, unplaced.asking.object, unplaced.why
            ))
        })
    }

    /// Every proposal line against the inbox's reading, in order: each joined with its object, or
    /// held with why the reading cannot place it, never dropped.
    #[must_use]
    pub fn lines(&self, inbox: &suzatary_inbox::secretary::inbox::Fold) -> Lines {
        Lines::of(&self.proposals, inbox)
    }
}

impl Lines {
    /// A conversation's `proposals`, every one it made in order, against the inbox's reading.
    #[must_use]
    pub fn of(proposals: &[Asking], inbox: &suzatary_inbox::secretary::inbox::Fold) -> Self {
        Lines(
            proposals
                .iter()
                .map(|asking| {
                    joined(proposals, asking, inbox)
                        .map_or_else(Line::Unplaced, |standing| Line::Placed(Box::new(standing)))
                })
                .collect(),
        )
    }
}

fn joined(
    proposals: &[Asking],
    asking: &Asking,
    inbox: &suzatary_inbox::secretary::inbox::Fold,
) -> Result<Standing, Unplaced> {
    let object = inbox.get(&asking.object).ok_or_else(|| Unplaced {
        asking: asking.clone(),
        why: if inbox.unasked().contains(&asking.object) {
            Unheld::Unasked
        } else {
            Unheld::NotHeld
        },
    })?;
    let rows = |object: &suzatary_inbox::secretary::inbox::Object| match &object.asked.proposal {
        Proposal::Changes(changeset) => changeset.acts().len(),
        Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Act(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => 0,
    };
    let revises = asking.revises.map(|card| super::Revised {
        card,
        carried: proposals
            .get(card)
            .and_then(|old| inbox.get(&old.object))
            .map_or(0, |old| rows(old).min(rows(object))),
    });
    Ok(Standing {
        asking: asking.clone(),
        object: object.clone(),
        revises,
    })
}

fn proposed(
    stored: &mut Stored,
    at: Datetime,
    object: &suzatary_inbox::secretary::inbox::ProposalId,
    revises: Option<usize>,
) {
    stored.proposals.push(Asking {
        index: stored.proposals.len(),
        at,
        object: object.clone(),
        after_turn: stored.turns.len(),
        revises,
    });
}

/// The turns from `to` go aside, and the session with them.
fn rewound(stored: &mut Stored, to: usize, at: Datetime, by: &str) -> Result<(), NotAConversation> {
    if to > stored.turns.len() {
        return Err(NotAConversation::Unsaid {
            id: stored.id.clone(),
            to,
        });
    }
    stored.aside.push(Aside {
        turns: (to..stored.turns.len()).collect(),
        at,
        by: by.to_owned(),
    });
    stored.session = None;
    Ok(())
}

fn answered<'a>(
    stored: &'a mut Stored,
    id: &ConversationId,
    index: usize,
) -> Result<&'a mut Asking, NotAConversation> {
    stored
        .proposals
        .get_mut(index)
        .ok_or(NotAConversation::Phantom {
            id: id.clone(),
            index,
        })
}
