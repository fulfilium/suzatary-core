use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::Written;
use crate::agent::Holds;
use crate::priced::Priced;
use crate::secretary::changes::Act;
use crate::verb::{Chained, Verb};
use suzatary_base::refusal::Refusal;
use suzatary_record::todo::TodoPatch;

/// Change fields on a todo, as the surface writes.
pub struct TodoUpdate<S>(PhantomData<S>);

#[derive(Deserialize, JsonSchema)]
pub struct TodoUpdateArgs {
    todo_id: String,
    rationale: Option<String>,
    body: Option<String>,
    detail: Option<String>,
    detail_append: Option<String>,
    waiting_on: Option<String>,
    #[serde(default)]
    clear_waiting_on: bool,
    #[serde(flatten)]
    priced: Priced,
}

impl<S: Holds> Verb for TodoUpdate<S> {
    type Surface = S;
    type Args = Chained<Written<TodoUpdateArgs, S::Filing>>;
    type Answer = Value;
    const NAME: &'static str = "todo_update";
    const DESCRIPTION: &'static str = S::SAYS.todo_update;

    fn run(call: &S::Call<'_>, Chained(written): Self::Args) -> Result<Value, Refusal> {
        let args = written.args;
        // Category, checklist and deferral are not arguments of this verb; widening it changes the
        // snapshotted surface.
        let act = Act::Update {
            todo: args.todo_id,
            fields: TodoPatch {
                rationale: args.rationale.into(),
                price: args.priced.price()?.into(),
                body: args.body.into(),
                detail: args.detail.into(),
                detail_append: args.detail_append.into(),
                waiting_on: args.waiting_on.into(),
                clear_waiting_on: args.clear_waiting_on,
                ..TodoPatch::default()
            },
        };
        S::write(call, act, written.filing)
    }
}
