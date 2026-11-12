//! A proposal filed: asked once in the inbox, by its content, and drawn as a card in one
//! conversation. Every proposer files through the one function here, so every proposal is tapped
//! by the same route.

use crate::secretary::acts::Act;
use crate::secretary::changes::Changeset;
use crate::secretary::conversations;
use crate::secretary::inbox::{self, ProposalId};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::inbox_reading::Reading;
use crate::secretary::proposal::{Proposal, Reasons};
use crate::secretary::proposer::Proposer;
use crate::secretary::store::{Entry, Kind};
use crate::secretary::transcript;
use crate::store::{Prodrome, StoreError};
use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::ProposalRef;
use suzatary_base::refusal::Refusal;

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
    pub serves: Option<suzatary_base::reference::TodoRef>,
    /// The card it replaces, recorded on the line.
    pub revises: Option<usize>,
}

/// File `filing` at `at`: the inbox asked first ([`inbox::ask`], which writes nothing for an act
/// it holds), then a line naming the object, unless the conversation already draws it.
///
/// Under the inbox's hold, taken before the conversations' as the tap takes them, so no other
/// filing comes between the card's index and its line, or between a coding session and
/// [`crate::secretary::build::admitted`].
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

/// The triage agent's one door into the Inbox: it files a changeset or a box act as the agent's
/// card and does nothing else, so a verb holding it can ask the owner and never answer for him.
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

    /// File the box act `act` as the triage agent's card, at `at`; the same act asked again
    /// answers the card already here.
    ///
    /// # Errors
    /// The stores' failures.
    pub fn file_act(
        &self,
        at: Datetime,
        act: crate::secretary::acts::Act,
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
        serves: Option<suzatary_base::reference::TodoRef>,
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
                "this was asked before, as {}, which reads {}; the same request, whatever its \
                 price, is filed again only on a source that card did not rest on",
                answered.asked.asked, answered.reading,
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

/// Every card the triage agent filed, in any reading, with its changeset.
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

/// Whether `act` makes a request `card` already makes.
fn asked_by(card: &Changeset, act: &crate::secretary::changes::Act) -> bool {
    card.acts()
        .iter()
        .any(|asked| asked.request() == act.request())
}

/// Whether `card` asks for every act of `changeset`, on the same chain.
fn asks_for(card: &Changeset, changeset: &Changeset) -> bool {
    card.chain() == changeset.chain() && changeset.acts().iter().all(|act| asked_by(card, act))
}

/// A card somebody answered (tapped, dismissed or withdrew) that asked for every act of
/// `changeset` on every source `reasons` names: the same request on the same evidence, which is
/// never asked again. A source that card did not rest on makes the write a new request.
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

/// `changeset` joined to the open card `object` holding `standing`: the card's acts, then those of
/// `changeset` it does not already ask for, with both reasons.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemory;

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 10, 27, 9, minute, 0, 0).expect("an instant")
    }

    /// A coding session serving `todo`, filed in `conversation` by `proposer`.
    fn session<'a>(conversation: &'a ConversationId, proposer: Proposer, todo: &str) -> Filing<'a> {
        let serves: suzatary_base::reference::TodoRef = format!("todo:suzatary/{todo}")
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
                vec![suzatary_base::reference::Reference::Todo(serves.clone())],
            )
            .expect("reasons"),
            serves: Some(serves),
            revises: None,
        }
    }

    /// `todo` asked for as a new todo (0), an update that names a price (1) or a reprice (2), at
    /// `percent`, with a rationale that differs on every pass.
    fn priced(shape: u8, todo: &str, percent: u32, pass: usize) -> crate::secretary::changes::Act {
        use crate::secretary::changes::Act as ChainAct;
        use suzatary_record::todo::{Patch, Price, TodoFields, TodoPatch};
        let price = Price::Flat { percent };
        let rationale = format!("read on pass {pass}");
        match shape {
            0 => ChainAct::Add {
                todo: todo.to_owned(),
                fields: TodoFields {
                    body: "Answer the chat request".to_owned(),
                    rationale,
                    kind: "todo".to_owned(),
                    category: String::new(),
                    price,
                    detail: String::new(),
                    waiting_on: String::new(),
                },
                from: crate::secretary::changes::Provenance::default(),
            },
            1 => ChainAct::Update {
                todo: todo.to_owned(),
                fields: TodoPatch {
                    detail_append: Patch::Set("The sender asked again.".to_owned()),
                    rationale: Patch::Set(rationale),
                    price: Patch::Set(price),
                    ..TodoPatch::default()
                },
            },
            _ => ChainAct::Reprice {
                todo: todo.to_owned(),
                priority: price,
                rationale,
            },
        }
    }

    /// Reasons resting on `sources`, each a briefing's day.
    fn resting_on(sources: &[&str]) -> Reasons {
        Reasons::new(
            "a message asks for it",
            sources
                .iter()
                .map(|day| format!("briefing:{day}").parse().expect("a reference"))
                .collect(),
        )
        .expect("reasons")
    }

    /// One triage write of `act` at `at`, every changeset admitted.
    fn filed_triage(
        cards: &TriageCards<'_>,
        at: Datetime,
        act: crate::secretary::changes::Act,
        reasons: Reasons,
    ) -> Result<Carded, Refusal> {
        cards.file(
            at,
            Changeset::new("life", vec![act]).expect("one act"),
            reasons,
            None,
            &|_| Ok(()),
        )
    }

    proptest::proptest! {
        /// THE REFILE LOOP: the triage agent proposing the same request on every pass, its price
        /// and rationale read afresh each time, leaves exactly one standing card, the first, with
        /// the first pass's one act; no pass files, joins or withdraws.
        #[test]
        fn the_same_request_on_every_pass_is_one_standing_card(
            shape in 0u8..3,
            percents in proptest::collection::vec(1u32..100, 1..12),
        ) {
            let conversations = crate::secretary::conversations::InMemory::default();
            let inbox = InMemory::<InboxEvent>::default();
            let cards = TriageCards::new(conversations.conversations(), &inbox);
            let mut first = None;
            for (pass, percent) in percents.iter().enumerate() {
                let carded = filed_triage(
                    &cards,
                    at(u32::try_from(pass).expect("a minute")),
                    priced(shape, "chatrequest", *percent, pass),
                    resting_on(&["2026-11-03"]),
                )
                .expect("filed");
                let first = first.get_or_insert_with(|| carded.card.clone());
                proptest::prop_assert_eq!(&carded.card, &*first);
                proptest::prop_assert_eq!(carded.acts, 1);
                proptest::prop_assert_eq!(
                    carded.how,
                    if pass == 0 { Carding::New } else { Carding::Already }
                );
            }
            let standing = crate::secretary::verbs::standing(&inbox).expect("the inbox reads");
            proptest::prop_assert_eq!(standing.len(), 1, "{:?}", standing);
            proptest::prop_assert_eq!(inbox::fold(&inbox).expect("reads").objects().count(), 1);
        }
    }

    /// A request somebody answered is not asked again on the same sources at another price, and
    /// is asked again, as a new card, on a source the answered card did not rest on.
    #[test]
    fn an_answered_request_comes_back_only_on_a_new_source() {
        let conversations = crate::secretary::conversations::InMemory::default();
        let inbox = InMemory::<InboxEvent>::default();
        let cards = TriageCards::new(conversations.conversations(), &inbox);
        let first = filed_triage(
            &cards,
            at(0),
            priced(2, "permit", 62, 0),
            resting_on(&["2026-11-02"]),
        )
        .expect("filed");
        let id = inbox::fold(&inbox)
            .expect("reads")
            .objects()
            .find(|object| object.asked.asked == first.card)
            .expect("the card")
            .id
            .clone();
        inbox::withdraw(
            &inbox,
            &id,
            &transcript::writer(),
            at(1),
            crate::secretary::responded::Withdrawal::NoLongerNeeded {
                why: "a repricing with no new fact".to_owned(),
            },
        )
        .expect("withdrawn");
        let refused = filed_triage(
            &cards,
            at(2),
            priced(2, "permit", 61, 1),
            resting_on(&["2026-11-02"]),
        )
        .expect_err("the same request on the same source");
        assert!(
            refused.message().contains(&first.card.to_string()),
            "{}",
            refused.message()
        );
        let again = filed_triage(
            &cards,
            at(3),
            priced(2, "permit", 40, 2),
            resting_on(&["2026-11-02", "2026-11-03"]),
        )
        .expect("a new source is a new request");
        assert_eq!(again.how, Carding::New);
        assert_ne!(again.card, first.card);
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
