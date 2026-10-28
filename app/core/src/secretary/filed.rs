//! A proposal filed: asked once in the inbox, by its content, and drawn as a card in one
//! conversation. Every proposer files through the one function here, so every proposal is tapped
//! by the same route.

use crate::capability::Datetime;
use crate::reference::ProposalRef;
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::changes::Changeset;
use crate::secretary::inbox::{self, ProposalId};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::inbox_reading::Reading;
use crate::secretary::proposer::Proposer;
use crate::secretary::store::{ConversationId, Entry, Kind, Proposal, Reasons};
use crate::secretary::transcript;
use crate::store::{Prodrome, StoreError};

/// Where a proposal is drawn: its card's index in the conversation it was filed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filed {
    /// Drawn by this filing.
    Drawn(usize),
    /// Already drawn there: the same act asked again is the card already here, and nothing was
    /// written.
    Already(usize),
}

impl Filed {
    /// The card's index, which a tap names.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Filed::Drawn(index) | Filed::Already(index) => index,
        }
    }
}

/// What is filed, by whom, in which conversation: everything but the instant and the stores.
#[derive(Debug, Clone)]
pub struct Filing<'a> {
    /// The conversation it is drawn in.
    pub conversation: &'a ConversationId,
    /// Who asks for it.
    pub proposer: Proposer,
    /// The act.
    pub proposal: Proposal,
    /// Why, and on what.
    pub reasons: Reasons,
    /// The todo it serves.
    pub serves: Option<crate::reference::TodoRef>,
    /// The card it replaces, recorded on the line.
    pub revises: Option<usize>,
}

/// File `filing` at `at`: the inbox asked first ([`inbox::ask`], which writes nothing for an act
/// it holds), then a line naming the object, unless the conversation already draws it.
///
/// Under the inbox's one hold, taken before the conversations' as the tap takes them, so the
/// card's index is read and its line written with no other filing between, and a coding
/// session serving a todo is held to [`crate::secretary::build::admitted`] with no other filing
/// between, whoever files it, each todo read open in `chains`.
///
/// # Errors
/// [`crate::secretary::build::admitted`]'s, a conversation that cannot be read, and the
/// stores' failures to append.
pub fn file(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[crate::secretary::Bound<'_>],
    at: Datetime,
    filing: Filing<'_>,
) -> Result<(Filed, ProposalId), Refusal> {
    let _held = inbox.locked()?;
    file_held(conversations, inbox, chains, at, filing)
}

/// [`file()`], under the inbox's hold the caller has taken.
///
/// # Errors
/// As [`file()`]'s.
pub fn file_held(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[crate::secretary::Bound<'_>],
    at: Datetime,
    filing: Filing<'_>,
) -> Result<(Filed, ProposalId), Refusal> {
    let Filing {
        conversation,
        proposer,
        proposal,
        reasons,
        serves,
        revises,
    } = filing;
    if let (Proposal::Act(Act::CodingSession { .. }), Some(todo)) = (&proposal, &serves) {
        crate::secretary::build::admitted(
            &inbox::fold(inbox)?,
            &crate::secretary::coding::waiting(conversations)?,
            chains,
            at,
            todo,
        )?;
    }
    let stored = super::read(conversations, conversation)?;
    let index = stored.proposals.len();
    let asked = inbox::Asked {
        proposer,
        proposal,
        reasons,
        asked: ProposalRef {
            conversation: conversation.clone(),
            index,
        },
        price: serves.map_or_else(inbox::Price::unpriced, inbox::Price::Serves),
    };
    let object = inbox::ask_held(inbox, asked, &super::writes_as(), at)?;
    if let Some(already) = stored
        .proposals
        .iter()
        .position(|asking| asking.object == object)
    {
        // The Inbox is content-addressed: the same act asked again is the card that asked it,
        // and one somebody answered or withdrew is not asked again.
        return match inbox::fold(inbox)?.get(&object).map(|asked| &asked.reading) {
            Some(reading) if !matches!(reading, Reading::Standing) => {
                Err(Refusal::conflict(format!(
                    "this was asked before, as {}, which reads {reading}; the same act is never \
                     filed again",
                    ProposalRef {
                        conversation: conversation.clone(),
                        index: already
                    },
                )))
            }
            Some(_) | None => Ok((Filed::Already(already), object)),
        };
    }
    transcript::push(
        conversations,
        conversation,
        &Entry::Proposed {
            at,
            object: object.clone(),
            revises,
        },
    )?;
    Ok((Filed::Drawn(index), object))
}

/// The conversation the triage agent's cards are drawn in: one, the box's own.
///
/// # Panics
/// Never: `triage` is a conversation id.
#[must_use]
pub fn triage_conversation() -> ConversationId {
    ConversationId::new("triage").expect("triage is a slug")
}

/// The triage agent's one door into the Inbox: it files a changeset as the agent's card and
/// does nothing else, so a verb holding it can ask the owner and never answer for him.
#[derive(Clone, Copy)]
pub struct TriageCards<'a> {
    conversations: crate::secretary::conversations::Conversations<'a>,
    inbox: &'a dyn Prodrome<InboxEvent>,
}

/// A triage card as filed: the card a tap names, and how many acts it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carded {
    /// The card.
    pub card: ProposalRef,
    /// Its acts, the one just asked for last.
    pub acts: usize,
    /// How this write came to be on it.
    pub how: Carding,
}

