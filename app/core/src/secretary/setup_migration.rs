//! `suzatary secretary migrate-setup`: withdraw, once, the setup set's waiting cards that an
//! earlier build filed more than once.
//!
//! Before one need had one card ([`crate::secretary::store::Need`]), the box filed a calendar's
//! link again on every notice, each card's words naming the card before it. The fix files no
//! more of them, and an open card of a need holds it, so the duplicates would stand for good. A
//! run withdraws each waiting card of the set's kinds (`noticed::owns`) whose words name
//! an earlier card, which no rule writes now, and of the rest that ask for one need, all but
//! the newest. [`asked_again`] then files at once a card for each secret still missing, in the
//! words the secrets rule writes now, rather than at the next notice. A second run withdraws
//! nothing.
//!
//! This module goes with the verb once the box has run it.

use crate::capability::Datetime;
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::inbox::{self, Object};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::Withdrawal;
use crate::secretary::store::Proposal;
use crate::store::Prodrome;

/// The words of the secrets rule's card as an earlier build wrote them, naming the card before.
const NAMED_EARLIER: &str = "it no longer resolves after proposal:";

/// Why a run withdraws a card.
pub const WITHDRAWN: &str = "the box asked for this more than once: one card asks for it now";

/// The waiting cards a run withdraws, oldest first.
#[must_use]
pub fn plan(fold: &inbox::Fold) -> Vec<&Object> {
    let waiting: Vec<&Object> = fold
        .objects()
        .filter(|object| {
            object.asked.proposer == Proposer::Box
                && super::noticed::owns(&object.asked.proposal)
                && object.reading.performs()
        })
        .collect();
    let kept = |object: &Object| {
        !matches!(&object.asked.proposal,
            Proposal::Act(Act::Secret { why, .. }) if why.contains(NAMED_EARLIER))
    };
    let mut withdrawn: Vec<&Object> = waiting
        .iter()
        .copied()
        .filter(|object| {
            !kept(object)
                || waiting.iter().any(|newer| {
                    kept(newer)
                        && newer.asked.proposal.need() == object.asked.proposal.need()
                        && (newer.at, &newer.id) > (object.at, &object.id)
                })
        })
        .collect();
    withdrawn.sort_by(|a, b| (a.at, &a.id).cmp(&(b.at, &b.id)));
    withdrawn
}

/// Withdraw what [`plan`] names, at `at`, under the inbox's one hold, and answer each card's
/// place and headline.
///
/// # Errors
/// The store's failures to read or append.
pub fn run(
    inbox_store: &dyn Prodrome<InboxEvent>,
    at: Datetime,
) -> Result<Vec<(crate::reference::ProposalRef, String)>, Refusal> {
    let _held = inbox_store.locked()?;
    let fold = inbox::fold(inbox_store)?;
    let planned = plan(&fold);
    for object in &planned {
        inbox::withdraw_held(
            inbox_store,
            &object.id,
            &super::writes_as(),
            at,
            Withdrawal::NoLongerNeeded {
                why: WITHDRAWN.to_owned(),
            },
        )?;
    }
    Ok(planned.iter().map(|object| said(object)).collect())
}

/// File now what the secrets rule owes, the one rule whose cards named an earlier card, so a need
/// whose only cards a run withdrew is asked for at once and not at the next notice. It is filed
/// [holding](super::noticed::Owed::holding), so it withdraws nothing another rule still needs;
/// while the store is locked it files nothing, and the next notice asks.
///
/// # Errors
/// The stores' failures to read or append.
pub fn asked_again(
    settables: &crate::settings::Settables,
    secrets: &dyn crate::capability::Secrets,
    pushed: &crate::settings::remote::Pushed,
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox_store: &dyn Prodrome<InboxEvent>,
    at: Datetime,
) -> Result<super::noticed::Filed, Refusal> {
    use crate::capability::Answer;
    let in_use = settables.restricted(|by| by.in_use(pushed));
    let snapshot = crate::capability::Snapshot::read(secrets, in_use.iter());
    let Ok(answers) = snapshot.answers() else {
        return Ok(super::noticed::Filed::default());
    };
    super::fixes::owed(
        &in_use,
        |name| answers.get(name) == Some(&Answer::Missing),
        &inbox::fold(inbox_store)?,
    )
    .holding()
    .file(conversations, inbox_store, at)
}

