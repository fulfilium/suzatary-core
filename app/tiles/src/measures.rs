//! The lengths a host chooses for the arrangement, beside the pages' own.

use super::px::{Edge, Px, Viewport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measures {
    /// Between two drawn children.
    pub divider: Px,
    /// A crumb row's height.
    pub row: Px,
    /// A crumb's width, which bounds how many crumbs a row shows; zero shows every crumb.
    pub crumb: Px,
    /// Every flowing page's least height.
    pub tall: Px,
    pub root_row: RootRow,
}

/// Where the root's crumb row stands in the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRow {
    /// At the top, on every viewport.
    Top,
    /// At the foot on a viewport narrower than this width, else at the top.
    FootUnder(Px),
}

impl RootRow {
    pub(crate) fn edge(self, at: Viewport) -> Edge {
        match self {
            RootRow::FootUnder(w) if at.w < w => Edge::Foot,
            RootRow::FootUnder(_) | RootRow::Top => Edge::Top,
        }
    }
}
