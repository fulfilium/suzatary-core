use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::responded::Withdrawal;
use crate::secretary::verbs::withdrawable;
use crate::secretary::{self, Call, SecretarySurface, inbox};
use crate::verb::Verb;

/// Withdraw a stale proposal that nothing replaces.
pub struct WithdrawProposal;

#[derive(Deserialize, JsonSchema)]
pub struct WithdrawProposalArgs {
    index: usize,
    why: String,
}

impl Verb for WithdrawProposal {
    type Surface = SecretarySurface;
    type Args = WithdrawProposalArgs;
    type Answer = Value;
    const NAME: &'static str = "withdraw_proposal";
    const DESCRIPTION: &'static str = "WITHDRAW a proposal that went stale with no replacement — nothing else \
        is proposing to take its place, and the thing it was for is no longer \
        true. It answers the proposal, so it stops counting toward the \
        Inbox.\n\n\
        Use `revises` on a fresh propose_* call instead when something replaces \
        it; use this only when nothing does. `index` names the proposal on THIS \
        conversation and `why` is REQUIRED — it is the whole of what the next \
        round, and the owner if they look, reads back.";

    fn run(call: &Call<'_>, args: WithdrawProposalArgs) -> Result<Value, Refusal> {
        let why = args.why.trim().to_owned();
        if why.is_empty() {
            return Err(Refusal::bad_request(
                "say why: the reason is the only part of a withdrawal the next answer can use",
            ));
        }
        let standing = withdrawable(
            call.caps,
            &secretary::read(call.caps.conversations, call.conversation)?,
            args.index,
        )?;
        inbox::withdraw(
            call.caps.inbox,
            &standing.object.id,
            &crate::secretary::transcript::writer(),
            call.caps.clock.now(),
            Withdrawal::NoLongerNeeded { why: why.clone() },
        )?;
        call.caps.memories.sweep_after_write();
        Ok(json!({
            "withdrawn": args.index,
            "why": why,
            "next": "It no longer stands and no longer counts toward the Inbox.",
        }))
    }
}
