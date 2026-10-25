use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::refusal::Refusal;
use crate::secretary::compaction::Reading;
use crate::secretary::{Call, SecretarySurface, told};
use crate::verb::Verb;

/// What lies beneath a part of the record.
pub struct Zoom;

#[derive(Deserialize, JsonSchema)]
pub struct ZoomArgs {
    handle: String,
}

impl Verb for Zoom {
    type Surface = SecretarySurface;
    type Args = ZoomArgs;
    type Answer = Value;
    const NAME: &'static str = "zoom";
    const DESCRIPTION: &'static str = "OPEN A PART OF THE RECORD of every conversation you \
        were told when this one opened: `handle` is a part's `first+lines`. Answers what is \
        beneath it, each with its own handle: a line whole, a summary (`derived`, nobody's own \
        words), or a span not summarised yet. A part you were shown in parentheses may hold \
        somebody else's words, and opening it shows them: its answer says `taint: tainted`, \
        and a `remember` after it becomes a card. Zoom before you act on what a summary says, \
        guess, or ask.";

    fn run(call: &Call<'_>, args: ZoomArgs) -> Result<Value, Refusal> {
        told::zoom(
            &Reading::of(&call.caps.memories.digests)?,
            &args.handle,
            |_| true,
        )
    }
}
