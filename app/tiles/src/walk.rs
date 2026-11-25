//! Which leaves are drawn: from the focused leaf up through its ancestors, adding what fits.

use super::Tile;
use super::measures::Measures;
use super::node::{Axis, Fit, Node};
use super::px::{Px, Size, Viewport};
use super::tree::Tree;

/// The drawn part of a tree: a leaf, or a node with its drawn children, which are the
/// consecutive children from `first` on.
pub(crate) enum Drawn<'t, L> {
    Leaf(&'t L),
    Node {
        node: &'t Node<L>,
        first: usize,
        kids: Kids<Drawn<'t, L>>,
    },
}

/// One or more, in order.
pub(crate) struct Kids<T> {
    head: Box<T>,
    tail: Vec<T>,
}

/// At a `Never` node nothing is added. At a `Fits` node the children after the drawn region are
/// tried, then those before it, each side nearest first: a child is added if the region with it
/// fits the viewport, less the rows of the nodes above, on both axes. A node on the same axis
/// that fits only in part is added by its nearest children, tried the same way. A side shuts at
/// the first child not added whole, at that node and every node above it. A `Fits` node that
/// leaves a child behind draws a row, so its walk is tried again with that row's height taken:
/// every drawn tile then has its least size, but a focused leaf drawn alone.
pub(crate) fn walk<'t, L: Tile>(root: &'t Node<L>, at: Viewport, m: &Measures) -> Drawn<'t, L> {
    let cx = Cx {
        at: Size { w: at.w, h: at.h },
        m,
    };
    node_path(root, Px(0), &cx).drawn
}

struct Cx<'m> {
    at: Size,
    m: &'m Measures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

struct Walked<'t, L> {
    drawn: Drawn<'t, L>,
    shut: Vec<(Axis, Side)>,
}

enum Grown<'t, L> {
    Whole(Drawn<'t, L>, Size),
    Part(Drawn<'t, L>, Size),
    Behind,
}

fn path<'t, L: Tile>(tree: &'t Tree<L>, above: Px, cx: &Cx) -> Walked<'t, L> {
    match tree {
        Tree::Leaf(l) => Walked {
            drawn: Drawn::Leaf(l),
            shut: Vec::new(),
        },
        Tree::Node(n) => node_path(n, above, cx),
    }
}

fn node_path<'t, L: Tile>(n: &'t Node<L>, above: Px, cx: &Cx) -> Walked<'t, L> {
    match n.fit {
        Fit::Never => {
            let row = if n.len() > 1 { cx.m.row } else { Px(0) };
            let inner = path(&n.focus.tree, above + row, cx);
            Walked {
                drawn: Drawn::Node {
                    node: n,
                    first: n.index(),
                    kids: Kids::one(inner.drawn),
                },
                shut: inner.shut,
            }
        }
        Fit::Fits => {
            let open = fits_walk(n, above, Px(0), cx);
            match &open.drawn {
                Drawn::Node { kids, .. } if kids.len() < n.len() => {
                    fits_walk(n, above, cx.m.row, cx)
                }
                Drawn::Node { .. } | Drawn::Leaf(_) => open,
            }
        }
    }
}

/// A `Fits` node's walk with `row` taken from the height of its rectangle.
fn fits_walk<'t, L: Tile>(n: &'t Node<L>, above: Px, row: Px, cx: &Cx) -> Walked<'t, L> {
    let inner = path(&n.focus.tree, above + row, cx);
    let avail = Size {
        w: cx.at.w,
        h: cx.at.h - above,
    };
    let mut need = inner.drawn.extent(cx.m);
    let mut shut = inner.shut;
    let mut kids = Kids::one(inner.drawn);
    let mut first = n.index();
    let sides: [(Side, Vec<_>); 2] = [
        (Side::After, n.after.iter().collect()),
        (Side::Before, n.before.iter().rev().collect()),
    ];
    for (side, parts) in sides {
        if shut.contains(&(n.axis, side)) {
            continue;
        }
        for part in parts {
            let region = need;
            let ctx = move |e: Size| with_row(join(region, e, n.axis, cx.m), row);
            let (d, e, whole) = match grow(&part.tree, n.axis, side, &ctx, avail, cx) {
                Grown::Whole(d, e) => (d, e, true),
                Grown::Part(d, e) => (d, e, false),
                Grown::Behind => {
                    shut.push((n.axis, side));
                    break;
                }
            };
            need = join(need, e, n.axis, cx.m);
            match side {
                Side::After => kids.push(d),
                Side::Before => {
                    kids.prepend(d);
                    first -= 1;
                }
            }
            if !whole {
                shut.push((n.axis, side));
                break;
            }
        }
    }
    Walked {
        drawn: Drawn::Node {
            node: n,
            first,
            kids,
        },
        shut,
    }
}

