use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::capability::Secretary;
use crate::refusal::Refusal;
use crate::secretary;
use crate::secretary::memory::{self, Fact, Kind};
use crate::secretary::store::{ConversationId, Proposal, Reasons};
use crate::secretary::verb_answer::VerbAnswer;
use crate::secretary::verbs::{Gate, NO_OPENER, gate, propose, references};
use crate::secretary::{Call, SecretarySurface};
use crate::verb::{Caught, Verb};

/// Remember a fact for every later conversation.
pub struct Remember;

#[derive(Deserialize, JsonSchema)]
pub struct RememberArgs {
    fact: String,
    kind: String,
    #[serde(default)]
    why: Caught<Option<String>>,
    #[serde(default)]
    sources: Caught<Option<String>>,
}

/// `remember` in a tainted round: the fact as a card, with the reasons only a card reads.
fn card(
    caps: &Secretary<'_>,
    id: &ConversationId,
    args: &RememberArgs,
    fact: &Fact,
    kind: Kind,
) -> Result<Value, Refusal> {
    let reasons = Reasons::new(
        args.why.read()?.as_deref().unwrap_or_default(),
        references(
            args.sources.read()?.as_deref().unwrap_or_default(),
            "sources",
        )?,
    )
    .map_err(|failed| {
        Refusal::bad_request(format!(
            "this round has read something that is not the owner's own words, so the fact \
             becomes a card they tap, and a card needs its reasons: {failed}"
        ))
    })?;
    // Under the memory's hold, so no sweep decides about these words between keeping them and
    // the card that names them being filed.
    let held = caps.memories.hold()?;
    let kept = held.keep_card(fact)?;
    let proposal = Proposal::Remember {
        text: kept.name().clone(),
        kind,
    };
    let index = match propose(caps, &[], id, proposal, &reasons, None, None) {
        Ok(index) => index,
        Err(refused) => {
            held.unkeep(&kept)?;
            return Err(refused);
        }
    };
    // The card names the words and the memory's text store keeps them; the answer, a
    // conversation line nothing erases, holds neither.
    Ok(json!({
        "proposed": index,
        "kind": kind.as_str(),
        "next": "This round read something that is not the owner's own words, so the fact is \
                 a card they tap, not a memory yet. Say so in one clause.",
    }))
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
        let said = match gate(&secretary::read(
            call.caps.conversations,
            call.conversation,
        )?) {
            Gate::Closed => return Err(Refusal::forbidden(NO_OPENER)),
            Gate::Write(said) => said,
            Gate::Card => {
                return card(call.caps, call.conversation, &args, &fact, kind)
                    .map(VerbAnswer::from);
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
