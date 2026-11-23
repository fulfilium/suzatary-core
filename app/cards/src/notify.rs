//! The notification decision: a deterministic, total fold the watcher only interprets.
//!
//! Events by this deployment's own actors are skipped, so a person's own taps never page them.

mod effect;
pub use effect::{Delivered, Notify, NotifyError, Printed, Push};

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use suzatary_base::capability::Datetime;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Stored;
use suzatary_record::record::TodoEvent;

/// What to say about new chain objects, or `None` when nothing is worth a push.
///
/// `own` is the set of actor names this deployment's own writers use.
#[must_use]
pub fn decide(events: &[TodoEvent], own: &BTreeSet<String>, _now: Datetime) -> Option<Push> {
    let mut authored = 0usize;
    let mut changes = 0usize;
    for event in events {
        if own.contains(event.actor().as_str()) {
            continue;
        }
        match event {
            // A decision is something waiting for him; `needs_you` says so.
            TodoEvent::Authored(record) if is_decision(record) => {}
            TodoEvent::Authored(_) => authored += 1,
            TodoEvent::Completed(_)
            | TodoEvent::Cancelled(_)
            | TodoEvent::Reopened(_)
            | TodoEvent::Tended(_)
            | TodoEvent::SpecRevised(_) => changes += 1,
            // The `Authored` beside a `Created` already announces it.
            TodoEvent::Created(_) => {}
        }
    }
    (authored > 0 || changes > 0).then_some(Push::Agent { authored, changes })
}

/// The push for anything newly waiting on him, or `None`.
///
/// A proposal counts when these inbox events ask it and a decision when these
/// chain events create it, so an edit to one already waiting does not ring again.
/// Only a lone proposal is linked: a decision's id is a todo's name.
#[must_use]
pub fn needs_you(
    decisions: &[TodoEvent],
    asked: &[suzatary_inbox::secretary::inbox_event::InboxEvent],
) -> Option<Push> {
    let created: BTreeSet<&str> = decisions
        .iter()
        .filter(|event| matches!(event, TodoEvent::Created(_)))
        .map(|event| event.todo().as_str())
        .collect();
    let decided: BTreeSet<&str> = decisions
        .iter()
        .filter_map(|event| match event {
            TodoEvent::Authored(record) if is_decision(record) => Some(event.todo().as_str()),
            _ => None,
        })
        .filter(|id| created.contains(id))
        .collect();
    let proposals: BTreeSet<&suzatary_inbox::secretary::inbox::ProposalId> = asked
        .iter()
        .filter_map(|event| match event.said {
            suzatary_inbox::secretary::inbox_event::Said::Asked(_) => Some(&event.proposal),
            suzatary_inbox::secretary::inbox_event::Said::Responded(_) => None,
        })
        .collect();
    let count = NonZeroUsize::new(decided.len() + proposals.len())?;
    let open = match (decided.is_empty(), proposals.iter().next(), proposals.len()) {
        (true, Some(id), 1) => Some((*id).clone()),
        _ => None,
    };
    Some(Push::Waiting { count, open })
}

/// The push a finished round earns: only for an answer, never naming its words.
#[must_use]
pub fn answer_push(stored: &Stored, brief: bool) -> Option<Push> {
    let Some(Turn::Model { text, calls }) = stored.turns.last().map(|said| &said.turn) else {
        return None;
    };
    (calls.is_empty() && !text.trim().is_empty()).then(|| Push::Answered {
        conversation: stored.id.clone(),
        brief,
    })
}

fn is_decision(record: &suzatary_record::record::Authored) -> bool {
    record.payload.is_decision()
}

pub const POLL_SECONDS: u64 = 30;
/// Stillness before one push summarises what moved: the agent can write a dozen objects in one
/// pass.
pub const QUIET_SECONDS: u64 = 120;

#[cfg(test)]
mod tests {
    use prodrome_core::event::{mk_completed, mk_created, mk_reopened};
    use prodrome_core::fpl;
    use proptest::prelude::*;
    use suzatary_inbox::secretary::inbox::ProposalId;
    use suzatary_inbox::secretary::inbox_event::{InboxEvent, Said};
    use suzatary_record::record::mk_authored;

    use super::*;

    const APP: &str = "https://box.example.test";

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn own() -> BTreeSet<String> {
        ["ada".to_owned()].into_iter().collect()
    }

