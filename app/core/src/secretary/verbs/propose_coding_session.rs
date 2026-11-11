use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::coding::Task;
use crate::secretary::acts::Act;
use crate::secretary::afresh::Request;
use crate::secretary::filed::Filing;
use crate::secretary::proposal::{Proposal, Reasons};
use crate::secretary::proposer::Proposer;
use crate::secretary::store::Stored;
use crate::secretary::verbs::FilingArgs;
use crate::secretary::{self, Call, SecretarySurface, taint};
use crate::verb::Verb;
use suzatary_base::reference::{ProposalRef, Reference};
use suzatary_base::refusal::Refusal;

/// Why a coding session is refused in a round that read anything but the owner's own words.
const READ_OUTSIDE: &str = "this round has read something that is not the owner's own words \
                            (mail, a todo, a document, the web, a repository or an old \
                            answer), and a coding session's task is the whole of what an agent \
                            with the repository is told, so it is proposed only in a round that \
                            has read nothing";

/// Propose a cloud coding session, which a tap starts.
pub struct ProposeCodingSession;

#[derive(Deserialize, JsonSchema)]
pub struct ProposeCodingSessionArgs {
    task: String,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl Verb for ProposeCodingSession {
    type Surface = SecretarySurface;
    type Args = ProposeCodingSessionArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_coding_session";
    const DESCRIPTION: &'static str = "PROPOSE A CODING SESSION for a change to this app too big for \
        `propose_patch`: one that needs a shell, a build and the gates. `task` \
        is plain text, at most 4 KB, saying what to change and why; it is the \
        WHOLE of what the session is told beside its fixed brief, so name the \
        todo, the files and what done looks like. The owner's tap starts it in \
        the cloud, spending their account's usage; it pushes a claude/ branch with \
        a report, which comes back here as a card, and nothing is merged \
        without their main session. Never put anyone's personal details, \
        identity documents, money, health or legal matters into a task. `why` \
        and `sources` are required as for every proposal; `serves` and \
        `revises` as for `propose_email`.\n\n\
        ⚠️ CALL IT FIRST, before you read anything. In a round that has already \
        read mail, a todo, an attachment, a document, the web, a repository or \
        an old answer, it is refused, and the box files a card that asks the \
        owner's request again in a fresh conversation: tell the owner why, and \
        point at that card.";

    fn run(call: &Call<'_>, args: ProposeCodingSessionArgs) -> Result<Value, Refusal> {
        // A session takes the task as its whole instruction and spends the account, so a task an
        // email could have worded is never put on a card at all, however clearly it reads.
        let stored = secretary::read(call.caps.conversations, call.conversation)?;
        if taint::tainted(&stored) {
            return Err(afresh(call, &stored));
        }
        let reasons = args.filing.reasons()?;
        let task =
            Task::try_from(args.task).map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let act = Act::CodingSession { task };
        let index = args
            .filing
            .file(call, Proposal::Act(act.clone()), &reasons)?;
        Ok(json!({
            "proposed": index,
            "sentence": act.sentence(),
            "next": "The owner reads the whole task on a card. Their tap starts the session; its \
                     branch and report come back to this conversation as a card of their own. \
                     You cannot start one and there is no verb that can.",
        }))
    }
}

/// The refusal of a coding session in a round that read more than the owner's words. The box
/// files the card that asks his request again in a fresh conversation, as its own proposal, so
/// the model chooses neither the words that cross nor the card's reasons.
fn afresh(call: &Call<'_>, stored: &Stored) -> Refusal {
    let Some(request) = Request::in_round(stored) else {
        return Refusal::forbidden(format!(
            "{READ_OUTSIDE}, and no turn of this round is the owner's own words alone: say so, \
             and the owner asks again in a conversation of its own"
        ));
    };
    match filed(call, stored, &request) {
        Ok(index) => Refusal::forbidden(format!(
            "{READ_OUTSIDE}. The box filed card {index}, which starts the owner's request again \
             in a fresh conversation that holds only their words: say so, and point at that card"
        )),
        Err(refused) => Refusal::forbidden(format!(
            "{READ_OUTSIDE}, and the card that asks again afresh was not filed ({refused}): say \
             so, and the owner asks again in a conversation of its own"
        )),
    }
}

/// The card that asks `request` again, filed by the box in `stored`'s conversation; the card
/// itself is the evidence it rests on.
fn filed(call: &Call<'_>, stored: &Stored, request: &Request) -> Result<usize, Refusal> {
    let card = ProposalRef {
        conversation: stored.id.clone(),
        index: stored.proposals.len(),
    };
    let reasons = Reasons::new(
        format!(
            "a coding session may be proposed only where nothing but your words was read, and \
             this conversation read more after turn {}",
            request.turn()
        ),
        vec![Reference::Proposal(card)],
    )
    .map_err(|failed| Refusal::internal(failed.to_string()))?;
    let (filed, _) = crate::secretary::filed::file(
        call.caps.conversations,
        call.caps.inbox,
        call.chains,
        call.caps.clock.now(),
        Filing {
            conversation: call.conversation,
            proposer: Proposer::Box,
            proposal: Proposal::Act(request.act(call.conversation)),
            reasons,
            serves: None,
            revises: None,
        },
    )?;
    Ok(filed.index())
}