/// A card as a run names it: where it was asked, and its headline.
#[must_use]
pub fn said(object: &Object) -> (crate::reference::ProposalRef, String) {
    (
        object.asked.asked.clone(),
        object
            .asked
            .proposal
            .headline(|_| crate::secretary::inbox_reading::Words::Unread),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reference::ProposalRef;
    use crate::secretary::store::{ConversationId, Reasons};
    use crate::store::InMemory;
    use crate::store::Reads as _;

    fn at(minute: u32) -> Datetime {
        Datetime::new(2026, 10, 28, 9, minute, 0, 0).expect("an instant")
    }

    fn ask(store: &InMemory<InboxEvent>, proposer: Proposer, name: &str, why: &str, minute: u32) {
        let asked = inbox::Asked {
            proposer,
            proposal: Proposal::Act(Act::Secret {
                name: name.to_owned(),
                why: why.to_owned(),
            }),
            reasons: Reasons::new(
                "asked",
                vec!["briefing:2026-10-28".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new("setup").expect("a slug"),
                index: usize::try_from(minute).expect("small"),
            },
            price: inbox::Price::unpriced(),
        };
        // Appended whole, as an earlier build asked, since the inbox now holds a second card of
        // one need as the first.
        store
            .append(InboxEvent {
                proposal: inbox::ProposalId::of(&asked, at(minute)),
                at: at(minute),
                actor: super::super::writes_as(),
                said: crate::secretary::inbox_event::Said::Asked(Box::new(asked)),
            })
            .expect("asked");
    }

    fn open(store: &InMemory<InboxEvent>) -> Vec<(String, String)> {
        inbox::fold(store)
            .expect("reads")
            .objects()
            .filter(|object| object.reading.open())
            .filter_map(|object| match &object.asked.proposal {
                Proposal::Act(Act::Secret { name, why }) => Some((name.clone(), why.clone())),
                _ => None,
            })
            .collect()
    }

    /// A run leaves the newest card of each need whose words name no earlier card, and every
    /// card the box did not ask for; a second run withdraws nothing.
    #[test]
    fn a_run_leaves_one_card_per_need_and_runs_once() {
        let store = InMemory::<InboxEvent>::default();
        let named = |n: u32| format!("the ics plugin's link reads it, {NAMED_EARLIER}setup/{n}");
        ask(&store, Proposer::Box, "PLUGIN_ICS_BU", &named(1), 2);
        ask(&store, Proposer::Box, "PLUGIN_ICS_BU", &named(2), 3);
        ask(
            &store,
            Proposer::Box,
            "NTFY_TOPIC",
            "the ntfy channel reads it",
            4,
        );
        ask(
            &store,
            Proposer::Box,
            "NTFY_TOPIC",
            "the ntfy channel's own words",
            5,
        );
        ask(&store, Proposer::Secretary, "PLUGIN_ICS_BU", &named(3), 6);
        let withdrawn = run(&store, at(10)).expect("ran");
        assert_eq!(
            withdrawn
                .iter()
                .map(|(card, _)| card.index)
                .collect::<Vec<_>>(),
            [2, 3, 4]
        );
        let mut left = open(&store);
        left.sort();
        assert_eq!(
            left,
            [
                (
                    "NTFY_TOPIC".to_owned(),
                    "the ntfy channel's own words".to_owned()
                ),
                ("PLUGIN_ICS_BU".to_owned(), named(3)),
            ]
        );
        assert!(run(&store, at(11)).expect("ran").is_empty());
    }

    /// A store holding nothing.
    struct Empty;

    impl crate::capability::Secrets for Empty {
        fn secret_state(&self, _: &crate::settings::Settable) -> crate::capability::SecretState {
            crate::capability::SecretState::Missing
        }

        fn mail_grants(
            &self,
            _: &crate::mail::account::MailAccount,
        ) -> crate::capability::MailGrants {
            crate::capability::MailGrants::default()
        }

        fn google_grant(&self, _: &crate::mail::address::EmailAddress) -> crate::google::Held {
            crate::google::Held::NotSignedIn
        }
    }

    /// A secret whose only card named an earlier one is asked for again at once, on one card in
    /// today's words, and asking again files nothing more.
    #[test]
    fn a_need_whose_only_card_is_withdrawn_is_asked_for_at_once() {
        let store = InMemory::<InboxEvent>::default();
        let conversations = crate::secretary::conversations::InMemory::default();
        let settables = crate::settings::Settables::derive(
            crate::settings::MODEL_TOKEN,
            [crate::connect::Channel::Ntfy],
            None,
            &[],
        );
        let named = format!("the ntfy channel reads it, {NAMED_EARLIER}setup/1");
        ask(&store, Proposer::Box, "NTFY_TOPIC", &named, 2);
        assert_eq!(run(&store, at(10)).expect("ran").len(), 1);
        let again = || {
            asked_again(
                &settables,
                &Empty,
                &crate::settings::remote::Pushed::Nowhere,
                conversations.conversations(),
                &store,
                at(10),
            )
            .expect("filed")
        };
        again();
        let ntfy = || {
            open(&store)
                .into_iter()
                .filter(|(name, _)| name == "NTFY_TOPIC")
                .collect::<Vec<_>>()
        };
        let standing = ntfy();
        assert_eq!(standing.len(), 1, "{standing:?}");
        assert!(!standing[0].1.contains(NAMED_EARLIER), "{standing:?}");
        let objects = store.objects().expect("reads").len();
        again();
        assert_eq!(store.objects().expect("reads").len(), objects);
    }
}
