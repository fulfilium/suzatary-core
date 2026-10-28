//! A whole conversation as a reader gets one: the fold of its lines.

use super::{
    Aside, Asking, ConversationId, Entry, Kept, Kind, Line, Lines, Proposal, Review, ReviewFailure,
    Rewind, Said, Scope, Standing, Unheld, Unplaced,
};

use crate::capability::{Datetime, Turn};
use crate::secretary::turn_shape::TurnShape;
use crate::secretary::words::Words;

/// A whole conversation as a reader gets one: the fold of its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// Its name.
    pub id: ConversationId,
    /// When it was opened.
    pub opened: Datetime,
    /// What it is about: the last [`Entry::Titled`](super::Entry::Titled), else the opening line's; empty for an untitled
    /// topic.
    pub title: Words<String>,
    /// What it is for.
    pub kind: Kind,
    /// Which model is behind it.
    pub model: String,
    /// Every turn, oldest first, each with the instant it was said.
    pub turns: Vec<Said>,
    /// Every proposal, in the order made; the index is the tap's.
    pub proposals: Vec<Asking>,
    /// Every card this conversation withdrew or held, in the order it did.
    pub reviews: Vec<Review>,
    /// Every time a review was asked for here and could not run, in order; usually empty.
    pub failures: Vec<ReviewFailure>,
    /// The session a [`SessionModel`](crate::capability::SessionModel) caller should `resume`: the
    /// last [`Entry::Kept`](super::Entry::Kept) or [`Entry::Session`](super::Entry::Session) line, or `None` when none was ever opened or a
    /// stop, a rewind or an [`Entry::Unheard`](super::Entry::Unheard) line since ended it.
    pub session: Option<Kept>,
    /// How its model is asked: the last [`Entry::Scoped`](super::Entry::Scoped) line, a window when there is none.
    pub scope: Scope,
    /// The memory's version this conversation was last told; `None` for one never told.
    pub memory_seen: Option<u64>,
    /// Every span of turns a rewind set aside, in the order it did.
    /// `turns` keeps them, so every index stays the one the page and a render
    /// URL name; [`Stored::live`] is the path a model reads.
    pub aside: Vec<Aside>,
    /// The index of every act an [`Entry::Stopped`](super::Entry::Stopped) folded into, known by its line and never its
    /// words, so [`Stored::rewind`] cannot take another act for one.
    pub stops: Vec<usize>,
    /// Each coding session a tap started, by the proposal's index: the last
    /// [`Entry::Coding`](super::Entry::Coding) line for it.
    pub coding: std::collections::BTreeMap<usize, crate::coding::Session>,
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

/// The act an [`Entry::Stopped`](super::Entry::Stopped) line reads as, in the box's words, so the model reading it back
/// is told who did what.
#[must_use]
pub fn stopped_by(by: &str) -> String {
    format!("{} stopped the answer", crate::secretary::acts::named(by))
}

/// A conversation's file said something this program does not understand.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAConversation {
    /// No `Opened` line: every file this program writes starts with one.
    #[error("{0} has no opening line — a conversation starts with one")]
    Unopened(ConversationId),
    /// A `Rewound` naming a turn past the end of the file.
    #[error("{id} records a rewind to turn {to}, which was never said")]
    Unsaid {
        /// The conversation.
        id: ConversationId,
        /// The turn it named.
        to: usize,
    },
    /// A `Coding` line naming a proposal that was never made.
    #[error("{id} records a session for proposal {index}, which was never proposed")]
    Phantom {
        /// The conversation.
        id: ConversationId,
        /// The index it named.
        index: usize,
    },
}

impl Stored {
    /// Fold the lines into the conversation: the one function between the file and the value, so a
    /// transcript's reader and its route cannot disagree.
    ///
    /// # Errors
    /// [`NotAConversation`] for a file with no opening line, or one recording
    /// a session for a proposal nothing proposed.
    pub fn of(id: &ConversationId, entries: &[Entry]) -> Result<Self, NotAConversation> {
        Self::of_lines(id, entries)
    }

