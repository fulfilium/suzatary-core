//! The undo of the secretary's withdrawals: a withdrawn card never stands again, so the brief
//! offers each back as an [`Act::Restore`] card.

use crate::capability::Datetime;
use crate::conversation_id::ConversationId;
use crate::reference::{ProposalRef, Reference};
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::changes::Changeset;
use crate::secretary::filed::{self, Filing};
use crate::secretary::inbox::{self, Object, Price};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposal::{Proposal, Reasons};
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{Responded, Withdrawal};
use crate::store::Prodrome;

/// How far back a brief looks for withdrawals: one day, the brief's own cadence.
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
///
/// # Errors
/// The stores' own, and [`filed::file`]'s.
pub fn offer_withdrawn(
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn Prodrome<InboxEvent>,
    chains: &[crate::secretary::Bound<'_>],
    conversation: &ConversationId,
    now: Datetime,
) -> Result<usize, Refusal> {
    let fold = inbox::fold(inbox)?;
    let since = prodrome_core::fpl::instant_of(now) - chrono::Duration::seconds(DAY_SECONDS);
    let mut filed = 0;
    for object in fold.objects() {
        let Some((_, why, at)) = withdrawn_by_the_secretary(object) else {
            continue;
        };
        let restore = Proposal::Act(Act::Restore {
            card: object.asked.asked.clone(),
        });
        let offered = fold.objects().any(|other| other.asked.proposal == restore);
        if prodrome_core::fpl::instant_of(at) < since || offered {
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
pub(crate) fn withdrawn(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::changes::Act as Change;
    use crate::secretary::conversations::InMemory;
    use crate::secretary::filed::TriageCards;

    /// A tapped restore applies exactly the changes of the card the secretary withdrew, and of no
    /// card still standing.
    #[test]
    fn a_restore_applies_the_withdrawn_card_s_changes_alone() {
        let (lines, inbox) = (
            InMemory::default(),
            crate::store::InMemory::<InboxEvent>::default(),
        );
        let at = Datetime::new(2026, 11, 8, 9, 0, 0, 0).expect("an instant");
        let file = |todo: &str| {
            let changes = Changeset::new(
                "life",
                vec![Change::Complete {
                    todo: todo.to_owned(),
                    note: "it looked done".to_owned(),
                }],
            )
            .expect("one act");
            let reasons = Reasons::new(
                "a message says so",
                vec!["briefing:2026-11-08".parse().expect("a reference")],
            )
            .expect("reasons");
            let card = TriageCards::new(lines.conversations(), &inbox)
                .file(at, changes.clone(), reasons, None, &|_| Ok(()))
                .expect("filed")
                .card;
            (card, changes)
        };
        let (withdrawn_card, changes) = file("rent");
        let (standing_card, _) = file("lease");
        let id = inbox::fold(&inbox)
            .expect("reads")
            .objects()
            .find(|object| object.asked.asked == withdrawn_card)
            .expect("filed")
            .id
            .clone();
        inbox::withdraw_unanswered(
            &inbox,
            &id,
            &crate::secretary::transcript::writer(),
            at,
            Withdrawal::NoLongerNeeded {
                why: "a duplicate".to_owned(),
            },
        )
        .expect("withdrawn");
        assert_eq!(withdrawn(&inbox, &withdrawn_card).expect("reads"), changes);
        assert!(
            withdrawn(&inbox, &standing_card).is_err(),
            "a standing card is no restore's"
        );
    }
}
