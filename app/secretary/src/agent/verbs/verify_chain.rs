use serde_json::Value;

use crate::agent::{Call, TriageSurface, reads};
use crate::verb::{Chained, NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// A full check of the chain.
pub struct VerifyChain;

impl Verb for VerifyChain {
    type Surface = TriageSurface;
    type Args = Chained<NoArgs>;
    type Answer = Value;
    const NAME: &'static str = "verify_chain";
    const DESCRIPTION: &'static str = "Full fsck of the event chain (hash coverage, every parent present). \
        Returns 'healthy' or the list of problems, one per line. `chain` picks \
        which store is checked; the push path checks every one.";

    fn run(call: &Call<'_>, _: Chained<NoArgs>) -> Result<Value, Refusal> {
        Ok(Value::String(reads::verify_chain(call.caps.chain)))
    }
}
