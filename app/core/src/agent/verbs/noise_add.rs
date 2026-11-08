use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::agent::{Call, TriageSurface, refused};
use crate::reference::Reference;
use crate::refusal::{Refusal, shown};
use crate::secretary::acts::Act;
use crate::secretary::proposal::Reasons;
use crate::verb::Verb;

/// Ask the owner to leave a queued message's sender out of triage.
pub struct NoiseAdd;

#[derive(Deserialize, JsonSchema)]
pub struct NoiseAddArgs {
    hash: String,
    why: String,
}

impl Verb for NoiseAdd {
    type Surface = TriageSurface;
    type Args = NoiseAddArgs;
    type Answer = Value;
    const NAME: &'static str = "noise_add";
    const DESCRIPTION: &'static str = "Propose leaving every later message from one sender out of \
        triage. `hash` is the canonical hash of a message `new` lists now; its sender is the one \
        proposed, never an address you name. Nothing changes until the owner taps the card this \
        files. `why` is REQUIRED: one sentence the owner reads first.";

    fn run(call: &Call<'_>, args: NoiseAddArgs) -> Result<Value, Refusal> {
        let queued = call.caps.corpus.queue(None).map_err(|e| refused(&e))?;
        let row = queued
            .iter()
            .find(|row| row.hash == args.hash)
            .ok_or_else(|| {
                Refusal::not_found(format!(
                    "no message in the triage queue has the hash {}: noise_add takes the hash \
                     of a message `new` lists",
                    shown(&args.hash)
                ))
            })?;
        let source = format!("mail:{}", row.hash)
            .parse::<Reference>()
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let reasons = Reasons::new(args.why, vec![source])
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        let card = call.cards.file_act(
            call.caps.clock.now(),
            Act::Noise {
                sender: row.sender.clone(),
                message: row.hash.clone(),
            },
            reasons,
        )?;
        Ok(json!({
            "filed": card.to_string(),
            "sender": row.sender,
            "next": "Filed as a card in the owner's Inbox. The sender is still triaged until he \
                     taps it; mark this message as you would any other.",
        }))
    }
}
