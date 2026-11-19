use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::agent::{Holds, reads};
use crate::verb::{Own, Verb};
use suzatary_base::refusal::Refusal;

/// A todo's events, oldest first, its arguments spelled `A`.
pub struct History<S, A>(PhantomData<(S, A)>);

/// The todo whose events.
#[derive(Deserialize, JsonSchema)]
pub struct HistoryArgs {
    todo: String,
}

impl<S: Holds, A: Own<HistoryArgs> + DeserializeOwned + JsonSchema> Verb for History<S, A> {
    type Surface = S;
    type Args = A;
    type Answer = Value;
    const NAME: &'static str = "history";
    const DESCRIPTION: &'static str = S::SAYS.history;

    fn run(call: &S::Call<'_>, args: A) -> Result<Value, Refusal> {
        reads::history(S::agent(call).chain, &args.own().todo)
    }
}
