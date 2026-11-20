use prodrome_core::dag::{Dag, Finding};
use prodrome_core::policy::Policy;
use prodrome_core::schema::Schema;
use prodrome_core::sign::Registrar;

use super::Trust;

/// The fsck's findings in the objects of `dag` alone: [`Dag::verify`] under `policy` at that
/// history, then [`since_epoch`]. A store on disk reports these beside what only its files show.
#[must_use]
pub fn findings_of<E: Schema, P: Policy<E>>(dag: &Dag<E>, policy: &P) -> Vec<Finding> {
    let at = policy.at(dag);
    since_epoch(dag, dag.verify(at.as_ref().unwrap_or(policy)))
}

/// `findings` less each `Unsigned` beneath an epoch: an object written before its actor had a key
/// is history, not a finding. Every other finding
/// stands, an unsigned write after the epoch included.
#[must_use]
pub fn since_epoch<E: Schema>(dag: &Dag<E>, findings: Vec<Finding>) -> Vec<Finding> {
    let earlier = Trust::of(dag, &Registrar::Anyone);
    findings
        .into_iter()
        .filter(|finding| !matches!(finding, Finding::Unsigned { object, .. } if earlier.predates(object)))
        .collect()
}
