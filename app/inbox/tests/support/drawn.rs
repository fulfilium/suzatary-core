//! A proposal, and every kind of response to one, for the inbox schema's laws in any crate's
//! tests.

use proptest::prelude::*;

use crate::secretary::inbox::{self, Asked};
use crate::secretary::proposal::{Draft, Proposal, Reasons};
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::{Done, Responded, Tried, Withdrawal};

/// An email the secretary proposes, serving a todo.
///
/// # Panics
/// Never: every literal is one its parser takes.
#[must_use]
pub fn asked() -> Asked {
    inbox::Asked {
        proposer: Proposer::Secretary,
        proposal: Proposal::Email(Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["mira@example.com".to_owned()],
            cc: Vec::new(),
            subject: "Thursday".to_owned(),
            body: "Thursday works.".to_owned(),
            attachments: Vec::new(),
        }),
        reasons: Reasons::new(
            "she asked",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: "proposal:default/0".parse().expect("a ref"),
        price: inbox::Price::Serves("todo:life/reply".parse().expect("a ref")),
    }
}

/// A response of each kind, at a small attempt.
pub fn a_response() -> impl Strategy<Value = Responded> {
    let reason = "[a-z ]{0,8}";
    prop_oneof![
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::NotSent { reason }
        }),
        (1..4_u32).prop_map(|attempt| Responded::Tried {
            attempt,
            tried: Tried::Lacks(suzatary_mail::mail::lacking::Lacking::GoogleClient),
        }),
        (1..4_u32).prop_map(|attempt| Responded::Tried {
            attempt,
            tried: Tried::Running {
                began: "2026-10-01T09:00:00".to_owned(),
                expires_at: "2026-10-01T09:15:00".to_owned(),
            }
        }),
        "[a-z]{1,4}".prop_map(|message| Responded::Done(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: format!("<{message}@box>"),
        })),
        Just(Responded::Done(Done::Applied {
            objects: vec!["a".repeat(64)],
            chain: "life".to_owned(),
        })),
        "[a-z ]{0,8}".prop_map(|what| Responded::Done(Done::Acted { what })),
        reason.prop_map(|reason| Responded::Done(Done::MayHaveGone { reason })),
        Just(Responded::Checked(())),
        reason.prop_map(|reason| Responded::Dismissed { reason }),
        Just(Responded::Withdrawn(Withdrawal::Superseded)),
        reason.prop_map(|why| Responded::Withdrawn(Withdrawal::NoLongerNeeded { why })),
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::Refused { reason }
        }),
        (1..4_u32, reason).prop_map(|(attempt, reason)| Responded::Tried {
            attempt,
            tried: Tried::Unfinished {
                reason,
                link: "https://github.com/owner/name/pulls".to_owned(),
            }
        }),
        reason.prop_map(|what| Responded::Withdrawn(Withdrawal::DoneElsewhere { what })),
    ]
}
