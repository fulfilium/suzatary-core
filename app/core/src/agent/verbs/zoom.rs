use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Call, TriageSurface};
use crate::refusal::Refusal;
use crate::verb::Verb;

/// What lies beneath a part of the secretary's record.
pub struct Zoom;

#[derive(Deserialize, JsonSchema)]
pub struct ZoomArgs {
    handle: String,
}

impl Verb for Zoom {
    type Surface = TriageSurface;
    type Args = ZoomArgs;
    type Answer = Value;
    const NAME: &'static str = "zoom";
    const DESCRIPTION: &'static str = "OPEN A PART OF THE SECRETARY'S RECORD, read-only: \
        `handle` is a part's `first+lines`, as `context`'s `memory` or `recall` shows it. \
        Answers what is beneath it, each with its own handle: the owner's own line whole, a \
        summary of nothing but their lines (`derived`, nobody's own words), or a span not \
        summarised yet. Anything else (the secretary's answers, a verb's answer, the box's \
        notes) stays shown by its kinds alone, however far you open it. Zoom before you file \
        on what a summary says.";

    fn run(call: &Call<'_>, args: ZoomArgs) -> Result<Value, Refusal> {
        call.caps.memory.record()?.zoom(&args.handle)
    }
}
