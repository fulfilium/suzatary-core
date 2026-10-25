use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::agent::{Call, TriageSurface};
use crate::refusal::Refusal;
use crate::secretary::search::{self, Needle};
use crate::verb::Verb;

/// The secretary's facts, and the owner's own lines, that hold some words.
pub struct Recall;

#[derive(Deserialize, JsonSchema)]
pub struct RecallArgs {
    query: String,
    limit: Option<usize>,
}

impl Verb for Recall {
    type Surface = TriageSurface;
    type Args = RecallArgs;
    type Answer = Value;
    const NAME: &'static str = "recall";
    const DESCRIPTION: &'static str = "SEARCH THE SECRETARY'S MEMORY, read-only: the facts \
        it keeps that contain `query` (plain text, any case), and the owner's own lines in its \
        record that contain it, or a summary of nothing but those, newest first, each with its \
        handle for `zoom`. Never the secretary's answers, a verb's answer or the box's notes. \
        Use it before filing mail about a person, a place or a matter the owner may have \
        talked about.";

    fn run(call: &Call<'_>, args: RecallArgs) -> Result<Value, Refusal> {
        let query = args.query.trim();
        let needle =
            Needle::new(query).map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        let memory = &call.caps.memory;
        let limit = search::recall_limit(args.limit);
        let mut found = memory.facts()?.facts;
        found.retain(|fact| needle.found_in(fact.fact.as_str()));
        found.sort_by_key(|fact| std::cmp::Reverse(fact.id));
        let facts: Vec<Value> = found
            .iter()
            .take(limit)
            .map(|fact| {
                json!({
                    "id": fact.id.0,
                    "kind": fact.kind.as_str(),
                    "fact": fact.fact.as_str(),
                })
            })
            .collect();
        let lines = memory.record()?.find(&needle, limit);
        Ok(json!({ "query": query, "facts": facts, "lines": lines }))
    }
}
