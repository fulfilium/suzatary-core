//! A whole conversation as a reader gets one: the fold of its lines.

use super::{
    Aside, Asking, Entry, Kind, Line, Lines, Review, ReviewFailure, Rewind, Said, Scope, Standing,
    Unheld, Unplaced,
};
use suzatary_base::conversation_id::ConversationId;
use suzatary_inbox::secretary::proposal::Proposal;

use crate::secretary::model::Turn;
use crate::secretary::queue;
use crate::secretary::taint::{self, Taint};
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
    /// The model its opening named; empty for a conversation a template set up.
    pub model: String,
    /// Every turn, oldest first.
    pub turns: Vec<Said>,
    /// What each turn carries into a call, by the same index as `turns`.
    pub carried: Vec<Carried>,
    /// The window's fold over the live rounds ([`queue::window`]).
    pub steps: queue::Steps,
    /// Every proposal, in the order made; the index is the tap's.
    pub proposals: Vec<Asking>,
    pub reviews: Vec<Review>,
    pub failures: Vec<ReviewFailure>,
    /// The last [`Entry::Scoped`](super::Entry::Scoped) line, a window when there is none.
    pub scope: Scope,
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

/// One line as a conversation's fold takes it: its entry, the turns an `Again` sets aside, what
/// its turn weighs (its stored line's sealed bytes), and what its line stores that it carries.
#[derive(Debug, Clone)]
pub struct Folded<'e> {
    pub entry: &'e Entry,
    pub aside: Vec<usize>,
    pub weight: usize,
    /// What the box's own line stores that its turn carries (a model's turn, what its call was
    /// sent; a verb's answer, its class), as the line's one reader of a carry gives it, with the
    /// writer's check; `None` for any other line, another writer's, and an older box's.
    pub carries: Option<Taint>,
}

impl<'e> Folded<'e> {
    /// A line with no stored print: it sets nothing aside, weighs its words, and stores no carry,
    /// since only a line, with its writer, says one.
    #[must_use]
    pub fn bare(entry: &'e Entry) -> Self {
        let weight = Said::of(entry).map_or(0, |said| queue::bytes(&said.turn));
        Self {
            entry,
            aside: Vec::new(),
            weight,
            carries: None,
        }
    }
}

