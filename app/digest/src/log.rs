//! A log as the view and the zoom read it: a tree of lines, each inner node shown by a summary.

use prodrome_data::event::Hash;
use prodrome_digest::{Measure, Node, Tree, View};

use crate::secretary::digest_line::Line;

/// What a label says of the lines under a part that its reader is not shown the words of.
pub trait Kinded {
    /// The kinds of those lines, as a phrase.
    fn kinds(self) -> String;
}

/// A log with nothing to hide shows no kinds.
impl Kinded for () {
    fn kinds(self) -> String {
        String::new()
    }
}

pub trait Log {
    /// What each node carries up the tree.
    type Label: Measure + Copy + Kinded;

    /// The tree, `None` while there are no lines.
    fn tree(&self) -> Option<&Tree<Self::Label>>;

    /// The node of the tree named `name`.
    fn node(&self, name: &Hash) -> Option<&Node<Self::Label>>;

    /// The log under the budget in bytes ([`prodrome_digest::view`]); `None` while there are no
    /// lines.
    fn view(&self, budget: u64) -> Option<View>;

    /// The summary shown for the inner node `node`, if it has one.
    fn summary(&self, node: &Hash) -> Option<&Line>;

    /// What `node` stands for to a reader: its label joined with what a summary of it adds.
    fn label(&self, node: &Node<Self::Label>) -> Self::Label;

    /// The leaf `leaf` as one line of a view.
    fn line(&self, leaf: &Hash) -> Option<String>;

    /// The leaf `leaf` whole.
    fn whole(&self, leaf: &Hash) -> Option<String>;
}
