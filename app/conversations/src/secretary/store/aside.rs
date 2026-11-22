//! Turns a rewind set aside.

/// Turns one [`Entry::Rewound`](super::Entry::Rewound) or [`Entry::Again`](super::Entry::Again)
/// set aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    pub turns: std::collections::BTreeSet<usize>,
    pub at: suzatary_base::capability::Datetime,
    pub by: String,
}