/// What one turn carries into a call, read when it is folded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Carried {
    /// A model's turn carries what its line stores its call was sent, and one whose line stores
    /// nothing every turn before it; a verb's answer the class its line stores, and an older
    /// one its reader's class ([`taint::turn`]); a person's or an act's turn its own class.
    pub taint: Taint,
    /// Its stored line's sealed bytes.
    pub bytes: usize,
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
        Self::of_read(id, entries.into_iter().map(Folded::bare))
    }

    /// [`Stored::of`] over lines as the transcript's fold takes them ([`Folded`]).
    pub fn of_read<'e>(
        id: &ConversationId,
        lines: impl IntoIterator<Item = Folded<'e>>,
    ) -> Result<Self, NotAConversation> {
        let mut lines = lines.into_iter();
        let mut stored = Self::opened(id, lines.next().map(|folded| folded.entry))?;
        for folded in lines {
            stored.fold_read(folded)?;
        }
        Ok(stored)
    }

    /// Fold one more line in, as a line with no stored print.
    pub fn fold(&mut self, entry: &Entry) -> Result<(), NotAConversation> {
        self.fold_read(Folded::bare(entry))
    }

    /// Fold one more line in, as [`Stored::of_read`] does.
    pub fn fold_read(&mut self, folded: Folded<'_>) -> Result<(), NotAConversation> {
        let Folded {
            entry,
            aside,
            weight,
            carries,
        } = folded;
        match entry {
            // A second opening line is ignored, not refused, so one stray line does not cost a
            // whole transcript; an erasure's slots were read as gone where they stood, a reseal's
            // words where its slot stands, a claim is never a turn, and a retired line is read as nothing.
            Entry::Opened { .. }
            | Entry::SetUp { .. }
            | Entry::Erased { .. }
            | Entry::Resealed { .. }
            | Entry::Retired { .. }
            | Entry::Began { .. } => {}
            Entry::Titled { title, .. } => self.title = super::entry::named(title),
            Entry::Said { .. } => self.said(Said::of(entry), weight, carries),
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
            Entry::Coding {
                proposal, session, ..
            } => {
                answered(self, &self.id.clone(), *proposal)?;
                self.coding.insert(*proposal, session.clone());
            }
            Entry::Stopped { .. } => {
                self.stops.push(self.turns.len());
                self.said(Said::of(entry), weight, None);
            }
            Entry::Rewound { to, at, by } => {
                rewound(self, *to, *at, by)?;
                self.restep();
            }
            Entry::Again { at, by, .. } => {
                self.aside.push(Aside {
                    turns: aside.into_iter().collect(),
                    at: *at,
                    by: by.clone(),
                });
                self.restep();
            }
            Entry::Scoped { scope, .. } => self.scope = *scope,
        }
        Ok(())
    }

    fn said(&mut self, said: Option<Said>, weight: usize, stored: Option<Taint>) {
        let Some(said) = said else { return };
        let carried = Carried {
            // By its shape, so a model's turn whose words are gone carries what it carried whole.
            taint: match crate::secretary::turn_shape::shape(&said.turn) {
                // A stored fact; with none stored, what a session could have held: every turn
                // before it, set aside since or not, since a later edit cannot take back what
                // the session was sent.
                TurnShape::Model { .. } => stored.unwrap_or_else(|| {
                    self.carried
                        .iter()
                        .fold(Taint::Clean, |held, earlier| held.join(earlier.taint))
                }),
                TurnShape::Tool { .. } => stored.unwrap_or_else(|| taint::turn(&said)),
                TurnShape::Person { .. } | TurnShape::Act { .. } => taint::turn(&said),
            },
            bytes: weight,
        };
        let index = self.turns.len();
        let before = (0..index).rev().find(|earlier| self.is_live(*earlier));
        let opens = self.opens_after(before, &said);
        self.turns.push(said);
        self.carried.push(carried);
        self.steps.turn(index, opens, queue::weight(self, index));
    }

    /// The window's fold again from the first turn, after a rewind or an edit set turns aside.
    fn restep(&mut self) {
        let mut steps = queue::Steps::default();
        let mut before = None;
        for index in (0..self.turns.len()).filter(|index| self.is_live(*index)) {
            let opens = self.opens_after(before, &self.turns[index]);
            steps.turn(index, opens, queue::weight(self, index));
            before = Some(index);
        }
        self.steps = steps;
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
            carried: Vec::new(),
            steps: queue::Steps::default(),
            proposals: Vec::new(),
            reviews: Vec::new(),
            failures: Vec::new(),
            scope: Scope::Window,
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
            Some(TurnShape::Model { calls, .. }) => calls.is_empty(),
            Some(TurnShape::Act { .. }) => self.stops.contains(&index),
            Some(TurnShape::Person { .. } | TurnShape::Tool { .. }) | None => false,
        }
    }

    /// Whether position `at` of `live` (this conversation's [`Stored::live`]) opens a round.
    #[must_use]
    pub fn opens(&self, live: &[(usize, &Said)], at: usize) -> bool {
        self.opens_after(at.checked_sub(1).map(|before| live[before].0), live[at].1)
    }

    /// Whether `said` opens a round when the live turn before it is `before`: the first turn, or
    /// the turn after one that [closes](Stored::closes), unless it is the empty answer a round
    /// wrote after its stop, which belongs to its stop.
    #[must_use]
    pub fn opens_after(&self, before: Option<usize>, said: &Said) -> bool {
        before.is_none_or(|before| {
            self.closes(before)
                && !(self.stops.contains(&before)
                    && matches!(&said.turn, Turn::Model { text, calls } if text.is_empty() && calls.is_empty()))
        })
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
        let closing = if end > 0 && !self.opens(&live, end) && self.closes(live[end - 1].0) {
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

/// The turns from `to` go aside.
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
