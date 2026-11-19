use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Holds, reads};
use crate::verb::{Chained, Verb};
use suzatary_base::refusal::Refusal;

/// One todo as the chain holds it.
pub struct TodoShow<S>(PhantomData<S>);

#[derive(Deserialize, JsonSchema)]
pub struct TodoShowArgs {
    todo_id: String,
}

impl<S: Holds> Verb for TodoShow<S> {
    type Surface = S;
    type Args = Chained<TodoShowArgs>;
    type Answer = Value;
    const NAME: &'static str = "todo_show";
    const DESCRIPTION: &'static str = S::SAYS.todo_show;

    fn run(call: &S::Call<'_>, Chained(args): Chained<TodoShowArgs>) -> Result<Value, Refusal> {
        reads::todo_show(&S::agent(call), S::chain(call), &args.todo_id)
    }
}