    /// [`Stored::of`] over lines in order.
    ///
    /// # Errors
    /// As [`Stored::of`].
    pub fn of_lines<'e>(
        id: &ConversationId,
        entries: impl IntoIterator<Item = &'e Entry>,
    ) -> Result<Self, NotAConversation> {
        let mut entries = entries.into_iter();
        let mut stored = Self::opened(id, entries.next())?;
        for entry in entries {
            stored.fold(entry)?;
        }
        Ok(stored)
    }

    /// Fold one more line in: what [`Stored::of`] does to every line after the opening.
    ///
    /// # Errors
    /// [`NotAConversation`] for a coding line no proposal asked for, or a rewind past the end.
    pub fn fold(&mut self, entry: &Entry) -> Result<(), NotAConversation> {
        match entry {
            // A second opening line is ignored, not refused: the first is the fact, and one
            // stray line must not cost a whole transcript. An erasure's slots were read as gone
            // where they stood, and a reseal's words where the slot it seals stands.
            Entry::Opened { .. } | Entry::Erased { .. } | Entry::Resealed { .. } => {}
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
            // The last session line wins: one writer, so overwriting is the register's reading.
            Entry::Session {
                at,
                session,
                model,
                cwd,
                surface,
            } => {
                // A line from before the opening window was recorded: what it was sent is not
                // known, so it is read as holding every turn.
                self.session = Some(Kept::of(session, model, cwd, *at, surface.clone(), 0));
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

    /// A conversation of `kind` opened at `opened` with no line yet: what a fold begins from
    /// before its opening line.
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
        let Some(Entry::Opened {
            at,
            title,
            model,
            kind,
        }) = first
        else {
            return Err(NotAConversation::Unopened(id.clone()));
        };
        Ok(Self {
            title: title.clone(),
            model: model.clone(),
            ..Self::empty(id, *kind, *at)
        })
    }

    /// Whether turn `index` is on the live path: no rewind set it aside.
    #[must_use]
    pub fn is_live(&self, index: usize) -> bool {
        !self
            .aside
            .iter()
            .any(|aside| (aside.from..aside.until).contains(&index))
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
    /// answer a round once wrote after its stop.
    #[must_use]
    pub fn opens(&self, live: &[(usize, &Said)], at: usize) -> bool {
        at == 0 || (self.closes(live[at - 1].0) && !self.ends_its_stop(live, at))
    }

    /// Whether position `at` of `live` is the empty answer a round wrote after its stop before a
    /// stop closed its question by itself. Stored conversations hold one, and it belongs to its
    /// stop: it closes nothing more and opens no round of its own.
    fn ends_its_stop(&self, live: &[(usize, &Said)], at: usize) -> bool {
        at.checked_sub(1)
            .is_some_and(|before| self.stops.contains(&live[before].0))
            && matches!(&live[at].1.turn, Turn::Model { text, calls } if text.is_empty() && calls.is_empty())
    }

    /// The live path: every turn no rewind set aside, with its index; what the queue delivers, a
    /// model is sent and the state reads.
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
    /// The last round must be finished (a running round is the worker's; stop it first) and opened
    /// by a question: an edit replaces it, a regenerate leaves it pending.
    ///
    /// A rewind never hides a write: only model turns, verbs whose [`Effect`] is [`Effect::Reads`]
    /// or which were refused, a stop, and for an edit the question may go aside. Anything else in
    /// the round refuses the rewind.
    ///
    /// # Errors
    /// [`Refusal::conflict`](crate::refusal::Refusal::conflict) naming which rule it broke.
    ///
    /// [`Effect`]: crate::secretary::Effect
    /// [`Effect::Reads`]: crate::secretary::Effect::Reads
    pub fn rewind(&self, how: Rewind) -> Result<usize, crate::refusal::Refusal> {
        let conflict = crate::refusal::Refusal::conflict;
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
                            verb.class().effect != crate::secretary::Effect::Reads
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

    /// The name a list draws: [`Stored::title`], "New topic" while the title job is owed, or the
    /// box's sentence for a name whose words are gone.
    #[must_use]
    pub fn heading(&self) -> std::borrow::Cow<'_, str> {
        heading(&self.title)
    }

    /// Whether the conversation is named: a title whose words are gone was a name.
    #[must_use]
    pub fn untitled(&self) -> bool {
        self.title
            .get()
            .is_some_and(|title| title.trim().is_empty())
    }

    /// Proposal `index`, joined with its inbox object.
    ///
    /// # Errors
    /// [`Refusal::not_found`](crate::refusal::Refusal::not_found) for an index
    /// nothing proposed, and [`Refusal::conflict`](crate::refusal::Refusal::conflict)
    /// naming why for a line naming an object the inbox does not read.
    pub fn standing(
        &self,
        index: usize,
        inbox: &crate::secretary::inbox::Fold,
    ) -> Result<Standing, crate::refusal::Refusal> {
        let asking = self.proposals.get(index).ok_or_else(|| {
            crate::refusal::Refusal::not_found(format!(
                "conversation {} has no proposal {index} (it has {})",
                self.id,
                self.proposals.len()
            ))
        })?;
        joined(self, asking, inbox).map_err(|unplaced| {
            crate::refusal::Refusal::conflict(format!(
                "conversation {} names proposal {}, which cannot be read: {}",
                self.id, unplaced.asking.object, unplaced.why
            ))
        })
    }

    /// Every proposal line against the inbox's reading, in order: each joined with its object, or
    /// held with why the reading cannot place it, never dropped.
    #[must_use]
    pub fn lines(&self, inbox: &crate::secretary::inbox::Fold) -> Lines {
        Lines(
            self.proposals
                .iter()
                .map(|asking| {
                    joined(self, asking, inbox)
                        .map_or_else(Line::Unplaced, |standing| Line::Placed(Box::new(standing)))
                })
                .collect(),
        )
    }

    /// The index of the proposal naming `object`, if this conversation made one.
    #[must_use]
    pub fn index_of(&self, object: &crate::secretary::inbox::ProposalId) -> Option<usize> {
        self.proposals
            .iter()
            .position(|asking| &asking.object == object)
    }
}

fn joined(
    stored: &Stored,
    asking: &Asking,
    inbox: &crate::secretary::inbox::Fold,
) -> Result<Standing, Unplaced> {
    let object = inbox.get(&asking.object).ok_or_else(|| Unplaced {
        asking: asking.clone(),
        why: if inbox.unasked().contains(&asking.object) {
            Unheld::Unasked
        } else {
            Unheld::NotHeld
        },
    })?;
    let rows = |object: &crate::secretary::inbox::Object| match &object.asked.proposal {
        Proposal::Changes(changeset) => changeset.acts().len(),
        Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Act(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => 0,
    };
    let revises = asking.revises.map(|card| super::Revised {
        card,
        carried: stored
            .proposals
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

/// Fold one [`Entry::Proposed`](super::Entry::Proposed): push the new [`Asking`](super::Asking).
fn proposed(
    stored: &mut Stored,
    at: Datetime,
    object: &crate::secretary::inbox::ProposalId,
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

/// Fold one [`Entry::Rewound`](super::Entry::Rewound): the turns from `to` go aside, and the session with them.
fn rewound(stored: &mut Stored, to: usize, at: Datetime, by: &str) -> Result<(), NotAConversation> {
    if to > stored.turns.len() {
        return Err(NotAConversation::Unsaid {
            id: stored.id.clone(),
            to,
        });
    }
    stored.aside.push(Aside {
        from: to,
        until: stored.turns.len(),
        at,
        by: by.to_owned(),
    });
    stored.session = None;
    Ok(())
}

/// The [`Asking`](super::Asking) a `Coding` line names, or the refusal that says it names
/// nothing.
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
