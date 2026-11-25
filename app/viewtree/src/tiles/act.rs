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
    /// so: a page of the focused leaf's kind replaces it; a pinned page is held (`Hold`) and
    /// focused, or replaces a focused pinned end; any other page goes beside (`open::beside`). An
    /// open from a pinned leaf is the same open from the free leaf before it (`open::origin`).
    Open(P),
    /// The focused leaf becomes a node down with the page below it: the one act that makes a node
    /// down. From a pinned leaf, it is made from the free leaf before it; a pinned page opens as
    /// `Open` opens it.
    Below(P),
    /// Removes the focused leaf; the neighbour before it takes the focus, else the one after. A
    /// node left with one child is replaced by it. A workspace's last free leaf stays, and so does
    /// its last leaf. Beside a page `Hold` docked, the close is the one made without it.
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
    /// A pinned page docked at the focused workspace's end, the focus where it stands: it
    /// replaces a pinned end of another page, or stands after the workspace's children where the
    /// workspace is a node across that fits, and otherwise beside the whole workspace in a new
    /// node across; every other workspace lets its pinned end go, but one that is that
    /// workspace's focused leaf, so one page is docked at a time. A pinned end that has the focus is the person's and stays, and a free page is not
    /// held. `Release` after it is `Release`.
    Hold(P),
    /// Every workspace's pinned end let go, the focus where it stands; one that is its
    /// workspace's focused leaf stays.
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Total, and keeps the tree well formed (`Tiles::check`). An open or a `Below` that would put
/// more than `MOST` free tiles in the workspace opens here instead.
pub fn act<P: Page>(tiles: Tiles<P>, act: Act<P>) -> Tiles<P> {
    match act {
        Act::Open(page) => opened(tiles, page),
        Act::Below(page) if page.pin() == Pin::Pinned => opened(tiles, page),
        Act::Below(page) => {
            let tiles = Tiles {
                root: open::origin(tiles.root),
                ..tiles
            };
            let opened = page.share();
            let mut tiles = tiles;
            let new = tiles.mint(page);
            Tiles {
                root: open::bounded(tiles.root, new, |root, new| {
                    descend(root, |h| {
                        h.put(|leaf| open::wrap(Axis::Down, leaf, new, opened))
                    })
                }),
                ..tiles
            }
        }
        Act::Close => match (tiles.focused().page().pin(), open::free(&tiles.root)) {
            (Pin::Free, n) if n <= 1 => tiles,
            (Pin::Free | Pin::Pinned, _) => closed(tiles),
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
        Act::Hold(page) => held(tiles, page),
        Act::Release => released(tiles),
    }
}

fn opened<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    if let Some(id) = shown(&tiles, &page) {
        return Tiles {
            root: open::focus(tiles.root, id),
            ..tiles
        };
    }
    let tiles = Tiles {
        root: open::origin(tiles.root),
        ..tiles
    };
    let chain = homes(&page);
    match chain.iter().position(|c| c == tiles.focused().page()) {
        Some(k) => chain[..k]
            .iter()
            .rev()
            .cloned()
            .chain(std::iter::once(page))
            .fold(tiles, one),
        None => one(tiles, page),
    }
}

/// The first leaf of the focused workspace that shows `page`.
fn shown<P: Page>(tiles: &Tiles<P>, page: &P) -> Option<Id> {
    tiles
        .root
        .focus
        .tree
        .leaves()
        .iter()
        .find(|l| l.page() == page)
        .map(|l| l.id())
}

/// One page opened by the rule, from the focused leaf.
fn one<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    if let Some(id) = shown(&tiles, &page) {
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
        Pin::Free => {
            let share = page.share();
            let mut tiles = tiles;
            let new = tiles.mint(page);
            Tiles {
                root: open::bounded(tiles.root, new, |root, new| open::beside(root, new, share)),
                ..tiles
            }
        }
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

/// The focused leaf replaced by a fresh leaf of `page`.
fn here<P: Page>(mut tiles: Tiles<P>, page: P) -> Tiles<P> {
    let new = tiles.mint(page);
    tiles.root = descend(tiles.root, |h| Hole { leaf: new, ..h }.put(Tree::Leaf));
    tiles
}

/// A pinned page held and focused, or in place of a focused pinned end.
fn at_end<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    let tiles = docked_here(tiles, page.clone());
    let (end, _) = tiles.root.focus.tree.end();
    match (*end.page() == page).then(|| end.id()) {
        Some(id) => Tiles {
            root: open::focus(tiles.root, id),
            ..tiles
        },
        None => here(tiles, page),
    }
}

fn held<P: Page>(tiles: Tiles<P>, page: P) -> Tiles<P> {
    match page.pin() {
        Pin::Free => tiles,
        Pin::Pinned => let_go(docked_here(tiles, page), Where::Elsewhere),
    }
}

