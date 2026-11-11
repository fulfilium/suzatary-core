use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::Written;
use crate::agent::Holds;
use crate::secretary::changes::Act;
use crate::verb::{Chained, Verb};
use suzatary_base::refusal::{Refusal, shown};

/// A todo's lifecycle event, as the surface writes.
pub struct Record<S>(PhantomData<S>);

/// The event, the todo and a note, absent or null meaning none.
#[derive(Deserialize, JsonSchema)]
pub struct RecordArgs {
    kind: String,
    todo: String,
    note: Option<String>,
}

impl RecordArgs {
    /// The act this event is; repricing is `todo_update`'s.
    fn act(self) -> Result<Act, Refusal> {
        let (todo, note) = (self.todo, self.note.unwrap_or_default());
        match self.kind.as_str() {
            "completed" => Ok(Act::Complete { todo, note }),
            "cancelled" => Ok(Act::Cancel { todo, note }),
            "reopened" => Ok(Act::Reopen { todo, note }),
            "tended" => Ok(Act::Tend { todo, note }),
            other => Err(Refusal::bad_request(format!(
                "kind must be one of completed, cancelled, reopened or tended, got {}",
                shown(other)
            ))),
        }
    }
}

impl<S: Holds> Verb for Record<S> {
    type Surface = S;
    type Args = Chained<Written<RecordArgs, S::Filing>>;
    type Answer = Value;
    const NAME: &'static str = "record";
    const DESCRIPTION: &'static str = S::SAYS.record;

    fn run(call: &S::Call<'_>, Chained(written): Self::Args) -> Result<Value, Refusal> {
        S::write(call, written.args.act()?, written.filing)
    }
}
