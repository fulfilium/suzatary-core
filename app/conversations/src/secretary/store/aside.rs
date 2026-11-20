//! Turns a rewind set aside.

/// Turns one [`Entry::Rewound`](super::Entry::Rewound) or [`Entry::Again`](super::Entry::Again)
/// set aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    /// The turns, by index.
    pub turns: std::collections::BTreeSet<usize>,
    /// When.
    pub at: suzatary_base::capability::Datetime,
    /// Who asked again.
    pub by: String,
}
