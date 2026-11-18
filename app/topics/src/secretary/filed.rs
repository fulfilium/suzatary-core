//! A proposal filed: asked once in the inbox, by its content, and drawn as a card in one
//! conversation. Every proposer files through [`file()`].

use crate::secretary::build::Chain;
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;
use suzatary_base::reference::TodoRef;
use suzatary_base::refusal::Refusal;
use suzatary_conversations::secretary::store::{Entry, Kind};
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::changes::Changeset;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::inbox_reading::Reading;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded;
use suzatary_lines::secretary::conversations;
use suzatary_lines::secretary::transcript;
use suzatary_store::store::{Prodrome, StoreError};

/// Where a proposal is drawn: its card's index in the conversation it was filed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filed {
    Drawn(usize),
    /// The same act asked again: the card already here, and nothing was written.
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
    pub conversation: &'a ConversationId,
    pub proposer: Proposer,
    pub proposal: Proposal,
    pub reasons: Reasons,
    pub serves: Option<TodoRef>,
    /// The card it replaces, recorded on the line.
    pub revises: Option<usize>,
}

/// File `filing` at `at`: ask the inbox, then write a line naming the object unless the
/// conversation already draws it.
///
/// Under the inbox's hold, taken before the conversations' as the tap takes them, so no other
/// filing comes between the card's index and its line, or between a coding session and
/// [`crate::secretary::build::admitted`].
///
/// # Errors
/// [`crate::secretary::build::admitted`]'s, a conversation that cannot be read, and the
/// stores' failures to append.
pub fn file(
    conversations: conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[Chain<'_>],
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
    conversations: conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[Chain<'_>],
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
    let stored = conversations::read(conversations, conversation)?;
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
    let object = inbox::ask_held(inbox, asked, &transcript::writer(), at)?;
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
                    "this was asked before, as {}, which is {}; the same act is never filed again",
                    ProposalRef {
                        conversation: conversation.clone(),
                        index: already
                    },
                    phase_said(reading),
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

/// The triage agent's one door into the Inbox: it files a changeset or a box act as the agent's
/// card and does nothing else, so a verb holding it can ask the owner and never answer for him.
#[derive(Clone, Copy)]
pub struct TriageCards<'a> {
    conversations: conversations::Conversations<'a>,
    inbox: &'a dyn Prodrome<InboxEvent>,
}

/// A triage card as filed: the card a tap names, and how many acts it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carded {
    pub card: ProposalRef,
    /// Its acts, the one just asked for last.
    pub acts: usize,
    pub how: Carding,
}

/// How a triage write came to be on its card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carding {
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
        conversations: conversations::Conversations<'a>,
        inbox: &'a dyn Prodrome<InboxEvent>,
    ) -> Self {
        Self {
            conversations,
            inbox,
        }
    }

    /// File the box act `act` as the triage agent's card, at `at`; the same act asked again
    /// answers the card already here.
    ///
    /// # Errors
    /// The stores' failures.
    pub fn file_act(
        &self,
        at: Datetime,
        act: suzatary_inbox::secretary::acts::Act,
        reasons: Reasons,
    ) -> Result<ProposalRef, Refusal> {
        let conversation = triage_conversation();
        opened(
            self.conversations,
            &conversation,
            "Proposed by the triage agent",
            &Proposer::Triage,
            at,
        )?;
        let (filed, _) = file(
            self.conversations,
            self.inbox,
            &[],
            at,
            Filing {
                conversation: &conversation,
                proposer: Proposer::Triage,
                proposal: Proposal::Act(act),
                reasons,
                serves: None,
                revises: None,
            },
        )?;
        Ok(ProposalRef {
            conversation,
            index: filed.index(),
        })
    }

    /// File `act` as the triage agent's, at `at`, keeping one card per todo: a write the open card
    /// already asks for files nothing, and other acts on the same todo join it in a card that
    /// replaces it. `check` refuses now a card the tap would refuse.
    ///
    /// # Errors
    /// [`Refusal::conflict`] for a write an answered card already asked for on the same sources,
    /// `check`'s refusal, and the stores' failures.
    pub fn file(
        &self,
        at: Datetime,
        changeset: Changeset,
        reasons: Reasons,
        serves: Option<TodoRef>,
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
        let open = open_card(&fold, &changeset);
        if let Some((object, standing)) = open
            && asks_for(standing, &changeset)
        {
            return Ok(Carded {
                card: object.asked.asked.clone(),
                acts: standing.acts().len(),
                how: Carding::Already,
            });
        }
        if let Some(answered) = answered_card(&fold, &changeset, &reasons) {
            return Err(Refusal::conflict(format!(
                "this was asked before, as {}, which is {}; the same request, whatever its \
                 price, is filed again only on a source that card did not rest on",
                answered.asked.asked,
                phase_said(&answered.reading),
            )));
        }
        let (changeset, reasons, revises) = match open {
            None => (changeset, reasons, None),
            Some((object, standing)) => {
                let (changeset, reasons) = joined(object, standing, &changeset, &reasons)?;
                (
                    changeset,
                    reasons,
                    Some((object.id.clone(), object.asked.asked.index)),
                )
            }
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
                &transcript::writer(),
                at,
                responded::Withdrawal::Superseded,
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
    conversations: conversations::Conversations<'_>,
    conversation: &ConversationId,
    title: &str,
    proposer: &Proposer,
    at: Datetime,
) -> Result<(), Refusal> {
    let opened = Entry::Opened {
        at,
        title: suzatary_memory::secretary::words::Words::kept(title.to_owned()),
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

/// The changeset of `object` when it is a card the triage agent filed, in any reading.
#[must_use]
pub fn triage_card(object: &inbox::Object) -> Option<&Changeset> {
    match &object.asked.proposal {
        Proposal::Changes(changeset) if object.asked.proposer == Proposer::Triage => {
            Some(changeset)
        }
        Proposal::Changes(_)
        | Proposal::Email(_)
        | Proposal::Forward(_)
        | Proposal::Act(_)
        | Proposal::Remember { .. }
        | Proposal::Decide(_) => None,
    }
}

fn triage_cards(fold: &inbox::Fold) -> impl Iterator<Item = (&inbox::Object, &Changeset)> {
    fold.objects()
        .filter_map(|object| triage_card(object).map(|changeset| (object, changeset)))
}

/// The triage card about the same chain and todo as `changeset` that nobody has answered: a card
/// the owner tapped, even one whose tap failed, is his, and a new write files a card of its own.
fn open_card<'f>(
    fold: &'f inbox::Fold,
    changeset: &Changeset,
) -> Option<(&'f inbox::Object, &'f Changeset)> {
    triage_cards(fold).find(|(object, standing)| {
        matches!(object.reading, Reading::Standing) && subject(standing) == subject(changeset)
    })
}

fn asked_by(card: &Changeset, act: &suzatary_inbox::secretary::changes::Act) -> bool {
    card.acts()
        .iter()
        .any(|asked| asked.request() == act.request())
}

fn asks_for(card: &Changeset, changeset: &Changeset) -> bool {
    card.chain() == changeset.chain() && changeset.acts().iter().all(|act| asked_by(card, act))
}

/// An answered card that asked for every act of `changeset` on every source `reasons` names:
/// the same request on the same evidence is never asked again.
fn answered_card<'f>(
    fold: &'f inbox::Fold,
    changeset: &Changeset,
    reasons: &Reasons,
) -> Option<&'f inbox::Object> {
    triage_cards(fold)
        .find(|(object, card)| {
            !matches!(object.reading, Reading::Standing)
                && asks_for(card, changeset)
                && reasons
                    .sources()
                    .iter()
                    .all(|source| object.asked.reasons.sources().contains(source))
        })
        .map(|(object, _)| object)
}

/// `changeset` joined to the open card `object`, with both reasons.
fn joined(
    object: &inbox::Object,
    standing: &Changeset,
    changeset: &Changeset,
    reasons: &Reasons,
) -> Result<(Changeset, Reasons), Refusal> {
    let acts = standing
        .acts()
        .iter()
        .chain(
            changeset
                .acts()
                .iter()
                .filter(|act| !asked_by(standing, act)),
        )
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
    Ok((joined, reasons))
}

/// A card's state as a refusal says it, never its recorded words, which can hold a server's
/// reply (`taint`).
fn phase_said(reading: &Reading) -> String {
    serde_json::to_value(crate::secretary::readings::state(reading).0)
        .ok()
        .and_then(|state| state.as_str().map(str::to_owned))
        .unwrap_or_default()
}
