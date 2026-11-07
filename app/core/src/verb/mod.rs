//! A verb a model may call: its name, its words, its arguments, and what it does.

mod arguments;
mod caught;
mod chained;
mod sum;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::refusal::Refusal;

pub use arguments::parse;
pub use caught::Caught;
pub(crate) use chained::chain_named;
pub use chained::{Chained, Own, Request};
pub use sum::tool;
pub(crate) use sum::verbs;

/// One verb. Its arguments are parsed once, where the call arrives, and the schema the model
/// is shown is derived from their type; a surface lists its verbs with `verbs!`.
pub trait Verb {
    /// The surface it is on.
    type Surface: Surface;
    /// Its arguments.
    type Args: DeserializeOwned + JsonSchema;
    /// What it answers, which its surface's answer holds.
    type Answer: Into<<Self::Surface as Surface>::Answer>;
    /// The name the model calls it by.
    const NAME: &'static str;
    /// What the model is told it does.
    const DESCRIPTION: &'static str;
    /// Do it.
    ///
    /// # Errors
    /// A call the verb refuses, or an effect's failure.
    fn run(call: &Call<'_, Self>, args: Self::Args) -> Result<Self::Answer, Refusal>;
}

/// A set of verbs: what each runs against, what each answers, and what each is known by.
pub trait Surface {
    /// What a call runs against.
    type Call<'a>;
    /// What a call answers.
    type Answer;
    /// What the surface knows of each verb beyond its name.
    type Class: Copy;
}

/// What `V` runs against.
pub type Call<'a, V> = <<V as Verb>::Surface as Surface>::Call<'a>;

/// The arguments of a verb that takes none.
#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct NoArgs {}
