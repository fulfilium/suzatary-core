//! The tap's rule: a proposal the inbox reads as standing is performed when the owner taps it,
//! under the inbox's `Decision`, and its response recorded as an object of the inbox.
//!
//! The one rule of the box whose effect is outward (a send, a sign-in, a write the box makes on
//! his say), so its witness is a [`Tap`], which only the gate mints: an engine holds no such
//! rule, and the box can only PROPOSE what this rule performs.

use prodrome_core::literal::Datetime;

use crate::refusal::Refusal;
use crate::rule::{Named, ReadingName, Rule};
use crate::secretary::inbox::{self, Object, ProposalId, Response};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::inbox_reading::Reading;
use crate::secretary::responded::{Performed, Responded};
use crate::secretary::tap::Tap;
use crate::store::Prodrome;

/// The tap's rule at the instant of the tap.
#[derive(Debug, Clone, Copy)]
pub struct Tapped {
    /// When it is asked; an act's code past its expiry stands again.
    pub now: Datetime,
}

/// A proposal is named by its id and its state register's maximal writes.
impl Named for Object {
    fn name(&self) -> ReadingName {
        ReadingName::of(
            std::iter::once(self.id.as_str()).chain(
                self.reading
                    .answers()
                    .into_iter()
                    .map(|answer| answer.name.as_str()),
            ),
        )
    }
}

impl Rule for Tapped {
    type Key = ProposalId;
    type Reading = Object;
    type Witness = Tap;

    fn name(&self) -> &'static str {
        "tap"
    }

    /// While nothing answered it, its last try sent nothing, or its act's code went stale:
    /// anything else already holds the response this rule would record, and a conflict holds
    /// two.
    fn holds(&self, object: &Object) -> bool {
        match &object.reading {
            Reading::Standing => true,
            Reading::One(answer) => match &answer.response {
                Responded::Tried { tried, .. } => {
                    answer.response.performs() || tried.stale(self.now)
                }
                Responded::Done(_)
                | Responded::Checked(_)
                | Responded::Dismissed { .. }
                | Responded::Withdrawn(_) => false,
            },
            Reading::Conflict(_) | Reading::Unreadable(_) => false,
        }
    }
}

impl Tapped {
    /// Fire on proposal `id`, presented with the owner's tap: under the inbox's hold the reading
    /// decides, `perform` runs only while the rule holds, and its outcome is recorded as `tap`'s
    /// before the hold is let go, so a second tap reads the first's response.
    ///
    /// # Errors
    /// As [`inbox::submit`].
    pub fn fire<C: Prodrome<InboxEvent> + ?Sized>(
        &self,
        inbox: &C,
        id: &ProposalId,
        tap: &Tap,
        perform: impl FnOnce(&inbox::Asked) -> Result<Performed, Refusal>,
    ) -> Result<Response, Refusal> {
        inbox::submit(inbox, id, tap.who(), self.now, perform)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation_id::ConversationId;
    use crate::gate::ActHeaders;
    use crate::reference::ProposalRef;
    use crate::secretary::store::{Draft, Proposal, Reasons};

    fn tap() -> Tap {
        Tap::of(
            &crate::signatures::scene("ada"),
            &ActHeaders {
                act: Some("1"),
                client: Some(crate::gate::CLIENT),
                content_type: Some("application/json"),
                site: Some("same-origin"),
            },
            crate::seat::Seat::Device,
        )
        .expect("a tap")
    }

    fn now() -> Datetime {
        Datetime::new(2026, 10, 5, 9, 0, 0, 0).expect("an instant")
    }

    /// The tap's rule holds on a standing proposal, performs once under the inbox's Decision, and
    /// its recorded response discharges it: a second tap, and the rule after a restart, perform
    /// nothing.
    #[test]
    fn a_tap_performs_once_and_its_response_discharges_the_rule() {
        let store = crate::store::InMemory::<InboxEvent>::default();
        let asked = inbox::Asked {
            proposer: crate::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Email(Draft::default()),
            reasons: Reasons::new(
                "He asked for the scan.",
                vec!["mail:a1b2c3d4e5f6".parse().expect("a reference")],
            )
            .expect("reasons"),
            asked: ProposalRef {
                conversation: ConversationId::new("default").expect("a slug"),
                index: 0,
            },
            price: inbox::Price::unpriced(),
        };
        let id = inbox::ask(&store, asked, &crate::secretary::writes_as(), now()).expect("filed");
        let rule = Tapped { now: now() };
        let standing = |store: &crate::store::InMemory<InboxEvent>| {
            inbox::fold(store)
                .expect("reads")
                .get(&id)
                .expect("held")
                .clone()
        };
        assert!(rule.holds(&standing(&store)));

        let tap = tap();
        let mut performed = 0;
        let mut send = |_: &inbox::Asked| {
            performed += 1;
            Ok(Performed::Done(crate::secretary::responded::Done::Sent {
                recipients: vec!["a@b.test".to_owned()],
                message_id: "<x@box.example>".to_owned(),
            }))
        };
        assert!(matches!(
            rule.fire(&store, &id, &tap, &mut send).expect("taps"),
            Response::Ran(_)
        ));
        assert!(matches!(
            rule.fire(&store, &id, &tap, &mut send).expect("taps"),
            Response::Recorded(_)
        ));
        assert_eq!(performed, 1);
        assert!(!rule.holds(&standing(&store)));
        assert!(!Tapped { now: now() }.holds(&standing(&store)));
    }
}
