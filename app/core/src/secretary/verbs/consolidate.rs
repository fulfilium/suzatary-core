use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::refusal::Refusal;
use crate::secretary::memory::{self, Fact, FactId};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verbs::clean_round;
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Replace standing facts with one, only where forget would write.
pub struct Consolidate;

#[derive(Deserialize, JsonSchema)]
pub struct ConsolidateArgs {
    from: Vec<u64>,
    fact: String,
    kind: String,
}

impl Verb for Consolidate {
    type Surface = SecretarySurface;
    type Args = ConsolidateArgs;
    type Answer = VerbAnswer;
    const NAME: &'static str = "consolidate";
    const DESCRIPTION: &'static str = "CONSOLIDATE FACTS: replace the standing facts numbered \
        `from` with one fact that says what they said together, in your words, shortest true \
        form. One line, at most 280 characters, no URL; `kind` as for `remember`. The facts it \
        replaces leave the memory and their words are deleted; the new fact stands on where \
        each of them was said. Use it when two facts say one thing, or one corrects another, \
        or the memory is at its cap. Only in a round the owner opened that has read nothing \
        but the manual, the forecast and the memory; anywhere else it refuses.";

    fn run(call: &Call<'_>, args: ConsolidateArgs) -> Result<VerbAnswer, Refusal> {
        let from: BTreeSet<FactId> = args.from.into_iter().map(FactId).collect();
        if from.is_empty() {
            return Err(Refusal::bad_request(
                "from: name at least one standing fact by its number",
            ));
        }
        let fact = Fact::new(&args.fact)
            .map_err(|failed| Refusal::bad_request(format!("fact: {failed}")))?;
        let kind = memory::kind_of(&args.kind)?;
        let round = clean_round(call, "ask for it again in a round they open")?;
        let merged =
            call.caps
                .memories
                .consolidate(&from, &fact, kind, &round, call.caps.clock.now())?;
        // The ids, never the words, as `remember` answers: the receipt reads them by the id.
        Ok(VerbAnswer {
            remembered: Some(merged.id),
            ..VerbAnswer::from(json!({
                "remembered": merged.id.0,
                "replaced": from.iter().map(|id| id.0).collect::<Vec<_>>(),
                "kind": kind.as_str(),
                "next": "The owner sees the new fact under this answer with a Forget beside it.",
            }))
        })
    }
}
