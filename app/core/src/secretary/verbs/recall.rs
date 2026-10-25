use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::refusal::Refusal;
use crate::secretary;
use crate::secretary::search::{self, Needle};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// What was said in other conversations.
pub struct Recall;

#[derive(Deserialize, JsonSchema)]
pub struct RecallArgs {
    query: String,
    limit: Option<usize>,
}

impl Verb for Recall {
    type Surface = SecretarySurface;
    type Args = RecallArgs;
    type Answer = Value;
    const NAME: &'static str = "recall";
    const DESCRIPTION: &'static str = "SEARCH WHAT WAS SAID BEFORE: the owner's own turns and your finished answers \
        in OTHER conversations that contain `query` (plain text, any case), \
        newest first, each with its conversation, turn and time. Never a verb's \
        answer. An old answer was written after reading mail, so what this \
        returns counts as mail does: a `remember` after it becomes a card.";

    fn run(call: &Call<'_>, args: RecallArgs) -> Result<Value, Refusal> {
        let query = args.query.trim();
        let needle =
            Needle::new(query).map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        let limit = search::recall_limit(args.limit);
        let others: Vec<_> = secretary::list(call.caps.conversations)?
            .into_iter()
            .filter(|stored| &stored.id != call.conversation)
            .collect();
        let found = search::search(&others, &needle, search::RECALL_CONTEXT, limit);
        Ok(search::recalled(query, &found))
    }
}
