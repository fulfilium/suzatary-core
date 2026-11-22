//! A proposal as its conversation records it.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    /// Its index among the conversation's proposals: what `proposal:<conversation>/<index>`
    /// names.
    pub index: usize,
    pub at: suzatary_base::capability::Datetime,
    pub object: suzatary_inbox::secretary::inbox::ProposalId,
    /// How many turns stood before this was proposed: the card is drawn after turn
    /// `after_turn - 1`, `0` meaning before any turn.
    pub after_turn: usize,
    /// The proposal of this conversation it replaces.
    pub revises: Option<usize>,
}