/// A child tried on one side of the region; `ctx` gives the region's extent with the child's.
fn grow<'t, L: Tile>(
    tree: &'t Tree<L>,
    axis: Axis,
    side: Side,
    ctx: &dyn Fn(Size) -> Size,
    avail: Size,
    cx: &Cx,
) -> Grown<'t, L> {
    let whole = whole(tree);
    let e = whole.extent(cx.m);
    if fits(ctx(e), avail) {
        return Grown::Whole(whole, e);
    }
    let Tree::Node(n) = tree else {
        return Grown::Behind;
    };
    if n.fit == Fit::Never || n.axis != axis {
        return Grown::Behind;
    }
    let parts: Vec<_> = match side {
        Side::After => n.parts().collect(),
        Side::Before => n.parts().collect::<Vec<_>>().into_iter().rev().collect(),
    };
    let row = cx.m.row;
    let mut taken: Option<(Kids<Drawn<'t, L>>, Size)> = None;
    for part in parts {
        let so_far = taken.as_ref().map(|(_, s)| *s);
        let inner = move |e: Size| ctx(with_row(and(so_far, e, axis, cx.m), row));
        let (d, e, whole) = match grow(&part.tree, axis, side, &inner, avail, cx) {
            Grown::Whole(d, e) => (d, e, true),
            Grown::Part(d, e) => (d, e, false),
            Grown::Behind => break,
        };
        taken = Some(match taken {
            None => (Kids::one(d), e),
            Some((mut kids, s)) => {
                match side {
                    Side::After => kids.push(d),
                    Side::Before => kids.prepend(d),
                }
                (kids, join(s, e, axis, cx.m))
            }
        });
        if !whole {
            break;
        }
    }
    let Some((kids, s)) = taken else {
        return Grown::Behind;
    };
    let first = match side {
        Side::After => 0,
        Side::Before => n.len() - kids.len(),
    };
    Grown::Part(
        Drawn::Node {
            node: n,
            first,
            kids,
        },
        with_row(s, row),
    )
}

/// The tree drawn whole: every child of a `Fits` node, the focused child of a `Never` node.
fn whole<L>(tree: &Tree<L>) -> Drawn<'_, L> {
    match tree {
        Tree::Leaf(l) => Drawn::Leaf(l),
        Tree::Node(n) => match n.fit {
            Fit::Fits => {
                let mut kids = Kids::one(whole(&n.focus.tree));
                n.before
                    .iter()
                    .rev()
                    .for_each(|p| kids.prepend(whole(&p.tree)));
                n.after.iter().for_each(|p| kids.push(whole(&p.tree)));
                Drawn::Node {
                    node: n,
                    first: 0,
                    kids,
                }
            }
            Fit::Never => Drawn::Node {
                node: n,
                first: n.index(),
                kids: Kids::one(whole(&n.focus.tree)),
            },
        },
    }
}

impl<L: Tile> Drawn<'_, L> {
    /// The least extent of what is drawn: the leaves' leasts, the dividers between drawn
    /// children, and a row on each node with children behind.
    pub(crate) fn extent(&self, m: &Measures) -> Size {
        match self {
            Drawn::Leaf(l) => {
                let (w, h) = l.shape().least(m.tall);
                Size { w, h }
            }
            Drawn::Node { node, kids, .. } => {
                let inner = kids.tail.iter().fold(kids.head.extent(m), |acc, k| {
                    join(acc, k.extent(m), node.axis, m)
                });
                if kids.len() < node.len() {
                    with_row(inner, m.row)
                } else {
                    inner
                }
            }
        }
    }
}

impl<'t, L> Drawn<'t, L> {
    /// The drawn leaves, in order.
    pub(crate) fn leaves(&self) -> Vec<&'t L> {
        match self {
            Drawn::Leaf(l) => vec![*l],
            Drawn::Node { kids, .. } => kids.iter().flat_map(Drawn::leaves).collect(),
        }
    }
}

impl<T> Kids<T> {
    fn one(head: T) -> Kids<T> {
        Kids {
            head: Box::new(head),
            tail: Vec::new(),
        }
    }

    fn push(&mut self, t: T) {
        self.tail.push(t);
    }

    fn prepend(&mut self, t: T) {
        let old = std::mem::replace(&mut *self.head, t);
        self.tail.insert(0, old);
    }

    pub(crate) fn len(&self) -> usize {
        1 + self.tail.len()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&*self.head).chain(&self.tail)
    }

    pub(crate) fn first(&self) -> &T {
        &self.head
    }

    pub(crate) fn last(&self) -> &T {
        self.tail.last().unwrap_or(&*self.head)
    }
}

fn fits(need: Size, avail: Size) -> bool {
    need.w <= avail.w && need.h <= avail.h
}

/// Two extents side by side on `axis`, a divider between them.
fn join(a: Size, b: Size, axis: Axis, m: &Measures) -> Size {
    let d = m.divider;
    match axis {
        Axis::Across => Size {
            w: a.w + d + b.w,
            h: a.h.max(b.h),
        },
        Axis::Down => Size {
            w: a.w.max(b.w),
            h: a.h + d + b.h,
        },
    }
}

fn and(a: Option<Size>, b: Size, axis: Axis, m: &Measures) -> Size {
    match a {
        Some(a) => join(a, b, axis, m),
        None => b,
    }
}

fn with_row(s: Size, row: Px) -> Size {
    Size { h: s.h + row, ..s }
}
