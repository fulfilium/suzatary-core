use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::secretary::verbs::{located, refused_by};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// The lines of a repository containing some text.
pub struct RepoGrep;

#[derive(Deserialize, JsonSchema)]
pub struct RepoGrepArgs {
    repo: String,
    text: String,
    at: Option<String>,
}

impl Verb for RepoGrep {
    type Surface = SecretarySurface;
    type Args = RepoGrepArgs;
    type Answer = Value;
    const NAME: &'static str = "repo_grep";
    const DESCRIPTION: &'static str = "Every line of `repo` containing `text` (plain text, case-insensitive) \
        at `at` (omit for the default branch's tip), as `path:line:text`.";

    fn run(call: &Call<'_>, args: RepoGrepArgs) -> Result<Value, Refusal> {
        let (repo, at) = located(call.caps, &args.repo, args.at.as_deref())?;
        let needle = args.text.as_str();
        if needle.trim().is_empty() || needle.len() > 200 {
            return Err(Refusal::bad_request(
                "text is 1 to 200 characters to look for",
            ));
        }
        let lines = call
            .caps
            .repositories
            .grep(&repo, &at, needle)
            .map_err(|failed| refused_by(&failed))?;
        Ok(json!({ "repo": repo.to_string(), "at": at.as_str(), "lines": lines }))
    }
}
