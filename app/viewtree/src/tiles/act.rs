//! What a person does to the tree. No act reads a viewport, so a resize never changes the tree.

use super::Tiles;
use super::hole::{Hole, Level, descend};
use super::leaf::{Id, Leaf};
use super::node::{Axis, Fit, Node, Part, Share};
use super::open::{self, MOST};
use super::page::{Page, Pin};
use super::tree::Tree;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act<P> {
    /// A page the focused workspace already shows takes the focus, and nothing else moves. Else,
    /// where the focused leaf is on the page's `home` chain, each page of the chain after it opens
    /// first, by this rule, and then the page: an open from a workspace's home of a page two steps
    /// from it inserts two tiles, the tree the page's bare address reads as. Each page then opens
    /// so: a page of the focused leaf's kind replaces it; a pinned page becomes the focused
    /// workspace's end, replacing a pinned end or the end of a full workspace, or added after the
    /// last child; any other page goes beside (`open::beside`).
    Open(P),
    /// The focused leaf becomes a node down with the page, which stands below a free leaf and
    /// above a pinned one: the one act that makes a node down. A pinned page opens as `Open`
    /// opens it, at its workspace's end.
    Below(P),
    /// Removes the focused leaf; the neighbour before it takes the focus, else the one after. A
    /// node left with one child is replaced by it. A workspace's last leaf stays.
    Close,
    /// Focuses the leaf `Id`: a crumb's, a tile's, or a workspace's.
    Focus(Id),
    /// Swaps the focused child with its neighbour, in the nearest node below the root on the
    /// direction's axis that has one; nothing moves when either child holds a pinned leaf.
    Move(Dir),
    /// Sets the shares of two neighbouring children of a node that fits, the one holding the
    /// leaf `before` and the one right after it holding `after`: the children either side of the
    /// divider that names those leaves (`Divider`). Where no such two children stand, nothing
    /// changes.
    Resize {
        before: Id,
        after: Id,
        shares: [Share; 2],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Total, and keeps the tree well formed (`Tiles::check`). An open or a `Below` that would put
/// more than `MOST` tiles in the workspace opens here instead, or does nothing when the focused
/// leaf is pinned.
pub fn act<P: Page>(tiles: Tiles<P>, act: Act<P>) -> Tiles<P> {
    match act {
        Act::Open(page) => opened(tiles, page),
        Act::Below(page) if page.pin() == Pin::Pinned => opened(tiles, page),
        Act::Below(page) => bounded(tiles, page, |mut t, page| {
            let new = t.mint(page);
            let opened = new.page().share();
            t.root = descend(t.root, |h| {
                h.put(|leaf| open::wrap(Axis::Down, leaf, new, opened))
            });
            t
        }),
        Act::Close => Tiles {
            root: descend(tiles.root, close).normal_children(),
            ..tiles
        },
        Act::Focus(id) => Tiles {
            root: open::focus(tiles.root, id),
            ..tiles
        },
        Act::Move(dir) => {
            let root = tiles.root.on_focus(|tree| shifted(tree, dir)).0;
            Tiles { root, ..tiles }
        }
        Act::Resize {
            before,
            after,
            shares,
        } => Tiles {
            root: resized(tiles.root, before, after, shares),
            ..tiles
        },
    }
}

fn opened<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    let chain = homes(&page);
    let focused = tiles.focused().page();
    let shown = tiles
        .root
        .focus
        .tree
        .leaves()
        .iter()
        .any(|l| *l.page() == page);
    match chain.iter().position(|c| c == focused) {
        Some(k) if !shown => chain[..k]
            .iter()
            .rev()
            .cloned()
            .chain(std::iter::once(page))
            .fold(tiles, one),
        Some(_) | None => one(tiles, page),
    }
}

/// One page opened by the rule, from the focused leaf.
fn one<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    let shown = tiles
        .root
        .focus
        .tree
        .leaves()
        .iter()
        .find(|l| *l.page() == page)
        .map(|l| l.id());
    if let Some(id) = shown {
        return Tiles {
            root: open::focus(tiles.root, id),
            ..tiles
        };
    }
    if tiles.focused().page().kind() == page.kind() {
        return here(tiles, page);
    }
    match page.pin() {
        Pin::Pinned => at_end(tiles, page),
        Pin::Free => bounded(tiles, page, |mut t, page| {
            let share = page.share();
            let new = t.mint(page);
            t.root = open::beside(t.root, new, share);
            t
        }),
    }
}

/// The pages `page` is opened from, nearest first, up to a workspace's home or `MOST` of them.
pub(crate) fn homes<P: Page>(page: &P) -> Vec<P> {
    let mut chain: Vec<P> = Vec::new();
    let mut at = page.clone();
    while chain.len() < MOST {
        let home = at.home();
        if home == at {
            break;
        }
        chain.push(home.clone());
        at = home;
    }
    chain
}