/// How a triage write came to be on its card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carding {
    /// A new card.
    New,
    /// A new card holding the open card's acts about the same todo and then this one, which
    /// replaces that card.
    Joined,
    /// The open card about the same todo already asks for it: nothing was filed.
    Already,
}

impl<'a> TriageCards<'a> {
    /// The door over the box's conversations and inbox.
    #[must_use]
    pub fn new(
        conversations: crate::secretary::conversations::Conversations<'a>,
        inbox: &'a dyn Prodrome<InboxEvent>,
    ) -> Self {
        Self {
            conversations,
            inbox,
        }
    }

    /// File `act` as the triage agent's, at `at`. When an open triage card already asks for acts
    /// on the same chain and todo, the new card holds those acts and then this one and replaces
    /// it, so the Inbox holds one card per todo. `check` builds the whole changeset against the
    /// chain first, so a card the tap would refuse is refused now.
    ///
    /// # Errors
    /// `check`'s refusal, and the stores' failures.
    pub fn file(
        &self,
        at: Datetime,
        changeset: Changeset,
        reasons: Reasons,
        serves: Option<crate::reference::TodoRef>,
        check: &dyn Fn(&Changeset) -> Result<(), Refusal>,
    ) -> Result<Carded, Refusal> {
        let conversation = triage_conversation();
        opened(
            self.conversations,
            &conversation,
            "Proposed by the triage agent",
            &Proposer::Triage,
            at,
        )?;
        let _held = self.inbox.locked()?;
        let fold = inbox::fold(self.inbox)?;
        let (changeset, reasons, revises) = match open_card(&fold, &changeset) {
            None => (changeset, reasons, None),
            Some((object, standing)) => match joined(object, standing, &changeset, &reasons)? {
                Joining::Already(carded) => return Ok(carded),
                Joining::Joined(changeset, reasons) => (
                    changeset,
                    reasons,
                    Some((object.id.clone(), object.asked.asked.index)),
                ),
            },
        };
        check(&changeset)?;
        let acts = changeset.acts().len();
        let (filed, _) = file_held(
            self.conversations,
            self.inbox,
            &[],
            at,
            Filing {
                conversation: &conversation,
                proposer: Proposer::Triage,
                proposal: Proposal::Changes(changeset),
                reasons,
                serves,
                revises: revises.as_ref().map(|(_, index)| *index),
            },
        )?;
        if let (Filed::Drawn(_), Some((old, _))) = (filed, &revises) {
            inbox::withdraw_held(
                self.inbox,
                old,
                &super::writes_as(),
                at,
                crate::secretary::responded::Withdrawal::Superseded,
            )?;
        }
        Ok(Carded {
            card: ProposalRef {
                conversation,
                index: filed.index(),
            },
            acts,
            how: match (filed, &revises) {
                (Filed::Already(_), _) => Carding::Already,
                (Filed::Drawn(_), Some(_)) => Carding::Joined,
                (Filed::Drawn(_), None) => Carding::New,
            },
        })
    }
}

/// `conversation`, one the box keeps for `proposer`'s cards, opened under `title` unless it is
/// already.
///
/// # Errors
/// The store's failures to append.
pub fn opened(
    conversations: crate::secretary::conversations::Conversations<'_>,
    conversation: &ConversationId,
    title: &str,
    proposer: &Proposer,
    at: Datetime,
) -> Result<(), Refusal> {
    let opened = Entry::Opened {
        at,
        title: crate::secretary::words::Words::kept(title.to_owned()),
        model: proposer.named(),
        kind: Kind::Review,
    };
    match transcript::start(conversations, conversation, &opened) {
        Ok(()) | Err(StoreError::Taken { .. }) => Ok(()),
        Err(failed) => Err(failed.into()),
    }
}

/// The chain and the todo a changeset's first act is about: what makes two triage cards one.
fn subject(changeset: &Changeset) -> Option<(&str, &str)> {
    changeset
        .acts()
        .first()
        .map(|act| (changeset.chain(), act.todo()))
}

