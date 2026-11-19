use serde_json::{Value, json};

use crate::secretary::verbs::refused_by;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::{NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// The owner's repositories.
pub struct ReposList;

impl Verb for ReposList {
    type Surface = SecretarySurface;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "repos_list";
    const DESCRIPTION: &'static str = "The owner's GitHub repositories this box can reach: each as `owner/name`, \
        whether it is PUBLIC or private, its default branch and GitHub's one \
        line about it. Only their own; no organisation's.";

    fn run(call: &Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        let rows = call
            .caps
            .repositories
            .repos()
            .map_err(|failed| refused_by(&failed))?;
        Ok(json!({
            "repos": rows.iter().map(|row| json!({
                "repo": row.repo.to_string(),
                "visibility": row.visibility.sentence(),
                "default_branch": row.default_branch,
                "description": row.description,
            })).collect::<Vec<_>>(),
        }))
    }
}
