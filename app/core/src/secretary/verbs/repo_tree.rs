use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::secretary::verbs::{located, path_of, refused_by};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// The entries under a directory of a repository.
pub struct RepoTree;

#[derive(Deserialize, JsonSchema)]
pub struct RepoTreeArgs {
    repo: String,
    path: Option<String>,
    at: Option<String>,
}

impl Verb for RepoTree {
    type Surface = SecretarySurface;
    type Args = RepoTreeArgs;
    type Answer = Value;
    const NAME: &'static str = "repo_tree";
    const DESCRIPTION: &'static str = "The files and directories under `path` (\"\" for the top) in `repo`, \
        at `at` (a branch, a tag or a commit; omit it for the default branch's tip, which \
        the answer names). Directories end in `/`.";

    fn run(call: &Call<'_>, args: RepoTreeArgs) -> Result<Value, Refusal> {
        let (repo, at) = located(call.caps, &args.repo, args.at.as_deref())?;
        let dir = path_of(args.path)?;
        let entries = call
            .caps
            .repositories
            .tree(&repo, &at, &dir)
            .map_err(|failed| refused_by(&failed))?;
        Ok(json!({ "repo": repo.to_string(), "at": at.as_str(), "entries": entries }))
    }
}
