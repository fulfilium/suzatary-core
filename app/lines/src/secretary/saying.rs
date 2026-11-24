//! What a line that says something is beside its turn.

/// Said on its own, or an edit's own question, which takes up the edit's `Again` it rests on
/// alone: stored as `Said` and `Edited`, so a line is an edit's question by its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saying {
    Plain,
    Edit,
}
