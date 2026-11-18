//! The tap's rule: a proposal the inbox reads as standing is performed when the owner taps it,
//! under the inbox's `Decision`, and its response recorded as an object of the inbox.
//!
//! The one rule of the box whose effect is outward (a send, a sign-in, a write the box makes on
//! his say), so its witness is a [`Tap`], which only the gate mints: an engine holds no such
//! rule, and the box can only PROPOSE what this rule performs.

use prodrome_core::literal::Datetime;

use crate::secretary::inbox::{self, Object, ProposalId, Response};
use crate::secretary::inbox_event::InboxEvent;
use crate::secretary::inbox_reading::Reading;
use crate::secretary::responded::{Performed, Responded};
use suzatary_base::refusal::Refusal;
use suzatary_signatures::signatures::Tap;
use suzatary_store::store::Prodrome;
use suzatary_triage::rule::{Named, ReadingName, Rule};

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
