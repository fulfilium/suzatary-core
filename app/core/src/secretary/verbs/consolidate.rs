use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::refusal::Refusal;
use crate::secretary::gate::Gate;
use crate::secretary::memory::{self, Fact, FactId, Replaces, Sources};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verb_name::Binding;
use crate::secretary::verbs::{IfCarded, memory_card};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Replace standing facts with one, where remember would write, or as a card the owner taps.
pub struct Consolidate;

#[derive(Deserialize, JsonSchema)]
pub struct ConsolidateArgs {
    from: Vec<u64>,
    fact: String,
    kind: String,
    #[serde(flatten)]
    carded: IfCarded,
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
        or the memory is at its cap. Where `remember` would write, this writes; in a round that \
        has read anything but the owner's own words it stores a card showing the old facts \
        beside the new one, which the owner taps, and then `why` (what makes them one) and \
        `sources` are required as for any proposal. A round the box opened refuses.";

    fn run(call: &Call<'_>, args: ConsolidateArgs) -> Result<VerbAnswer, Refusal> {
        let from = Sources::new(
            args.from
                .iter()
                .copied()
                .map(FactId)
                .collect::<BTreeSet<_>>(),
        )
        .ok_or_else(|| {
            Refusal::bad_request("from: name at least one standing fact by its number")
        })?;
        let fact = Fact::new(&args.fact)
            .map_err(|failed| Refusal::bad_request(format!("fact: {failed}")))?;
        let kind = memory::kind_of(&args.kind)?;
        let replaced: Vec<u64> = from.ids().iter().map(|id| id.0).collect();
        let round = match Gate::of(
            Binding::Remembers,
            &crate::secretary::conversations::read(call.caps.conversations, call.conversation)?,
        ) {
            Gate::Refused(why) => return Err(Refusal::forbidden(why)),
            Gate::Binds(round) => round,
            Gate::Card => {
                let index = memory_card(
                    call.caps,
                    call.conversation,
                    &args.carded,
                    (&fact, kind, Replaces::Facts(from)),
                )?;
                return Ok(VerbAnswer::from(json!({
                    "proposed": index,
                    "replaces": replaced,
                    "kind": kind.as_str(),
                    "next": "This round read something that is not the owner's own words, so the \
                             merge is a card they tap; the facts stand until then. Say so in one \
                             clause.",
                })));
            }
        };
        let merged = call.caps.memories.hold()?.write(
            &fact,
            kind,
            &Replaces::Facts(from),
            &round,
            call.caps.clock.now(),
        )?;
        // The ids, never the words, as `remember` answers: the receipt reads them by the id.
        Ok(VerbAnswer {
            remembered: Some(merged.id),
            ..VerbAnswer::from(json!({
                "remembered": merged.id.0,
                "replaced": replaced,
                "kind": kind.as_str(),
                "next": "The owner sees the new fact under this answer with a Forget beside it.",
            }))
        })
    }
}
