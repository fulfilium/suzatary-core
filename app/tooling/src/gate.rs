//! Whose verdicts a table holds.

/// The directory under `.git/train/gates` of one gate's rows: `merge-gate`, or a stub's
/// `other-<its blob>`, so a stub's green is never read as the merge gate's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Gate(String);

impl Gate {
    #[must_use]
    pub fn merge_gate() -> Gate {
        Gate("merge-gate".into())
    }

    /// A stub gate's, keyed by its bytes: the guards' seam `SUZATARY_TRAIN_GATE`.
    #[cfg(feature = "seams")]
    #[must_use]
    pub fn stub(blob: &crate::oid::Oid) -> Gate {
        Gate(format!("other-{blob}"))
    }

    #[must_use]
    pub fn dir(&self) -> &str {
        &self.0
    }
}
