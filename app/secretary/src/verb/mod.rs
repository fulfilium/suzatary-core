mod chained;
mod sum;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use suzatary_base::refusal::Refusal;

pub(crate) use chained::chain_named;
pub use chained::{Chained, Own, Request};
pub use sum::tool;
pub(crate) use sum::verbs;

/// One verb. Its arguments are parsed once, where the call arrives, and the schema the model
/// is shown is derived from their type; a surface lists its verbs with `verbs!`.
pub trait Verb {
    type Surface: Surface;
    type Args: DeserializeOwned + JsonSchema;
    /// What it answers, which its surface's answer holds.
    type Answer: Into<<Self::Surface as Surface>::Answer>;
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    fn run(call: &Call<'_, Self>, args: Self::Args) -> Result<Self::Answer, Refusal>;
}

/// A set of verbs: what each runs against, what each answers, and what each is known by.
pub trait Surface {
    type Call<'a>;
    type Answer;
    /// What the surface knows of each verb beyond its name.
    type Class: Copy;
}

pub type Call<'a, V> = <<V as Verb>::Surface as Surface>::Call<'a>;

#[derive(Debug, serde::Deserialize, JsonSchema)]
pub struct NoArgs {}
