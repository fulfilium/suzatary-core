use prodrome_core::dag::{Dag, Finding};
use prodrome_core::schema::Schema;
use prodrome_core::sign::Registrar;

use super::Trust;

/// `findings` less each `Unsigned` beneath an epoch, read structurally as `verify` reads keys:
/// an object written before its actor had a key is history, not a finding. Every other finding
/// stands, an unsigned write after the epoch included.
#[must_use]
pub fn since_epoch<E: Schema>(dag: &Dag<E>, findings: Vec<Finding>) -> Vec<Finding> {
    let earlier = Trust::of(dag, &Registrar::Anyone);
    findings
        .into_iter()
        .filter(|finding| !matches!(finding, Finding::Unsigned { object, .. } if earlier.predates(object)))
        .collect()
}
