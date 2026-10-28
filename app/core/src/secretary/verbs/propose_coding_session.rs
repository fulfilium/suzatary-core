use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::coding::Task;
use crate::refusal::Refusal;
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;
use crate::secretary::verbs::FilingArgs;
use crate::secretary::{self, Call, SecretarySurface, taint};
use crate::verb::Verb;

/// Why a coding session is refused in a round that read anything but the owner's own words.
const READ_OUTSIDE: &str = "this round has read something that is not the owner's own words \
                            (mail, a todo, a document, the web, a repository or an old \
                            answer), and a coding session's task is the whole of what an agent \
                            with the repository is told, so it is proposed only in a round that \
                            has read nothing: say so, and the owner asks again in a round of \
                            its own";

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
        an old answer, it is refused: tell the owner why, and they ask again in a \
        round of its own.";

    fn run(call: &Call<'_>, args: ProposeCodingSessionArgs) -> Result<Value, Refusal> {
        // A session takes the task as its whole instruction and spends the account, so a task an
        // email could have worded is never put on a card at all, however clearly it reads.
        if taint::tainted(&secretary::read(
            call.caps.conversations,
            call.conversation,
        )?) {
            return Err(Refusal::forbidden(READ_OUTSIDE));
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
