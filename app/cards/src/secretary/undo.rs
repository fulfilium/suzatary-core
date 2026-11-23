//! The undo of the secretary's withdrawals: a withdrawn card never stands again, so the brief
//! offers each back as an [`Act::Restore`] card.

use suzatary_base::capability::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{ProposalRef, Reference};
use suzatary_base::refusal::Refusal;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::changes::Changeset;
use suzatary_inbox::secretary::inbox;
use suzatary_inbox::secretary::inbox::{Object, Price};
use suzatary_inbox::secretary::inbox_event::InboxEvent;
use suzatary_inbox::secretary::proposal::{Proposal, Reasons};
use suzatary_inbox::secretary::proposer::Proposer;
use suzatary_inbox::secretary::responded::{Responded, Withdrawal};
use suzatary_store::store::Prodrome;
use suzatary_topics::secretary::filed::{self, Filing};

/// How far back a brief looks for withdrawals: the brief's own cadence.
const DAY_SECONDS: i64 = 24 * 60 * 60;

/// A triage card the secretary withdrew: its changes, why, and when.
fn withdrawn_by_the_secretary(object: &Object) -> Option<(&Changeset, &str, Datetime)> {
    let answer = object.reading.shown()?;
    if let (
        Proposer::Triage,
        Proposal::Changes(changeset),
        Responded::Withdrawn(Withdrawal::NoLongerNeeded { why }),
    ) = (
        &object.asked.proposer,
        &object.asked.proposal,
        &answer.response,
    ) {
        return Some((changeset, why, answer.at));
    }
    None
}

/// File in `conversation` a restore card per triage card the secretary withdrew in the day
/// before `now`, once each, and say how many.
pub fn offer_withdrawn(
    conversations: suzatary_lines::secretary::conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[suzatary_topics::secretary::build::Chain<'_>],
    conversation: &ConversationId,
    now: Datetime,
) -> Result<usize, Refusal> {
    let fold = inbox::fold(inbox)?;
    let since = prodrome_data::instant::instant_of(now) - chrono::Duration::seconds(DAY_SECONDS);
    let mut filed = 0;
    for object in fold.objects() {
        let Some((_, why, at)) = withdrawn_by_the_secretary(object) else {
            continue;
        };
        let restore = Proposal::Act(Act::Restore {
            card: object.asked.asked.clone(),
        });
        let offered = fold.objects().any(|other| other.asked.proposal == restore);
        if prodrome_data::instant::instant_of(at) < since || offered {
            continue;
        }
        let card = Reference::Proposal(object.asked.asked.clone());
        let reasons = Reasons::new(
            format!(
                "The secretary withdrew the triage agent's card {card}: {why} Apply its changes \
                 after all, or dismiss this to keep it withdrawn."
            ),
            object.asked.reasons.sources().to_vec(),
        )
        .map_err(|failed| Refusal::internal(failed.to_string()))?
        .citing(card);
        filed::file(
            conversations,
            inbox,
            chains,
            now,
            Filing {
                conversation,
                proposer: Proposer::Secretary,
                proposal: restore,
                reasons,
                serves: match &object.asked.price {
                    Price::Serves(todo) => Some(todo.clone()),
                    Price::Own(_) => None,
                },
                revises: None,
            },
        )?;
        filed += 1;
    }
    Ok(filed)
}

/// What a tapped [`Act::Restore`] applies: the changes of the card the secretary withdrew.
pub fn withdrawn(
    inbox: &dyn Prodrome<InboxEvent>,
    card: &ProposalRef,
) -> Result<Changeset, Refusal> {
    inbox::fold(inbox)?
        .objects()
        .find(|object| object.asked.asked == *card)
        .and_then(withdrawn_by_the_secretary)
        .map(|(changeset, _, _)| changeset.clone())
        .ok_or_else(|| {
            Refusal::not_found(format!("{card} is no triage card the secretary withdrew"))
        })
}
