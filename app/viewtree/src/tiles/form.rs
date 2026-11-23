//! What a well-formed tree is. `Tiles::new` and `parse` answer only such trees, and every act
//! keeps one so.

use std::fmt;

use super::node::Node;
use super::open::MOST;
use super::page::Pin;
use super::tree::Tree;

/// Why a tree is not well formed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unfit {
    /// The root's axis, fit or number of workspaces is not the home's.
    Root,
    /// A node below the root has one child.
    Single,
    /// A workspace holds more than `MOST` leaves.
    Crowded,
    /// A workspace holds two pinned leaves, or one that is not its end: the last leaf, reached
    /// through the last child of each node.
    Pin,
}

/// `pin` reads a leaf's pin; `home` is the root the tree must keep the shape of.
pub(crate) fn check<L, H>(
    root: &Node<L>,
    home: &Node<H>,
    pin: &impl Fn(&L) -> Pin,
) -> Result<(), Unfit> {
    if root.axis != home.axis || root.fit != home.fit || root.len() != home.len() {
        return Err(Unfit::Root);
    }
    root.parts().try_for_each(|w| workspace(&w.tree, pin))
}

fn workspace<L>(tree: &Tree<L>, pin: &impl Fn(&L) -> Pin) -> Result<(), Unfit> {
    if !two_below(tree) {
        return Err(Unfit::Single);
    }
    let leaves = tree.leaves();
    if leaves.len() > MOST {
        return Err(Unfit::Crowded);
    }
    let pinned = leaves.iter().filter(|l| pin(l) == Pin::Pinned).count();
    let end_pinned = pin(tree.end().0) == Pin::Pinned;
    match (pinned, end_pinned) {
        (0, _) | (1, true) => Ok(()),
        _ => Err(Unfit::Pin),
    }
}

fn two_below<L>(tree: &Tree<L>) -> bool {
    match tree {
        Tree::Leaf(_) => true,
        Tree::Node(n) => n.len() >= 2 && n.parts().all(|p| two_below(&p.tree)),
    }
}

impl fmt::Display for Unfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Unfit::Root => "the root's axis, fit or workspaces are not the home's",
            Unfit::Single => "a node below the root has one child",
            Unfit::Crowded => "a workspace holds more tiles than it may",
            Unfit::Pin => "a workspace's pinned leaf is not its one end",
        })
    }
}

impl std::error::Error for Unfit {}
