use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::agent::{Holds, answered, refused};
use crate::capability::Query;
use crate::refusal::Refusal;
use crate::verb::Verb;

/// Search the mail: the triage agent's verb, which the secretary's own `search` widens to files.
pub struct Search<S>(PhantomData<S>);

#[derive(Deserialize, JsonSchema)]
pub struct SearchArgs {
    query: String,
    limit: Option<usize>,
}

impl<S: Holds> Verb for Search<S> {
    type Surface = S;
    type Args = SearchArgs;
    type Answer = Value;
    const NAME: &'static str = "search";
    const DESCRIPTION: &'static str = "Any notmuch query, newest first. Use to check for prior \
        related threads before deciding a disposition (e.g. `from:sender@example.com`). Each \
        row's `held` lists the kinds of held value its message and its PDF attachments name \
        (`ssn`, `passport`, `a_number`, ...), never a value: `show` gives each as a placeholder.";

    fn run(call: &S::Call<'_>, args: SearchArgs) -> Result<Value, Refusal> {
        let query = Query::new(&args.query).map_err(|e| Refusal::bad_request(e.to_string()))?;
        answered(
            &S::agent(call)
                .corpus
                .search(&query, args.limit)
                .map_err(|e| refused(&e))?,
        )
    }
}
