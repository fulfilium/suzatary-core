use serde_json::Value;

use crate::agent::{Call, TriageSurface, answered, refused};
use crate::verb::{NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// Sync every account and reindex.
pub struct Sync;

impl Verb for Sync {
    type Surface = TriageSurface;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "sync";
    const DESCRIPTION: &'static str =
        "Sync all 7 mail accounts and reindex. Returns per-account ok/status.";

    fn run(call: &Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        answered(&call.caps.corpus.sync().map_err(|e| refused(&e))?)
    }
}
