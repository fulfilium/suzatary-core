use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use crate::refusal::Refusal;
use crate::secretary;
use crate::secretary::gate::Gate;
use crate::secretary::memory::{self, Fact, Replaces};
use crate::secretary::surface::Binding;
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verbs::{IfCarded, memory_card};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::Verb;

/// Remember a fact for every later conversation.
pub struct Remember;

#[derive(Deserialize, JsonSchema)]
pub struct RememberArgs {
    fact: String,
    kind: String,
    #[serde(flatten)]
    carded: IfCarded,
}

impl Verb for Remember {
    type Surface = SecretarySurface;
    type Args = RememberArgs;
    type Answer = VerbAnswer;
    const NAME: &'static str = "remember";
    const DESCRIPTION: &'static str = "REMEMBER A FACT for every later conversation: something about the owner, \
        a person, a preference of theirs, or a standing instruction they gave. One \
        line, at most 280 characters, no URL. `kind` is `owner`, `person`, \
        `preference` or `standing`. What you remember reaches every conversation \
        as background, and the owner sees it at once with a Forget beside it.\n\n\
        ⚠️ CALL IT FIRST, before you read anything. In a round that has already \
        read mail, a todo, an attachment, a document, the web, a repository or \
        an old answer (`recall`), it does not write: it stores a card the owner taps, \
        and then `why` and `sources` are required as for any proposal. A round the \
        box opened (a brief, a review, a sweep) remembers nothing at all. At 200 \
        facts it refuses: forget one, or `consolidate` several into one, first.";

    fn run(call: &Call<'_>, args: RememberArgs) -> Result<VerbAnswer, Refusal> {
        let fact = Fact::new(&args.fact)
            .map_err(|failed| Refusal::bad_request(format!("fact: {failed}")))?;
        let kind = memory::kind_of(&args.kind)?;
        let said = match Gate::of(
            Binding::Remembers,
            &secretary::read(call.caps.conversations, call.conversation)?,
        ) {
            Gate::Refused(why) => return Err(Refusal::forbidden(why)),
            Gate::Binds(said) => said,
            Gate::Card => {
                let index = memory_card(
                    call.caps,
                    call.conversation,
                    &args.carded,
                    (&fact, kind, Replaces::Nothing),
                )?;
                // The card names the words and the memory's text store keeps them; the answer, a
                // conversation line nothing erases, holds neither.
                return Ok(VerbAnswer::from(json!({
                    "proposed": index,
                    "kind": kind.as_str(),
                    "next": "This round read something that is not the owner's own words, so the \
                             fact is a card they tap, not a memory yet. Say so in one clause.",
                })));
            }
        };
        let remembered = call
            .caps
            .memories
            .remember(&fact, kind, &said, call.caps.clock.now())?;
        // The id, never the words: an answer is a conversation line that outlives a forget. The
        // id rides on the turn's shape too, as its receipt, which reads the words from the memory
        // by it and still reads once the answer's words are erased.
        Ok(VerbAnswer {
            remembered: Some(remembered.id),
            ..VerbAnswer::from(json!({
                "remembered": remembered.id.0,
                "kind": kind.as_str(),
                "next": "The owner sees it under this answer with a Forget beside it.",
            }))
        })
    }
}