/// `grow`, unless it adds a tile to a workspace of `MOST` or more.
fn bounded<P: Page>(
    tiles: Tiles<P>,
    page: P,
    grow: impl FnOnce(Tiles<P>, P) -> Tiles<P>,
) -> Tiles<P> {
    let was = open::tiles(&tiles.root);
    let grown = grow(tiles.clone(), page.clone());
    let now = open::tiles(&grown.root);
    if now <= was || now <= MOST {
        grown
    } else {
        match tiles.focused().page().pin() {
            Pin::Free => here(tiles, page),
            Pin::Pinned => tiles,
        }
    }
}

/// The focused leaf replaced by a fresh leaf of `page`.
fn here<P: Page>(mut tiles: Tiles<P>, page: P) -> Tiles<P> {
    let new = tiles.mint(page);
    tiles.root = descend(tiles.root, |h| Hole { leaf: new, ..h }.put(Tree::Leaf));
    tiles
}

/// A pinned page as the focused workspace's end, taking the focus.
fn at_end<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    let (end, path) = tiles.root.focus.tree.end();
    let full = open::tiles(&tiles.root) >= MOST;
    if end.page().pin() == Pin::Pinned || full {
        let mut at = vec![tiles.root.index()];
        at.extend(path);
        let root = open::along(tiles.root, &at);
        return here(Tiles { root, ..tiles }, page);
    }
    let mut tiles = tiles;
    let share = page.share();
    let new = tiles.mint(page);
    tiles.root = tiles
        .root
        .on_focus(|tree| (appended(tree, new, share), ()))
        .0;
    tiles
}

/// The workspace with `new` after its last child, focused on it.
fn appended<P: Page>(tree: Tree<Leaf<P>>, new: Leaf<P>, share: Share) -> Tree<Leaf<P>> {
    match tree {
        Tree::Leaf(l) => open::wrap(Axis::Across, l, new, share),
        Tree::Node(n) => {
            let mut n = n.select(usize::MAX);
            let fresh = Part {
                share,
                tree: Tree::Leaf(new),
            };
            let end = std::mem::replace(&mut *n.focus, fresh);
            n.before.push(end);
            Tree::Node(n)
        }
    }
}

/// The node without its focused leaf; at the root, or with no other child, unchanged.
fn close<L>(h: Hole<L>) -> Node<L> {
    let Hole {
        axis,
        fit,
        mut before,
        share,
        leaf,
        mut after,
        at,
    } = h;
    let next = match (at, before.pop()) {
        (Level::Below, Some(p)) => Some(p),
        (Level::Below, None) if !after.is_empty() => Some(after.remove(0)),
        (Level::Root, Some(p)) => {
            before.push(p);
            None
        }
        (Level::Below | Level::Root, None) => None,
    };
    let focus = Box::new(next.unwrap_or(Part {
        share,
        tree: Tree::Leaf(leaf),
    }));
    Node {
        axis,
        fit,
        before,
        focus,
        after,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Done {
    Yes,
    No,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

fn shifted<P: Page>(tree: Tree<Leaf<P>>, dir: Dir) -> (Tree<Leaf<P>>, Done) {
    let Tree::Node(n) = tree else {
        return (tree, Done::No);
    };
    let (mut n, done) = n.on_focus(|t| shifted(t, dir));
    let (axis, side) = match dir {
        Dir::Left => (Axis::Across, Side::Before),
        Dir::Right => (Axis::Across, Side::After),
        Dir::Up => (Axis::Down, Side::Before),
        Dir::Down => (Axis::Down, Side::After),
    };
    let held = |p: &Part<Leaf<P>>| {
        p.tree
            .leaves()
            .iter()
            .any(|l| l.page().pin() == Pin::Pinned)
    };
    let done = match (done, n.axis == axis, side) {
        (Done::Yes, _, _) | (Done::No, false, _) => done,
        (Done::No, true, Side::After) => match n.after.first() {
            None => Done::No,
            Some(next) if held(next) || held(&n.focus) => Done::Yes,
            Some(_) => {
                let next = n.after.remove(0);
                n.before.push(next);
                Done::Yes
            }
        },
        (Done::No, true, Side::Before) => match n.before.last() {
            None => Done::No,
            Some(prev) if held(prev) || held(&n.focus) => Done::Yes,
            Some(_) => {
                let prev = n.before.remove(n.before.len() - 1);
                n.after.insert(0, prev);
                Done::Yes
            }
        },
    };
    (Tree::Node(n), done)
}

fn resized<P>(n: Node<Leaf<P>>, before: Id, after: Id, shares: [Share; 2]) -> Node<Leaf<P>> {
    let holding = |id: Id| {
        n.parts()
            .position(|p| p.tree.leaves().iter().any(|l| l.id() == id))
    };
    match (holding(before), holding(after), n.fit) {
        (Some(i), Some(j), Fit::Fits) if j == i + 1 => n.map_parts(|k, part| Part {
            share: match k {
                k if k == i => shares[0],
                k if k == j => shares[1],
                _ => part.share,
            },
            ..part
        }),
        (Some(i), Some(j), Fit::Fits | Fit::Never) if i == j => {
            n.map_parts(|k, part| match part.tree {
                Tree::Node(c) if k == i => Part {
                    tree: Tree::Node(resized(c, before, after, shares)),
                    ..part
                },
                tree => Part { tree, ..part },
            })
        }
        _ => n,
    }
}
