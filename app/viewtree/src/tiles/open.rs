//! Where an open puts a page that is not of the focused leaf's kind: the tree operations that
//! `act` and the arrangement's probe share.

use super::Tile;
use super::hole::{Hole, Level, descend};
use super::leaf::Id;
use super::node::{Axis, Fit, Node, Part, Share};
use super::page::Pin;
use super::tree::Tree;

/// The most free tiles a workspace holds; its pinned end is not counted, so a docked page never
/// costs a person a page. An open that would make more opens here.
pub const MOST: usize = 4;

/// Opens a page beside the focused leaf, in the leaf's node if that node fits and is not the
/// root, else in a new node across that wraps the leaf. The page replaces the free children after
/// the leaf, keeping the share of the first, or is inserted right after it, and takes the focus. A
/// pinned leaf here stands alone in its workspace (`origin` moved off any other): the page goes
/// before it.
pub(crate) fn beside<L: Tile>(root: Node<L>, new: L, share: Share) -> Node<L> {
    descend(root, |h| match (h.at, h.fit, h.leaf.pin()) {
        (Level::Below, Fit::Fits, Pin::Free) => within(h, new, share),
        (Level::Root, _, _)
        | (Level::Below, Fit::Never, _)
        | (Level::Below, Fit::Fits, Pin::Pinned) => {
            h.put(|leaf| wrap(Axis::Across, leaf, new, share))
        }
    })
}

fn within<L: Tile>(h: Hole<L>, new: L, share: Share) -> Node<L> {
    let Hole {
        axis,
        fit,
        mut before,
        share: s,
        leaf,
        after,
        ..
    } = h;
    let (pinned, free): (Vec<_>, Vec<_>) = after.into_iter().partition(pinned);
    let share = free.first().map_or(share, |p| p.share);
    before.push(Part {
        share: s,
        tree: Tree::Leaf(leaf),
    });
    Node {
        axis,
        fit,
        before,
        focus: Box::new(Part {
            share,
            tree: Tree::Leaf(new),
        }),
        after: pinned,
    }
}

/// The tree focused where an open from its focused leaf is made: a pinned leaf opens from the free
/// leaf before it, the one its node's child before it is focused on, so a link in a docked page
/// opens as it would from the page beside it. A pinned leaf alone in its workspace opens from
/// itself.
pub(crate) fn origin<L: Tile>(root: Node<L>) -> Node<L> {
    descend(root, |h| {
        let Hole {
            axis,
            fit,
            mut before,
            share,
            leaf,
            mut after,
            at,
        } = h;
        let here = Part {
            share,
            tree: Tree::Leaf(leaf),
        };
        match (at, here.tree.focused().pin(), before.pop()) {
            (Level::Below, Pin::Pinned, Some(prev)) => {
                after.insert(0, here);
                Node {
                    axis,
                    fit,
                    before,
                    focus: Box::new(prev),
                    after,
                }
            }
            (_, _, prev) => {
                before.extend(prev);
                Node {
                    axis,
                    fit,
                    before,
                    focus: Box::new(here),
                    after,
                }
            }
        }
    })
}

/// `grow` with `new`, unless it puts more than `MOST` free tiles in the workspace: then `new`
/// replaces the focused leaf. The act and the arrangement's probe open by this one rule.
pub(crate) fn bounded<L: Tile + Clone>(
    root: Node<L>,
    new: L,
    grow: impl FnOnce(Node<L>, L) -> Node<L>,
) -> Node<L> {
    let was = free(&root);
    let grown = grow(root.clone(), new.clone());
    let now = free(&grown);
    if now <= was || now <= MOST {
        grown
    } else {
        descend(root, |h| Hole { leaf: new, ..h }.put(Tree::Leaf))
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

/// The free leaves of the focused workspace: what `MOST` bounds.
pub(crate) fn free<L: Tile>(root: &Node<L>) -> usize {
    root.focus
        .tree
        .leaves()
        .iter()
        .filter(|l| l.pin() == Pin::Free)
        .count()
}
