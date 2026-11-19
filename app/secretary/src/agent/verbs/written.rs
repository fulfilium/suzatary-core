use schemars::JsonSchema;
use serde::Deserialize;

/// A writing verb's own arguments `A`, beside what its surface files a write with, `F`.
#[derive(Deserialize, JsonSchema)]
pub struct Written<A, F> {
    #[serde(flatten)]
    pub(crate) args: A,
    #[serde(flatten)]
    pub(crate) filing: F,
}