/// The pinned page at the focused workspace's end, where `Hold` docks it: a pinned end of the same
/// page stays the same leaf, moved there if it stands elsewhere, with its share; one of another
/// page is replaced. A pinned end that has the focus stays as it stands.
fn docked_here<P: Page>(mut tiles: Tiles<P>, page: P) -> Tiles<P> {
    let focused = tiles.focused().id();
    let (end, path) = tiles.root.focus.tree.end();
    let kept = match (page.pin(), end.page().pin()) {
        (Pin::Free, _) => return tiles,
        (Pin::Pinned, Pin::Pinned) if end.id() == focused => return tiles,
        (Pin::Pinned, Pin::Pinned) if *end.page() == page => {
            Some((end.clone(), share_at(&tiles.root.focus.tree, &path)))
        }
        (Pin::Pinned, Pin::Pinned | Pin::Free) => None,
    };
    let (leaf, share) = kept.unwrap_or_else(|| {
        let share = page.share();
        (tiles.mint(page), share)
    });
    let root = tiles.root.on_focus(|w| {
        let without = match w.end().0.page().pin() {
            Pin::Pinned => undocked(w),
            Pin::Free => w,
        };
        (docked(without, leaf, share), ())
    });
    Tiles {
        root: root.0,
        ..tiles
    }
}

/// The workspace with `new` docked at its end, the focus where it stands.
fn docked<P: Page>(tree: Tree<Leaf<P>>, new: Leaf<P>, share: Share) -> Tree<Leaf<P>> {
    let end = Part {
        share,
        tree: Tree::Leaf(new),
    };
    let (part, focus) = match tree {
        Tree::Node(mut n) if n.axis == Axis::Across && n.fit == Fit::Fits => {
            n.after.push(end);
            return Tree::Node(n);
        }
        Tree::Leaf(l) => (l.page().share(), Tree::Leaf(l)),
        Tree::Node(n) => (Share::Two, Tree::Node(n)),
    };
    Tree::Node(Node {
        after: vec![end],
        ..Node::of(
            Axis::Across,
            Fit::Fits,
            Part {
                share: part,
                tree: focus,
            },
        )
    })
}

fn released<P: Page>(tiles: Tiles<P>) -> Tiles<P> {
    let_go(tiles, Where::Everywhere)
}

/// Which workspaces let their pinned ends go: every one, or every one but the focused.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    Everywhere,
    Elsewhere,
}

/// The workspaces `at` names without their pinned ends, but one that is its workspace's focused
/// leaf: the person's, in every workspace.
fn let_go<P: Page>(tiles: Tiles<P>, at: Where) -> Tiles<P> {
    let here = tiles.root.index();
    let root = tiles.root.map_parts(|i, part| {
        let (end, _) = part.tree.end();
        let named = at == Where::Everywhere || i != here;
        match (
            end.page().pin(),
            end.id() == part.tree.focused().id(),
            named,
        ) {
            (Pin::Pinned, false, true) => Part {
                tree: undocked(part.tree),
                ..part
            },
            (Pin::Free, _, _) | (Pin::Pinned, true, _) | (Pin::Pinned, false, false) => part,
        }
    });
    Tiles { root, ..tiles }
}

/// The focused leaf closed. Beside a pinned end where `Hold` docks it, the close is the one the
/// workspace without it makes, and the end is docked again after; a pinned end anywhere else
/// stays where it stands.
fn closed<P: Page>(tiles: Tiles<P>) -> Tiles<P> {
    let shut = |t: Tiles<P>| Tiles {
        root: descend(t.root, close).normal_children(),
        ..t
    };
    let (end, path) = tiles.root.focus.tree.end();
    if end.page().pin() == Pin::Free || end.id() == tiles.focused().id() {
        return shut(tiles);
    }
    let (end, share) = (end.clone(), share_at(&tiles.root.focus.tree, &path));
    let redock = |t: Tiles<P>, end: Leaf<P>| Tiles {
        root: t.root.on_focus(|w| (docked(w, end, share), ())).0,
        ..t
    };
    let without = Tiles {
        root: tiles.root.clone().on_focus(|w| (undocked(w), ())).0,
        ..tiles.clone()
    };
    if redock(without.clone(), end.clone()).root == tiles.root {
        redock(shut(without), end)
    } else {
        shut(tiles)
    }
}

/// The share of the child at `path`, each child's place from the top of `tree`.
fn share_at<L>(tree: &Tree<L>, path: &[usize]) -> Share {
    let mut share = Share::One;
    let mut at = tree;
    for &i in path {
        let Tree::Node(n) = at else { break };
        let Some(part) = n.parts().nth(i) else { break };
        share = part.share;
        at = &part.tree;
    }
    share
}

/// The tree without its end leaf, which is not its focused one: a node whose end it is focuses
/// the child before it, and a node left with one child is replaced by it.
fn undocked<L>(tree: Tree<L>) -> Tree<L> {
    let Tree::Node(mut n) = tree else {
        return tree;
    };
    match n.after.pop() {
        Some(Part {
            tree: Tree::Leaf(_),
            ..
        }) => {}
        Some(Part { share, tree }) => n.after.push(Part {
            share,
            tree: undocked(tree),
        }),
        None => match (*n.focus, n.before.pop()) {
            (
                Part {
                    tree: Tree::Leaf(_),
                    ..
                },
                Some(prev),
            ) => n.focus = Box::new(prev),
            (Part { share, tree }, prev) => {
                n.before.extend(prev);
                n.focus = Box::new(Part {
                    share,
                    tree: undocked(tree),
                });
            }
        },
    }
    Tree::Node(n).normal()
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
