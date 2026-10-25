//! The notification decision: a deterministic, total fold the watcher only interprets.
//!
//! A push is a [`Push`], which holds counts and ids and no text, so what a relay carries is
//! a fact about its type. Events by this deployment's own actors are skipped, so a person's own
//! taps never page them.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

use crate::capability::{Datetime, Push};
use crate::record::TodoEvent;

/// What to say about new chain objects, or `None` when nothing is worth a push.
///
/// `own` is the set of actor names this deployment's own writers use. `now` is a parameter so
/// that no clock is read inside.
#[must_use]
pub fn decide(events: &[TodoEvent], own: &BTreeSet<String>, _now: Datetime) -> Option<Push> {
    let mut authored = 0usize;
    let mut changes = 0usize;
    for event in events {
        if own.contains(event.actor().as_str()) {
            continue;
        }
        match event {
            // A decision is something waiting for him, not the agent's work
            // on a todo; `needs_you` says so.
            TodoEvent::Authored(record) if is_decision(record) => {}
            TodoEvent::Authored(_) => authored += 1,
            TodoEvent::Completed(_)
            | TodoEvent::Cancelled(_)
            | TodoEvent::Reopened(_)
            | TodoEvent::Tended(_)
            | TodoEvent::SpecRevised(_) => changes += 1,
            // A `Created` is the id being minted, which the `Authored` beside
            // it already announces; counting both would double every new todo.
            TodoEvent::Created(_) => {}
        }
    }
    (authored > 0 || changes > 0).then_some(Push::Agent { authored, changes })
}

/// The push for anything newly waiting on him, or `None`.
///
/// A proposal counts when these inbox events ask it and a decision when these
/// chain events create it, so an edit to one already waiting does not ring again.
/// Only a lone proposal is linked: its id is a hash, where a decision's is a todo's name.
#[must_use]
pub fn needs_you(
    decisions: &[TodoEvent],
    asked: &[crate::secretary::inbox_event::InboxEvent],
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
    let proposals: BTreeSet<&crate::secretary::inbox::ProposalId> = asked
        .iter()
        .filter_map(|event| match event.said {
            crate::secretary::inbox_event::Said::Asked(_) => Some(&event.proposal),
            crate::secretary::inbox_event::Said::Responded(_) => None,
        })
        .collect();
    let count = NonZeroUsize::new(decided.len() + proposals.len())?;
    let open = match (decided.is_empty(), proposals.iter().next(), proposals.len()) {
        (true, Some(id), 1) => Some((*id).clone()),
        _ => None,
    };
    Some(Push::Waiting { count, open })
}

fn is_decision(record: &crate::record::Authored) -> bool {
    record.payload.is_decision()
}

/// The secret names `channel` needs: its template's steps.
#[must_use]
pub fn channel_secrets(channel: crate::connect::Channel) -> Vec<&'static str> {
    use crate::connect::{SecretName, StepShape};
    channel
        .template()
        .template()
        .steps
        .iter()
        .filter_map(|step| match step {
            StepShape::Secret {
                name: SecretName::Fixed(secret),
                ..
            } => Some(*secret),
            StepShape::Secret {
                name: SecretName::Link,
                ..
            }
            | StepShape::DeviceCode
            | StepShape::SendingConsent
            | StepShape::GoogleClient
            | StepShape::AuthCode
            | StepShape::SignIn
            | StepShape::Terminal { .. } => None,
        })
        .collect()
}

/// How often the watcher looks at the tip.
pub const POLL_SECONDS: u64 = 30;
/// How many seconds of stillness before one push summarises what moved.
///
/// A burst is one notification: the agent can write a dozen objects in one pass.
pub const QUIET_SECONDS: u64 = 120;

#[cfg(test)]
mod tests {
    use crate::record::mk_authored;
    use crate::secretary::inbox::ProposalId;
    use crate::secretary::inbox_event::{InboxEvent, Said};
    use prodrome_core::event::{mk_completed, mk_created, mk_reopened};
    use prodrome_core::fpl;
    use proptest::prelude::*;

    use super::*;

    const APP: &str = "https://box.example.test";

    fn at(day: u32) -> Datetime {
        Datetime::new(2026, 9, day, 12, 0, 0, 0).expect("a real instant")
    }

    fn own() -> BTreeSet<String> {
        ["ada".to_owned()].into_iter().collect()
    }

    fn asked(proposal: &str, why: &str) -> InboxEvent {
        let asked = crate::secretary::inbox::Asked {
            proposer: crate::secretary::proposer::Proposer::Secretary,
            proposal: crate::secretary::store::Proposal::Email(
                crate::secretary::store::Draft::default(),
            ),
            reasons: crate::secretary::store::Reasons::new(
                why,
                vec!["mail:abcdef012345".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: "proposal:default/0".parse().expect("a reference"),
            price: crate::secretary::inbox::Price::unpriced(),
        };
        InboxEvent {
            proposal: ProposalId::new(proposal).expect("an id"),
            at: at(1),
            actor: prodrome_core::event::Actor::new("assistant").expect("an actor"),
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
            crate::record::MarkupSource::escaped(body),
            fpl::mk_flat(0.5).expect("a spec"),
            vec!["because".to_owned()],
            "home",
            "",
            crate::record::MarkupSource::empty(),
            None,
            vec![],
            vec![],
            "",
        )
        .expect("a record")
    }

    /// Stored text: lowercase letters behind a prefix no printed constant, hex id or number
    /// holds, so a hit is the text itself and never a coincidence.
    fn stored() -> impl Strategy<Value = String> {
        "zq[a-z]{4,10}"
    }

    proptest! {
        /// Whatever the history holds, a printed push holds none of it: every todo's id,
        /// author and body, every proposal's reason and every answer's words stay on the box.
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
            let conversation = crate::secretary::store::ConversationId::new("2026-09-15-1")
                .expect("a slug");
            let round = crate::secretary::store::Stored::of(
                &conversation,
                &[
                    crate::secretary::store::Entry::Opened {
                        at: crate::capability::Datetime::new(2026, 9, 15, 9, 0, 0, 0).expect("an instant"),
                        title: crate::secretary::words::Words::kept("Morning".to_owned()),
                        model: "a-cli".to_owned(),
                        kind: crate::secretary::store::Kind::Topic,
                    },
                    crate::secretary::store::Entry::Said {
                        at: crate::capability::Datetime::new(2026, 9, 15, 9, 2, 0, 0).expect("an instant"),
                        turn: crate::capability::Turn::Model {
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
                crate::secretary::answer_push(&round, brief),
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
