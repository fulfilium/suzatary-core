use std::marker::PhantomData;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::agent::{Holds, answered, refused};
use crate::mail::corpus::Corpus;
use crate::mail::verbs;
use crate::verb::{Own, Verb};
use suzatary_base::refusal::{Refusal, shown};

/// One message in full, its arguments spelled `A`: paged, or whole.
pub struct Show<S, A>(PhantomData<(S, A)>);

/// The most one part of a paged `show` may answer; a longer body spills to a file the agent
/// cannot open.
pub(crate) const SHOW_CAP: usize = 24 * 1024;

/// Which of a message's body a `show` answers.
#[derive(Clone, Copy)]
pub enum Part {
    /// All of it.
    Whole,
    /// Its `n`th part of `SHOW_CAP` characters, from 1, each naming the next.
    Page(usize),
}

/// A message and the part of it.
pub struct Shown {
    id: String,
    part: Part,
}

/// A message, paged.
#[derive(Deserialize, JsonSchema)]
pub struct ShowArgs {
    id: String,
    part: Option<usize>,
}

impl Own<Shown> for ShowArgs {
    fn own(self) -> Shown {
        Shown {
            id: self.id,
            part: Part::Page(self.part.unwrap_or(1).max(1)),
        }
    }
}

/// A message, whole.
#[derive(Deserialize, JsonSchema)]
pub struct WholeArgs {
    id: String,
}

impl Own<Shown> for WholeArgs {
    fn own(self) -> Shown {
        Shown {
            id: self.id,
            part: Part::Whole,
        }
    }
}

impl<S: Holds, A: Own<Shown> + DeserializeOwned + JsonSchema> Verb for Show<S, A> {
    type Surface = S;
    type Args = A;
    type Answer = Value;
    const NAME: &'static str = "show";
    const DESCRIPTION: &'static str = S::SAYS.show;

    fn run(call: &S::Call<'_>, args: A) -> Result<Value, Refusal> {
        let Shown { id, part } = args.own();
        show(S::agent(call).corpus, &id, part)
    }
}

/// `show`, with a body past [`SHOW_CAP`] paged rather than cut when a page is asked for.
fn show(corpus: &dyn Corpus, id: &str, part: Part) -> Result<Value, Refusal> {
    let answer = corpus.show(id).map_err(|e| refused(&e))?;
    let Part::Page(part) = part else {
        return answered(&answer);
    };
    let mut message = match answer {
        verbs::Show::Shown(message) => message,
        verbs::Show::NotFound { .. } | verbs::Show::Ambiguous(_) => return answered(&answer),
    };
    let body = &message.body;
    let total_parts = body.chars().count().div_ceil(SHOW_CAP).max(1);
    if total_parts == 1 {
        return answered(&verbs::Show::Shown(message));
    }
    let at = part.min(total_parts);
    let paged = crate::mail::page(body, (at - 1) * SHOW_CAP, SHOW_CAP);
    let note = if at < total_parts {
        format!(
            "\n\n(… part {at} of {total_parts} — this body is longer than one answer. \
             Ask for the rest with show({}, part={}).)",
            shown(id),
            at + 1
        )
    } else {
        format!("\n\n(… part {at} of {total_parts}, the last.)")
    };
    message.body = format!("{paged}{note}");
    answered(&verbs::Show::Shown(message))
}