    fn asked(proposal: &str, why: &str) -> InboxEvent {
        let asked = suzatary_inbox::secretary::inbox::Asked {
            proposer: suzatary_inbox::secretary::proposer::Proposer::Secretary,
            proposal: suzatary_inbox::secretary::proposal::Proposal::Email(
                suzatary_inbox::secretary::proposal::Draft::default(),
            ),
            reasons: suzatary_inbox::secretary::proposal::Reasons::new(
                why,
                vec!["mail:abcdef012345".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: "proposal:default/0".parse().expect("a reference"),
            price: suzatary_inbox::secretary::inbox::Price::unpriced(),
        };
        InboxEvent {
            proposal: ProposalId::new(proposal).expect("an id"),
            at: at(1),
            actor: prodrome_data::event::Actor::new("assistant").expect("an actor"),
            said: Said::Asked(Box::new(asked)),
        }
    }

    fn authored_as(todo: &str, actor: &str, kind: &str, body: &str) -> TodoEvent {
        mk_authored(
            todo,
            at(1),
            actor,
            kind,
            at(1),
            suzatary_record::record::MarkupSource::escaped(body),
            fpl::mk_flat(0.5).expect("a spec"),
            vec!["because".to_owned()],
            "home",
            "",
            suzatary_record::record::MarkupSource::empty(),
            None,
            vec![],
            vec![],
            "",
        )
        .expect("a record")
    }

    /// A prefix no printed constant, hex id or number holds, so a hit is never a coincidence.
    fn stored() -> impl Strategy<Value = String> {
        "zq[a-z]{4,10}"
    }

    proptest! {
        /// Whatever the history holds, a printed push holds none of it.
        #[test]
        fn no_printed_push_holds_stored_bytes(
            todos in prop::collection::vec((stored(), stored(), stored(), 0u8..4), 0..12),
            reasons in prop::collection::vec(stored(), 0..4),
            answer in stored(),
            brief in any::<bool>(),
        ) {
            let events: Vec<TodoEvent> = todos
                .iter()
                .flat_map(|(todo, actor, body, kind)| match kind {
                    0 => vec![authored_as(todo, actor, "todo", body)],
                    1 => vec![mk_completed(todo, at(2), actor, body).expect("valid")],
                    2 => vec![
                        mk_created(todo, at(1), actor, "", "").expect("valid"),
                        authored_as(todo, actor, "decision", body),
                    ],
                    _ => vec![mk_reopened(todo, at(2), actor, body).expect("valid")],
                })
                .collect();
            let filed: Vec<InboxEvent> = reasons
                .iter()
                .enumerate()
                .map(|(n, why)| asked(&format!("p{n:032x}"), why))
                .collect();
            let conversation = suzatary_base::conversation_id::ConversationId::new("2026-09-15-1")
                .expect("a slug");
            let round = suzatary_conversations::secretary::store::Stored::of(
                &conversation,
                &[
                    suzatary_conversations::secretary::store::Entry::Opened {
                        at: suzatary_base::capability::Datetime::new(2026, 9, 15, 9, 0, 0, 0).expect("an instant"),
                        title: suzatary_memory::secretary::words::Words::kept("Morning".to_owned()),
                        model: "a-cli".to_owned(),
                        kind: suzatary_conversations::secretary::store::Kind::Topic,
                    },
                    suzatary_conversations::secretary::store::Entry::Said {
                        at: suzatary_base::capability::Datetime::new(2026, 9, 15, 9, 2, 0, 0).expect("an instant"),
                        turn: suzatary_conversations::secretary::model::Turn::Model {
                            text: answer.clone(),
                            calls: Vec::new(),
                        },
                        by: None,
                    },
                ],
            )
            .expect("a conversation");
            let pushes = [
                decide(&events, &own(), at(1)),
                needs_you(&events, &filed),
                answer_push(&round, brief),
            ];
            let texts: Vec<&String> = todos
                .iter()
                .flat_map(|(todo, actor, body, _)| [todo, actor, body])
                .chain(&reasons)
                .chain(std::iter::once(&answer))
                .collect();
            for printed in pushes.iter().flatten().map(|push| push.printed(APP)) {
                for field in [printed.title, printed.body.as_str(), printed.click.as_str(), printed.tag] {
                    for text in &texts {
                        prop_assert!(!field.contains(text.as_str()), "{text} crossed the relay in {field:?}");
                    }
                }
            }
        }

        #[test]
        fn a_log_of_ones_own_taps_is_always_silence(
            todos in prop::collection::vec("[a-z]{3,8}", 0..30),
        ) {
            let events: Vec<TodoEvent> = todos
                .iter()
                .map(|todo| mk_completed(todo, at(2), "ada", "").expect("valid"))
                .collect();
            prop_assert_eq!(decide(&events, &own(), at(1)), None);
        }
    }
}
