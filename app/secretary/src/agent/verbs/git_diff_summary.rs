use std::marker::PhantomData;

use serde_json::Value;

use crate::agent::{Holds, refused};
use crate::verb::{NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// What is uncommitted.
pub struct GitDiffSummary<S>(PhantomData<S>);

impl<S: Holds> Verb for GitDiffSummary<S> {
    type Surface = S;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "git_diff_summary";
    const DESCRIPTION: &'static str = S::SAYS.git_diff_summary;

    fn run(call: &S::Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        S::agent(call)
            .repository
            .diff_summary()
            .map(Value::String)
            .map_err(|e| refused(&e))
    }
}
