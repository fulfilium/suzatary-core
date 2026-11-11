use std::marker::PhantomData;

use serde_json::Value;

use crate::agent::{Holds, reads};
use crate::verb::{Chained, NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// One chain's open todos.
pub struct ListOpenTodos<S>(PhantomData<S>);

impl<S: Holds> Verb for ListOpenTodos<S> {
    type Surface = S;
    type Args = Chained<NoArgs>;
    type Answer = Value;
    const NAME: &'static str = "list_open_todos";
    const DESCRIPTION: &'static str = S::SAYS.list_open_todos;

    fn run(call: &S::Call<'_>, _: Chained<NoArgs>) -> Result<Value, Refusal> {
        reads::list_open_todos(&S::agent(call), S::chain(call))
    }
}
