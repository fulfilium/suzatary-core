use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::reference::ProposalRef;
use crate::refusal::{Refusal, shown};
use crate::secretary::inbox;
use crate::secretary::proposer::Proposer;
use crate::secretary::responded::Withdrawal;
use crate::secretary::store::{Decision, Entry, Proposal, Settle};
use crate::secretary::{Call, SecretarySurface, transcript, writes_as};
use crate::verb::Verb;

/// Withdraw one of the triage agent's cards, with the reason recorded.
pub struct TriageWithdraw;

#[derive(Deserialize, JsonSchema)]
pub struct TriageWithdrawArgs {
    card: String,
    why: String,
}

impl Verb for TriageWithdraw {
    type Surface = SecretarySurface;
    type Args = TriageWithdrawArgs;
    type Answer = Value;
    const NAME: &'static str = "triage_withdraw";
    const DESCRIPTION: &'static str = "WITHDRAW one of the triage agent's open cards, so it leaves \
        the owner's Inbox unperformed. `card` is its reference as `triage_cards` prints it \
        (`proposal:triage/<n>`); `why` is REQUIRED and recorded.\n\n\
        Withdraw a DUPLICATE (a card that restates a todo already open, or another card), \
        noise, and a reprice with no new fact. A withdrawal is final for that request: the agent \
        cannot file it again at another price on the same sources, so when in doubt, leave it. A card the owner has \
        tapped, even one whose tap failed, is his and cannot be withdrawn. You cannot perform a \
        card; only the owner's tap does.";

    fn run(call: &Call<'_>, args: TriageWithdrawArgs) -> Result<Value, Refusal> {
        let why = args.why.trim().to_owned();
        if why.is_empty() {
            return Err(Refusal::bad_request(
                "say why: the reason is the whole of what the owner reads back",
            ));
        }
        let card: ProposalRef = args
            .card
            .trim()
            .parse()
            .map_err(|failed| Refusal::bad_request(format!("card: {failed}")))?;
        let caps = call.caps;
        let fold = inbox::fold(caps.inbox)?;
        let not_triage = || {
            Refusal::not_found(format!(
                "{} is no card of the triage agent's that nobody has answered",
                shown(&args.card)
            ))
        };
        let object = fold
            .objects()
            .find(|object| object.asked.asked == card)
            .filter(|object| {
                object.asked.proposer == Proposer::Triage
                    && matches!(
                        object.reading,
                        crate::secretary::inbox_reading::Reading::Standing
                    )
            })
            .ok_or_else(not_triage)?;
        let chain = match &object.asked.proposal {
            Proposal::Changes(changeset) => changeset.chain().to_owned(),
            Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Act(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => return Err(not_triage()),
        };
        inbox::withdraw_unanswered(
            caps.inbox,
            &object.id,
            &writes_as(),
            caps.clock.now(),
            Withdrawal::NoLongerNeeded { why: why.clone() },
        )?;
        let reviewed = card.to_string();
        transcript::push(
            caps.conversations,
            call.conversation,
            &Entry::Reviewed {
                at: caps.clock.now(),
                chain,
                batch: reviewed.clone(),
                decision: Decision::Settled {
                    how: Settle::Rejected,
                    objects: crate::nonempty::NonEmpty::of(vec![reviewed.clone()])
                        .ok_or_else(|| Refusal::internal("one card"))?,
                },
                why: crate::secretary::words::Words::kept(why.clone()),
            },
        )?;
        Ok(json!({
            "withdrawn": reviewed,
            "why": why,
            "next": "It no longer stands and leaves the owner's Inbox.",
        }))
    }
}
