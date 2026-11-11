use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::repositories::readable;
use crate::secretary::verbs::{located, path_of, refused_by};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// One text file of a repository.
pub struct RepoRead;

#[derive(Deserialize, JsonSchema)]
pub struct RepoReadArgs {
    repo: String,
    #[schemars(required)]
    path: Option<String>,
    at: Option<String>,
}

impl Verb for RepoRead {
    type Surface = SecretarySurface;
    type Args = RepoReadArgs;
    type Answer = Value;
    const NAME: &'static str = "repo_read";
    const DESCRIPTION: &'static str = "One text file of `repo` at `at` (a branch, a tag or a commit; omit for the default branch's tip), \
        whole up to 100 KB. The answer names the commit it read: a change you \
        propose is written against exactly that commit.";

    fn run(call: &Call<'_>, args: RepoReadArgs) -> Result<Value, Refusal> {
        let (repo, at) = located(call.caps, &args.repo, args.at.as_deref())?;
        let path = path_of(args.path)?;
        let text = call
            .caps
            .repositories
            .blob(&repo, &at, &path)
            .and_then(|bytes| readable(&path, &bytes))
            .map_err(|failed| refused_by(&failed))?;
        Ok(json!({
            "repo": repo.to_string(),
            "at": at.as_str(),
            "path": path.as_str(),
            "text": text,
        }))
    }
}
