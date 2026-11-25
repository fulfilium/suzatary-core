//! `Act::Put`: a leaf taken from where it stands and put beside another leaf of its workspace.

use super::Tile;
use super::act::{Dir, Side};
use super::leaf::{Id, Leaf};
use super::node::{Axis, Fit, Node, Part};
use super::open;
use super::page::{Page, Pin};
use super::tree::Tree;

/// The tree without the leaf `leaf`, in which the leaf `beside` stands next to it on the side
/// `edge` names: a sibling in `beside`'s node where that node fits and runs along the edge's axis,
/// else in a new node of that axis that wraps `beside`. The leaf keeps its id and its share. Once
/// the leaf is put, a node left with one child is replaced by it. The leaf never moves into
/// another workspace, and a pinned leaf never moves. The pinned end stays last: beside a pinned
/// leaf, the leaf goes before it.
///
/// Every node keeps its focused child, but a child that goes passes its focus to its neighbour; the
/// moved leaf is a node's focused child only where it was the focused leaf, which stays focused.
pub(crate) fn put<P: Page>(root: Node<Leaf<P>>, leaf: Id, beside: Id, edge: Dir) -> Node<Leaf<P>> {
    let focused = root.focus.tree.focused().id();
    let (Some(from), Some(to)) = (open::path(&root, leaf), open::path(&root, beside)) else {
        return root;
    };
    let one_workspace = from.first() == to.first() && from.len() > 1;
    let free = root.parts().nth(from[0]).is_some_and(|w| {
        w.tree
            .leaves()
            .iter()
            .any(|l| l.id() == leaf && l.pin() == Pin::Free)
    });
    if leaf == beside || !one_workspace || !free {
        return root;
    }
    let (root, taken) = take(root, &from);
    let Some(part) = taken else { return root };
    let Some(to) = open::path(&root, beside) else {
        return root;
    };
    let side = match (edge.along().1, pinned(&root, &to)) {
        (Side::After, false) => Side::After,
        (Side::Before | Side::After, _) => Side::Before,
    };
    let root = put_at(root, &to, part, edge.along().0, side).normal_children();
    open::focus(root, focused)
}

/// The leaf at `path` is a pinned one.
fn pinned<P: Page>(root: &Node<Leaf<P>>, path: &[usize]) -> bool {
    let mut at = root;
    for (n, &k) in path.iter().enumerate() {
        match at.parts().nth(k).map(|p| &p.tree) {
            Some(Tree::Node(c)) => at = c,
            Some(Tree::Leaf(l)) => return n + 1 == path.len() && l.pin() == Pin::Pinned,
            None => return false,
        }
    }
    false
}

/// The tree without the child at `path`, and that child; unchanged, and none, where its node has
/// no other child.
fn take<L>(n: Node<L>, path: &[usize]) -> (Node<L>, Option<Part<L>>) {
    match path {
        [] => (n, None),
        [k] => match n.without(*k) {
            Ok((n, part)) => (n, Some(part)),
            Err(n) => (n, None),
        },
        [k, rest @ ..] => {
            let mut taken = None;
            let n = n.on_child(*k, |p| match p.tree {
                Tree::Node(c) => {
                    let (c, part) = take(c, rest);
                    taken = part;
                    Part::new(p.share, Tree::Node(c))
                }
                leaf @ Tree::Leaf(_) => Part::new(p.share, leaf),
            });
            (n, taken)
        }
    }
}

/// `part` put at the leaf at `path` on `side` of it, along `axis`.
fn put_at<L: Tile>(n: Node<L>, path: &[usize], part: Part<L>, axis: Axis, side: Side) -> Node<L> {
    match path {
        [] => n,
        [k] if n.axis == axis && n.fit == Fit::Fits => n.insert(
            match side {
                Side::Before => *k,
                Side::After => k + 1,
            },
            part,
        ),
        [k] => n.on_child(*k, |at| wrapped(at, part, axis, side)),
        [k, rest @ ..] => n.on_child(*k, |p| match p.tree {
            Tree::Node(c) => Part::new(p.share, Tree::Node(put_at(c, rest, part, axis, side))),
            leaf @ Tree::Leaf(_) => Part::new(p.share, leaf),
        }),
    }
}

/// A node of `at` and `part`, standing where `at` stood with its share, and focused on `at`.
fn wrapped<L>(at: Part<L>, part: Part<L>, axis: Axis, side: Side) -> Part<L> {
    let share = at.share;
    let (before, after) = match side {
        Side::Before => (vec![part], Vec::new()),
        Side::After => (Vec::new(), vec![part]),
    };
    let node = Node {
        axis,
        fit: Fit::Fits,
        before,
        focus: Box::new(at),
        after,
    };
    Part::new(share, Tree::Node(node))
}
