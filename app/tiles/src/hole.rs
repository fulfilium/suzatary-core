//! The one descent down the focus path: the node that holds the focused leaf, opened at it.

use super::node::{Axis, Fit, Node, Part, Share};
use super::tree::Tree;

/// A node whose focused child is a leaf, taken apart at that leaf.
pub(crate) struct Hole<L> {
    pub axis: Axis,
    pub fit: Fit,
    pub before: Vec<Part<L>>,
    pub share: Share,
    pub leaf: L,
    pub after: Vec<Part<L>>,
    pub at: Level,
}

/// Whether the node is the root, whose children are the workspaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Root,
    Below,
}

/// The tree with the node that holds the focused leaf replaced by what `f` makes of it.
pub(crate) fn descend<L>(root: Node<L>, f: impl FnOnce(Hole<L>) -> Node<L>) -> Node<L> {
    down(root, Level::Root, f)
}

fn down<L>(n: Node<L>, at: Level, f: impl FnOnce(Hole<L>) -> Node<L>) -> Node<L> {
    let Node {
        axis,
        fit,
        before,
        focus,
        after,
    } = n;
    let Part { share, tree } = *focus;
    match tree {
        Tree::Leaf(leaf) => f(Hole {
            axis,
            fit,
            before,
            share,
            leaf,
            after,
            at,
        }),
        Tree::Node(c) => Node {
            axis,
            fit,
            before,
            focus: Box::new(Part {
                share,
                tree: Tree::Node(down(c, Level::Below, f)),
            }),
            after,
        },
    }
}

impl<L> Hole<L> {
    /// The node put back, its focused child made by `f` from the leaf.
    pub(crate) fn put(self, f: impl FnOnce(L) -> Tree<L>) -> Node<L> {
        let Hole {
            axis,
            fit,
            before,
            share,
            leaf,
            after,
            ..
        } = self;
        Node {
            axis,
            fit,
            before,
            focus: Box::new(Part {
                share,
                tree: f(leaf),
            }),
            after,
        }
    }
}
