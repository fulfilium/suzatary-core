//! Where an open puts a page that is not of the focused leaf's kind: the tree operations that
//! `act` and the arrangement's probe share.

use super::Tile;
use super::hole::{Hole, Level, descend};
use super::leaf::Id;
use super::node::{Axis, Fit, Node, Part, Share};
use super::page::Pin;
use super::tree::Tree;

/// The most tiles a workspace holds. An open that would make more opens here.
pub const MOST: usize = 4;

/// Opens a free page beside the focused leaf, in the leaf's node if that node fits and is not
/// the root, else in a new node across that wraps the leaf. From a free leaf, the page replaces
/// the free children after it, keeping the share of the first, or is inserted right after it.
/// From a pinned leaf, the page replaces the free child right before it, or is inserted before
/// it. The page takes the focus.
pub(crate) fn beside<L: Tile>(root: Node<L>, new: L, share: Share) -> Node<L> {
    descend(root, |h| match (h.at, h.fit) {
        (Level::Root, _) | (Level::Below, Fit::Never) => {
            h.put(|leaf| wrap(Axis::Across, leaf, new, share))
        }
        (Level::Below, Fit::Fits) => within(h, new, share),
    })
}

fn within<L: Tile>(h: Hole<L>, new: L, share: Share) -> Node<L> {
    let Hole {
        axis,
        fit,
        mut before,
        share: s,
        leaf,
        mut after,
        ..
    } = h;
    let pin = leaf.pin();
    let here = Part {
        share: s,
        tree: Tree::Leaf(leaf),
    };
    let page = |share| {
        Box::new(Part {
            share,
            tree: Tree::Leaf(new),
        })
    };
    match pin {
        Pin::Free => {
            let (pinned, free): (Vec<_>, Vec<_>) = after.into_iter().partition(pinned);
            let share = free.first().map_or(share, |p| p.share);
            before.push(here);
            Node {
                axis,
                fit,
                before,
                focus: page(share),
                after: pinned,
            }
        }
        Pin::Pinned => {
            let share = match before.pop() {
                Some(p) if !pinned(&p) => p.share,
                Some(p) => {
                    before.push(p);
                    share
                }
                None => share,
            };
            after.insert(0, here);
            Node {
                axis,
                fit,
                before,
                focus: page(share),
                after,
            }
        }
    }
}

/// A node over the leaf and the new page, which takes the focus and stands before a pinned leaf
/// and after a free one.
pub(crate) fn wrap<L: Tile>(axis: Axis, leaf: L, new: L, opened: Share) -> Tree<L> {
    let pin = leaf.pin();
    let old = Part {
        share: leaf.share(),
        tree: Tree::Leaf(leaf),
    };
    let fresh = Node::of(
        axis,
        Fit::Fits,
        Part {
            share: opened,
            tree: Tree::Leaf(new),
        },
    );
    Tree::Node(match pin {
        Pin::Pinned => Node {
            after: vec![old],
            ..fresh
        },
        Pin::Free => Node {
            before: vec![old],
            ..fresh
        },
    })
}

/// A child that holds a pinned leaf.
pub(crate) fn pinned<L: Tile>(part: &Part<L>) -> bool {
    part.tree.leaves().iter().any(|l| l.pin() == Pin::Pinned)
}

/// The tree focused on the leaf `id`; unchanged when it holds no such leaf.
pub(crate) fn focus<L: Tile>(root: Node<L>, id: Id) -> Node<L> {
    match path(&root, id) {
        Some(path) => along(root, &path),
        None => root,
    }
}

fn path<L: Tile>(n: &Node<L>, id: Id) -> Option<Vec<usize>> {
    n.parts().enumerate().find_map(|(i, p)| match &p.tree {
        Tree::Leaf(l) => (l.id() == Some(id)).then(|| vec![i]),
        Tree::Node(c) => path(c, id).map(|mut rest| {
            rest.insert(0, i);
            rest
        }),
    })
}

/// The tree focused along `path`, a child's place in each node from the root down; the path
/// ends at a leaf, or where it runs out.
pub(crate) fn along<L>(n: Node<L>, path: &[usize]) -> Node<L> {
    let Some((&i, rest)) = path.split_first() else {
        return n;
    };
    n.select(i)
        .on_focus(|tree| match tree {
            Tree::Node(c) => (Tree::Node(along(c, rest)), ()),
            leaf @ Tree::Leaf(_) => (leaf, ()),
        })
        .0
}

/// The leaves of the focused workspace.
pub(crate) fn tiles<L>(root: &Node<L>) -> usize {
    root.focus.tree.leaves().len()
}
