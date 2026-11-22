use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::agent::Agent;
use crate::verb::Surface;
use suzatary_base::chains::ChainPolicy;
use suzatary_base::refusal::Refusal;
use suzatary_base::writer::Writer;
use suzatary_digest::secretary::shown_record::{ShownRecord, Shows};
use suzatary_inbox::secretary::changes::Act;

use super::Says;

/// A surface whose calls hold the agent's record and the chain they named, so a verb both the
/// triage agent and the secretary hold is one implementation over it: the record is a value the
/// call hands the verb, and whatever else the surface's record holds the verb never sees.
pub trait Holds: Surface<Answer: From<Value>> {
    const WRITES_AS: Writer;
    /// What this surface's model is told each shared verb does.
    const SAYS: Says;
    /// What a writing verb takes beside its act: the reasons for the card it may file.
    type Filing: DeserializeOwned + JsonSchema;
    fn agent<'a>(call: &Self::Call<'a>) -> Agent<'a>;
    /// Which parts of the record this surface's model reads in words.
    type Shows: Shows;
    /// The record of the conversations, as this surface's model may read it.
    fn record(call: &Self::Call<'_>) -> Result<ShownRecord<Self::Shows>, Refusal>;
    fn chain<'a>(call: &Self::Call<'a>) -> &'a ChainPolicy;
    /// Perform or file `act` on the call's chain, as this surface writes.
    fn write(call: &Self::Call<'_>, act: Act, filing: Self::Filing) -> Result<Value, Refusal>;
}
