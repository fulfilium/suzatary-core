use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::capability::RepoFailure;
use crate::refusal::Refusal;
use crate::repos::Patch;
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;
use crate::secretary::verbs::{FilingArgs, located, refused_by};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Propose a change to a repository, which a tap opens as a pull request.
pub struct ProposePatch;

#[derive(Deserialize, JsonSchema)]
pub struct ProposePatchArgs {
    repo: String,
    base: Option<String>,
    patch: String,
    message: String,
    #[serde(flatten)]
    filing: FilingArgs,
}

impl Verb for ProposePatch {
    type Surface = SecretarySurface;
    type Args = ProposePatchArgs;
    type Answer = Value;
    const NAME: &'static str = "propose_patch";
    const DESCRIPTION: &'static str = "PROPOSE A CHANGE to one of the owner's repositories: `patch` is a unified \
        diff (`--- a/path`, `+++ b/path`, `@@` hunks) of text files only, at \
        most 64 KB, written against `base`, the commit you read (omit it for \
        the default branch's tip; a branch or tag names its commit). `message` \
        is the commit message, and its first line the pull request's title. It \
        must apply cleanly to `base` or it is refused before anything is \
        stored. The owner sees the whole diff on a card, and their tap OPENS A \
        PULL REQUEST: exactly this, committed on a branch of its own and \
        proposed for merging into the default branch, which they merge or \
        closes on GitHub. A public repository takes a patch only where the \
        owner's instance file says `public = true` for it, and its branch is \
        readable by anyone once pushed, so NEVER put anything \
        about their identity documents, money, health, legal matters or anyone's \
        contact details into one. `why` and `sources` are required as for \
        every proposal; `serves` and `revises` as for `propose_email`.";

    fn run(call: &Call<'_>, args: ProposePatchArgs) -> Result<Value, Refusal> {
        let reasons = args.filing.reasons()?;
        let (repo, base) = located(call.caps, &args.repo, args.base.as_deref())?;
        let visibility = call
            .caps
            .repositories
            .repos()
            .map_err(|failed| refused_by(&failed))?
            .into_iter()
            .find(|row| row.repo == repo)
            .map(|row| row.visibility)
            .ok_or_else(|| refused_by(&RepoFailure::Unknown(repo.to_string())))?;
        call.caps
            .repositories
            .pushable(&repo, visibility)
            .map_err(|failed| refused_by(&failed))?;
        let patch = Patch::try_from(args.patch)
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let message = args.message.trim().to_owned();
        if message.is_empty() || message.len() > 4000 {
            return Err(Refusal::bad_request(
                "message is the commit message: a first line, then a body if it needs one",
            ));
        }
        call.caps
            .repositories
            .applies(&repo, &base, &patch)
            .map_err(|failed| refused_by(&failed))?;
        let files = patch.files();
        let act = Act::Patch {
            repo: repo.clone(),
            visibility,
            base: base.clone(),
            patch,
            message,
        };
        let index = args.filing.file(call, Proposal::Act(act), &reasons)?;
        Ok(json!({
            "proposed": index,
            "repo": repo.to_string(),
            "visibility": visibility.sentence(),
            "base": base.as_str(),
            "files": files,
            "next": "The owner sees the whole diff and the commit message on a card. Their tap \
                     opens exactly this as a pull request on a branch of its own; nothing \
                     reaches the default branch until they merge it.",
        }))
    }
}