/// The triage card about the same chain and todo as `changeset` that nobody has answered: a card
/// the owner tapped, even one whose tap failed, is his, and a new write files a card of its own.
fn open_card<'f>(
    fold: &'f inbox::Fold,
    changeset: &Changeset,
) -> Option<(&'f inbox::Object, &'f Changeset)> {
    fold.objects()
        .find_map(|object| match &object.asked.proposal {
            Proposal::Changes(standing)
                if object.asked.proposer == Proposer::Triage
                    && matches!(
                        object.reading,
                        crate::secretary::inbox_reading::Reading::Standing
                    )
                    && subject(standing) == subject(changeset) =>
            {
                Some((object, standing))
            }
            Proposal::Changes(_)
            | Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Act(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => None,
        })
}

/// What a new write about a todo comes to beside its open card.
enum Joining {
    /// The card already asks for every act: nothing is filed.
    Already(Carded),
    /// The card's acts, then the new ones, with both reasons.
    Joined(Changeset, Reasons),
}

/// `changeset` joined to the open card `object` holding `standing`.
fn joined(
    object: &inbox::Object,
    standing: &Changeset,
    changeset: &Changeset,
    reasons: &Reasons,
) -> Result<Joining, Refusal> {
    if changeset
        .acts()
        .iter()
        .all(|act| standing.acts().contains(act))
    {
        return Ok(Joining::Already(Carded {
            card: object.asked.asked.clone(),
            acts: standing.acts().len(),
            how: Carding::Already,
        }));
    }
    let acts = standing
        .acts()
        .iter()
        .chain(changeset.acts())
        .cloned()
        .collect();
    let joined = Changeset::new(standing.chain(), acts)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let had = object.asked.reasons.sources();
    let sources = had
        .iter()
        .chain(
            reasons
                .sources()
                .iter()
                .filter(|source| !had.contains(source)),
        )
        .cloned()
        .collect();
    let why = format!("{}\n{}", object.asked.reasons.why(), reasons.why());
    let reasons =
        Reasons::new(why, sources).map_err(|failed| Refusal::internal(failed.to_string()))?;
    Ok(Joining::Joined(joined, reasons))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemory;

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 10, 27, 9, minute, 0, 0).expect("an instant")
    }

    /// A coding session serving `todo`, filed in `conversation` by `proposer`.
    fn session<'a>(conversation: &'a ConversationId, proposer: Proposer, todo: &str) -> Filing<'a> {
        let serves: crate::reference::TodoRef = format!("todo:suzatary/{todo}")
            .parse()
            .expect("a todo reference");
        Filing {
            conversation,
            proposer,
            proposal: Proposal::Act(Act::CodingSession {
                task: crate::coding::Task::try_from(format!("Build {serves}.")).expect("a task"),
            }),
            reasons: Reasons::new(
                "asked",
                vec![crate::reference::Reference::Todo(serves.clone())],
            )
            .expect("reasons"),
            serves: Some(serves),
            revises: None,
        }
    }

    /// The limit is the filing door's, under the inbox's hold: with one build in progress, a
    /// build card and the secretary's session filed at the same instant cannot both pass.
    #[test]
    fn two_filings_at_once_take_one_free_slot() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox = InMemory::<InboxEvent>::default();
        let (build, default) = (
            ConversationId::new("build").expect("a slug"),
            ConversationId::new("default").expect("a slug"),
        );
        for conversation in [&build, &default] {
            opened(
                conversations.conversations(),
                conversation,
                "Builds",
                &Proposer::Box,
                at(0),
            )
            .expect("opened");
        }
        file(
            conversations.conversations(),
            &inbox,
            &[],
            at(1),
            session(&build, Proposer::Box, "first"),
        )
        .expect("the first build files");
        let barrier = std::sync::Barrier::new(2);
        let filed: Vec<Result<(Filed, ProposalId), Refusal>> = std::thread::scope(|scope| {
            let threads = [
                (&build, Proposer::Box, "second"),
                (&default, Proposer::Secretary, "third"),
            ]
            .map(|(conversation, proposer, todo)| {
                let (conversations, inbox, barrier) = (&conversations, &inbox, &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    file(
                        conversations.conversations(),
                        inbox,
                        &[],
                        at(2),
                        session(conversation, proposer, todo),
                    )
                })
            });
            threads
                .into_iter()
                .map(|thread| thread.join().expect("the thread ran"))
                .collect()
        });
        assert_eq!(
            filed.iter().filter(|filed| filed.is_ok()).count(),
            1,
            "{filed:?}"
        );
        let refused = filed
            .iter()
            .find_map(|filed| filed.as_ref().err())
            .expect("one refused");
        assert!(
            refused.message().contains("2 todos are in progress"),
            "{}",
            refused.message()
        );
    }
}
