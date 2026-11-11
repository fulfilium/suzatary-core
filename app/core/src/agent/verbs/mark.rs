use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Call, TriageSurface, answered, refused};
use crate::mail::state::Decision;
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;

/// Record a decision about a message.
pub struct Mark;

#[derive(Deserialize, JsonSchema)]
pub struct MarkArgs {
    hash: String,
    disposition: String,
    todo_id: Option<String>,
    note: Option<String>,
    native_id: Option<String>,
}

impl Verb for Mark {
    type Surface = TriageSurface;
    type Args = MarkArgs;
    type Answer = Value;
    const NAME: &'static str = "mark";
    const DESCRIPTION: &'static str = "Record a decision about a message. disposition: 'todo', 'noise', or \
        'skip'. Pass todo_id when disposition is 'todo', matching the id you gave \
        to `todo_add`; a 'todo' without it, or a 'noise' or 'skip' with it, is refused. Pass native_id as the `id` from the same new/search row \
        this `hash` came from; without it, the `id` that row was listed under is kept.";

    fn run(call: &Call<'_>, args: MarkArgs) -> Result<Value, Refusal> {
        let decision = Decision::parse(&args.disposition, args.todo_id.as_deref())
            .map_err(|failed| Refusal::bad_request(failed.to_string()))?;
        answered(
            &call
                .caps
                .corpus
                .mark(
                    &args.hash,
                    decision,
                    args.note.as_deref(),
                    args.native_id.as_deref(),
                )
                .map_err(|e| refused(&e))?,
        )
    }
}
