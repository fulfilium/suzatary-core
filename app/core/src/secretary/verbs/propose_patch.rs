use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::capability::RepoFailure;
use crate::refusal::Refusal;
use crate::repos::{MAX_SHOWN_BYTES, Patch, Visibility};
use crate::secretary::acts::Act;
use crate::secretary::gate::{Asked, Gate};
use crate::secretary::proposal::Proposal;
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
        diff of text files only, at most 64 KB, against `base`, the commit you read (omit \
        it for the default branch's tip); `message` is the commit message. The owner sees \
        the whole diff on a card, and their tap OPENS A PULL REQUEST of exactly this on a \
        branch of its own. After you read anything but the owner's words, a public \
        repository takes at most 4 KB of patch and message. A public branch is readable by \
        anyone once pushed, so NEVER put anything about their identity documents, money, \
        health, legal matters or anyone's contact details into one. `serves` and \
        `revises` as for `propose_email`.";

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
        let shown = patch.as_str().len() + message.len();
        let round =
            crate::secretary::conversations::read(call.caps.conversations, call.conversation)?;
        match (visibility, Gate::of(Asked::Publishes, &round)) {
            (Visibility::Public, Gate::Card) if shown > MAX_SHOWN_BYTES => {
                return Err(Refusal::bad_request(format!(
                    "after other words, a public patch and message are at most \
                     {MAX_SHOWN_BYTES} bytes; this is {shown}"
                )));
            }
            (Visibility::Public, Gate::Refused(why)) => return Err(Refusal::forbidden(why)),
            (Visibility::Private | Visibility::Public, _) => {}
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
        }))
    }
}
