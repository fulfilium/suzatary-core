//! What asking again keeps of the last round.

/// What asking again keeps of the last round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rewind {
    /// The question stands and is pending again; the trace and answer go.
    Regenerate,
    /// The question goes too; the new one is appended after the rewind.
    Edit,
}
