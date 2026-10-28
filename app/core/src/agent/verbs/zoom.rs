use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::Holds;
use crate::refusal::Refusal;
use crate::verb::Verb;

/// What lies beneath a part of the record, as the surface's model may read it.
pub struct Zoom<S>(PhantomData<S>);

#[derive(Deserialize, JsonSchema)]
pub struct ZoomArgs {
    handle: String,
}

impl<S: Holds> Verb for Zoom<S> {
    type Surface = S;
    type Args = ZoomArgs;
    type Answer = Value;
    const NAME: &'static str = "zoom";
    const DESCRIPTION: &'static str = S::SAYS.zoom;

    fn run(call: &S::Call<'_>, args: ZoomArgs) -> Result<Value, Refusal> {
        S::record(call)?.zoom(&args.handle)
    }
}
