//! A surface whose every call holds the agent's record.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::authoring::Writer;
use crate::capability::Agent;
use crate::chains::ChainPolicy;
use crate::refusal::Refusal;
use crate::secretary::changes::Act;
use crate::secretary::shown_record::{ShownRecord, Shows};
use crate::verb::Surface;

use super::Says;

/// A surface whose calls hold the agent's record and the chain they named, so a verb both the
/// triage agent and the secretary hold is one implementation over it: the record is a value the
/// call hands the verb, and whatever else the surface's record holds the verb never sees.
pub trait Holds: Surface<Answer: From<Value>> {
    /// Who a write through this surface is.
    const WRITES_AS: Writer;
    /// What this surface's model is told each shared verb does.
    const SAYS: Says;
    /// What a writing verb takes beside its act: the triage agent's reasons for the card it
    /// files, and nothing for the secretary, which writes at once.
    type Filing: DeserializeOwned + JsonSchema;
    /// The agent's record, as this call holds it.
    fn agent<'a>(call: &Self::Call<'a>) -> Agent<'a>;
    /// Which parts of the record this surface's model reads in words.
    type Shows: Shows;
    /// The record of the conversations, as this surface's model may read it.
    ///
    /// # Errors
    /// The stores' own.
    fn record(call: &Self::Call<'_>) -> Result<ShownRecord<Self::Shows>, Refusal>;
    /// The policy of the chain the call named.
    fn chain<'a>(call: &Self::Call<'a>) -> &'a ChainPolicy;
    /// Perform or file `act` on the call's chain, as this surface writes.
    ///
    /// # Errors
    /// The builders' refusal of the act, and the stores' failures.
    fn write(call: &Self::Call<'_>, act: Act, filing: Self::Filing) -> Result<Value, Refusal>;
}

/// What a surface that writes at once files a write with: nothing.
#[derive(Deserialize, JsonSchema)]
pub struct Direct {}
