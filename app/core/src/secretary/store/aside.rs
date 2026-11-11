//! Turns a rewind set aside.

/// Turns one [`Entry::Rewound`](super::Entry::Rewound) set aside: `from..until`, as the file stood.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aside {
    /// The first turn set aside.
    pub from: usize,
    /// One past the last: how many turns stood when it was written.
    pub until: usize,
    /// When.
    pub at: suzatary_base::capability::Datetime,
    /// Who asked again.
    pub by: String,
}
