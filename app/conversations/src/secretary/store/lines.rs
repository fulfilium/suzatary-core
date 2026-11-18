//! A conversation's proposal lines against the inbox's reading.

use super::{Line, Standing, Unplaced};

/// A conversation's proposal lines against the inbox's reading, one per line, in order: a line
/// the reading cannot place costs only itself and is never skipped unseen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lines(pub(super) Vec<Line>);

impl Lines {
    /// Every line, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Line> {
        self.0.iter()
    }

    /// The lines the reading places, in order, owned.
    #[must_use]
    pub fn into_standings(self) -> Vec<Standing> {
        self.0
            .into_iter()
            .filter_map(|line| match line {
                Line::Placed(standing) => Some(*standing),
                Line::Unplaced(_) => None,
            })
            .collect()
    }

    /// The lines the reading places, in order.
    pub fn standings(&self) -> impl Iterator<Item = &Standing> {
        self.0.iter().filter_map(|line| match line {
            Line::Placed(standing) => Some(&**standing),
            Line::Unplaced(_) => None,
        })
    }

    /// The lines it cannot place, in order, each with why.
    pub fn unplaced(&self) -> impl Iterator<Item = &Unplaced> {
        self.0.iter().filter_map(|line| match line {
            Line::Placed(_) => None,
            Line::Unplaced(unplaced) => Some(unplaced),
        })
    }
}
