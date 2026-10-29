use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::Refusal;
use crate::secretary::memory::FactId;
use crate::secretary::verbs::clean_round;
use crate::secretary::{self, Call, SecretarySurface};
use crate::verb::Verb;

/// Forget a fact, only where remember would write.
pub struct Forget;

#[derive(Deserialize, JsonSchema)]
pub struct ForgetArgs {
    id: usize,
}

impl Verb for Forget {
    type Surface = SecretarySurface;
    type Args = ForgetArgs;
    type Answer = Value;
    const NAME: &'static str = "forget";
    const DESCRIPTION: &'static str = "FORGET A FACT by its number: its words are deleted from the memory. Only \
        in a round the owner opened that has read nothing but the manual, the \
        forecast and the memory; anywhere else it refuses, and they can tap \
        Forget on the memory page themselves.";

    fn run(call: &Call<'_>, args: ForgetArgs) -> Result<Value, Refusal> {
        clean_round(call, "they can tap Forget on the memory page")?;
        let forgotten = call.caps.memories.forget(
            FactId(u64::try_from(args.id).unwrap_or(u64::MAX)),
            call.caps.clock.now(),
            &secretary::writes_as(),
        )?;
        Ok(json!({ "forgotten": forgotten.id.0 }))
    }
}
