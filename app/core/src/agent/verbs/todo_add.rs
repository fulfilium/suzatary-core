use std::marker::PhantomData;

use prodrome_core::fpl;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use super::Written;
use crate::agent::Holds;
use crate::agent::sourced::Sourced;
use crate::priced::{PRICE_SPELLINGS, Priced, day_of};
use crate::secretary::changes::{Act, Provenance};
use crate::todo::TodoFields;
use crate::verb::{Caught, Chained, Verb};
use suzatary_base::refusal::Refusal;

/// Author a new todo, as the surface writes.
pub struct TodoAdd<S>(PhantomData<S>);

#[derive(Deserialize, JsonSchema)]
pub struct TodoAddArgs {
    todo_id: String,
    body: String,
    rationale: String,
    #[serde(default = "crate::todo::fields::plain_kind")]
    kind: String,
    category: Option<String>,
    #[serde(flatten)]
    priced: Priced,
    #[serde(default)]
    detail: String,
    waiting_on: Option<String>,
    #[serde(default)]
    source_sender: Caught<Option<String>>,
    #[serde(default)]
    source_subject: Caught<Option<String>>,
    #[serde(default)]
    source_date: Caught<Option<String>>,
    #[serde(default)]
    source_hash: Caught<Option<String>>,
    date: Option<String>,
    parent: Option<String>,
}

impl TodoAddArgs {
    /// The act these arguments spell.
    fn act(self) -> Result<Act, Refusal> {
        let source = Sourced {
            sender: self.source_sender,
            subject: self.source_subject,
            date: self.source_date,
            hash: self.source_hash,
        }
        .source()?;
        let created = self
            .date
            .as_deref()
            .map(|text| {
                fpl::datetime_of(chrono::NaiveDateTime::from(day_of(text, "date")?))
                    .map_err(|failed| Refusal::bad_request(failed.to_string()))
            })
            .transpose()?;
        Ok(Act::Add {
            todo: self.todo_id,
            fields: TodoFields {
                body: self.body,
                rationale: self.rationale,
                kind: self.kind,
                category: self.category.unwrap_or_default(),
                price: self
                    .priced
                    .price()?
                    .ok_or_else(|| Refusal::bad_request(PRICE_SPELLINGS))?,
                detail: self.detail,
                waiting_on: self.waiting_on.unwrap_or_default(),
            },
            from: Provenance {
                source,
                created,
                parent: self.parent,
            },
        })
    }
}

impl<S: Holds> Verb for TodoAdd<S> {
    type Surface = S;
    type Args = Chained<Written<TodoAddArgs, S::Filing>>;
    type Answer = Value;
    const NAME: &'static str = "todo_add";
    const DESCRIPTION: &'static str = S::SAYS.todo_add;

    fn run(call: &S::Call<'_>, Chained(written): Self::Args) -> Result<Value, Refusal> {
        S::write(call, written.args.act()?, written.filing)
    }
}
