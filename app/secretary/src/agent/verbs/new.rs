use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Call, TriageSurface, answered, refused};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// The triage queue.
pub struct New;

#[derive(Deserialize, JsonSchema)]
pub struct NewArgs {
    limit: Option<usize>,
}

impl Verb for New {
    type Surface = TriageSurface;
    type Args = NewArgs;
    type Answer = Value;
    const NAME: &'static str = "new";
    const DESCRIPTION: &'static str = "The triage queue: every inbox message not yet seen (by canonical hash), \
        not noise. Read/unread is irrelevant — mail the owner already read on his \
        phone still gets triaged exactly once.";

    fn run(call: &Call<'_>, args: NewArgs) -> Result<Value, Refusal> {
        answered(
            &call
                .caps
                .corpus
                .queue(args.limit)
                .map_err(|e| refused(&e))?,
        )
    }
}
