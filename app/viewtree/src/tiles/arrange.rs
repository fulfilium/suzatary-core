//! The arrangement: a function of the tiles and the viewport alone.

use super::apportion::apportion;
use super::arranged::{Arranged, At, Crumb, Divider, Opens, Placed, Row, Scroller, Stop};
use super::leaf::{Id, Leaf};
use super::node::{Axis, Node, Share};
use super::open;
use super::page::{Page, Pin, Shape};
use super::px::{Edge, Px, Rect, Size, Viewport};
use super::walk::{Drawn, walk};
use super::{Tile, Tiles};

/// The lengths the arrangement reads beside the pages' own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measures {
    /// Between two drawn children.
    pub divider: Px,
    /// A crumb row's height.
    pub row: Px,
    /// A crumb's width in its row; zero shows every crumb.
    pub crumb: Px,
    /// Every flowing page's least height.
    pub tall: Px,
    /// A viewport narrower than this draws the root's row at its foot, as a phone draws its bar.
    pub foot: Px,
}

/// The drawn leaves (`walk`) laid out top down: a node with children behind gives its crumb row
/// the top of its rectangle, and divides the rest along its axis among its drawn children
/// (`apportion`), a divider between each two. A viewport smaller than a leaf's least still draws
/// the leaf, at the viewport's size.
#[must_use]
pub fn arrange<P: Page>(tiles: &Tiles<P>, at: Viewport, m: &Measures) -> Arranged<P> {
    let drawn = walk(&tiles.root, at, m);
    let cx = Cx {
        root: &tiles.root,
        at,
        m,
    };
    let mut out = Arranged {
        tiles: Vec::new(),
        dividers: Vec::new(),
        rows: Vec::new(),
    };
    let whole = Rect {
        x: Px(0),
        y: Px(0),
        w: at.w,
        h: at.h,
    };
    let edge = if at.w < m.foot { Edge::Foot } else { Edge::Top };
    place(&drawn, whole, edge, &mut Vec::new(), &cx, &mut out);
    if let [only] = out.tiles.as_mut_slice() {
        only.scroll = Scroller::Document;
    }
    out
}

/// A node's drawn children as `place` divides them: the start and length of its body along its
/// axis, and each child's least and share.
struct Split {
    start: Px,
    length: Px,
    kids: Vec<(Px, Share)>,
}

impl Split {
    /// The places of the divider after child `j`: its place now first, then every pair of shares
    /// for it and the child after it that gives no child less than its least; one per place, in
    /// order along the axis, with the index of its place now.
    fn stops(&self, j: usize, now: [Share; 2], d: Px) -> (Vec<Stop>, usize) {
        let all = [Share::One, Share::Two, Share::Three];
        let mut pairs: Vec<[Share; 2]> = all
            .iter()
            .flat_map(|&x| all.iter().map(move |&y| [x, y]))
            .collect();
        pairs.sort_by_key(|p| p[0].weight() + p[1].weight());
        let mut out: Vec<Stop> = Vec::new();
        let mut here = Px(0);
        for (shares, current) in
            std::iter::once((now, true)).chain(pairs.into_iter().map(|p| (p, false)))
        {
            let kids: Vec<(Px, u32)> = self
                .kids
                .iter()
                .enumerate()
                .map(|(k, &(least, share))| {
                    let share = match k {
                        k if k == j => shares[0],
                        k if k == j + 1 => shares[1],
                        _ => share,
                    };
                    (least, share.weight())
                })
                .collect();
            let (sizes, gaps) = lengths(self.length, &kids, d);
            if !current && binds(&sizes, &kids) {
                continue;
            }
            let at = sizes
                .iter()
                .take(j + 1)
                .chain(gaps.iter().take(j))
                .fold(self.start, |a, &l| a + l);
            if current {
                here = at;
            }
            if out.iter().all(|s| s.at != at) {
                out.push(Stop { at, shares });
            }
        }
        out.sort_by_key(|s| s.at);
        let now = out.iter().filter(|s| s.at < here).count();
        (out, now)
    }
}

/// Whether a child's share of the length the children divide is under its least.
fn binds(sizes: &[Px], kids: &[(Px, u32)]) -> bool {
    let inner: u128 = sizes.iter().map(|l| u128::from(l.0)).sum();
    let total: u128 = kids.iter().map(|&(_, w)| u128::from(w)).sum();
    kids.iter()
        .any(|&(least, w)| inner * u128::from(w) < u128::from(least.0) * total)
}

struct Cx<'t, 'm, P> {
    root: &'t Node<Leaf<P>>,
    at: Viewport,
    m: &'m Measures,
}

fn place<P: Page>(
    drawn: &Drawn<'_, Leaf<P>>,
    rect: Rect,
    edge: Edge,
    path: &mut Vec<usize>,
    cx: &Cx<'_, '_, P>,
    out: &mut Arranged<P>,
) {
    let m = cx.m;
    match drawn {
        Drawn::Leaf(l) => out.tiles.push(Placed {
            id: l.id(),
            page: l.page().clone(),
            rect,
            fill: l.page().shape().fill(rect),
            scroll: Scroller::Tile,
            opens: opens(cx, l),
        }),
        Drawn::Node { node, first, kids } => {
            let body = if kids.len() < node.len() {
                let (band, body) = rect.band(m.row, edge);
                out.rows.push(row(node, *first, kids.len(), band, path, m));
                body
            } else {
                rect
            };
            let shares: Vec<Share> = node
                .parts()
                .skip(*first)
                .take(kids.len())
                .map(|p| p.share)
                .collect();
            let leasts: Vec<(Px, u32)> = kids
                .iter()
                .zip(shares.iter().copied())
                .map(|(k, s)| (along(k.extent(m), node.axis), Share::weight(s)))
                .collect();
            let (sizes, gaps) = lengths(length(body, node.axis), &leasts, m.divider);
            let split = Split {
                start: start(body, node.axis),
                length: length(body, node.axis),
                kids: leasts.iter().map(|&(l, _)| l).zip(shares).collect(),
            };
            let kids: Vec<_> = kids.iter().collect();
            let mut at = Px(0);
            for (j, (kid, size)) in kids.iter().zip(sizes).enumerate() {
                path.push(first + j);
                place(
                    kid,
                    cut(body, node.axis, at, size),
                    Edge::Top,
                    path,
                    cx,
                    out,
                );
                path.pop();
                at = at + size;
                if let (Some(next), Some(&gap), Some(&(_, a)), Some(&(_, b))) = (
                    kids.get(j + 1),
                    gaps.get(j),
                    split.kids.get(j),
                    split.kids.get(j + 1),
                ) {
                    let (stops, now) = split.stops(j, [a, b], m.divider);
                    out.dividers.push(Divider {
                        rect: cut(body, node.axis, at, gap),
                        node: path.clone(),
                        axis: node.axis,
                        before: nearest(kid, End::Last),
                        after: nearest(next, End::First),
                        stops,
                        now,
                    });
                    at = at + gap;
                }
            }
        }
    }
}

/// The children's lengths, and the dividers' between them: all sum to `length`. Each divider is
/// `d` when the length holds them all, and the children get nothing otherwise.
fn lengths(length: Px, children: &[(Px, u32)], d: Px) -> (Vec<Px>, Vec<Px>) {
    let n = children.len().saturating_sub(1);
    let gaps: u64 = std::iter::repeat_n(u64::from(d.0), n).sum();
    if u64::from(length.0) >= gaps {
        (
            apportion(length - Px::saturating(gaps), children),
            vec![d; n],
        )
    } else {
        (
            vec![Px(0); children.len()],
            apportion(length, &vec![(Px(0), 1); n]),
        )
    }
}

fn row<P: Clone>(
    node: &Node<Leaf<P>>,
    first: usize,
    drawn: usize,
    rect: Rect,
    path: &[usize],
    m: &Measures,
) -> Row<P> {
    let crumbs: Vec<Crumb<P>> = node
        .parts()
        .enumerate()
        .map(|(i, p)| {
            let at = if i < first {
                At::Before
            } else if i < first + drawn {
                At::Drawn
            } else {
                At::After
            };
            let leaf = p.tree.focused();
            Crumb {
                to: leaf.id(),
                page: leaf.page().clone(),
                at,
            }
        })
        .collect();
    let mut slots = 0;
    let mut room = rect.w;
    while slots < crumbs.len() && (m.crumb == Px(0) || m.crumb <= room) {
        room = room - m.crumb;
        slots += 1;
    }
    let (shown, more) = if crumbs.len() <= slots {
        (crumbs, Vec::new())
    } else {
        let mut shown = crumbs;
        let more = shown.split_off(slots.saturating_sub(1));
        (shown, more)
    };
    Row {
        rect,
        node: path.to_vec(),
        axis: node.axis,
        shown,
        more,
    }
}

/// Whether a page the tile opens would be drawn beside it: the walk over the tree that the open
/// would make, with a ghost of the shape the tile's page opens.
fn opens<P: Page>(cx: &Cx<'_, '_, P>, tile: &Leaf<P>) -> Opens {
    let id = tile.id();
    let probe = open::origin(open::focus(cx.root.map(&Probe::Tile), id));
    let ghost = Probe::Ghost(tile.page().opens());
    let grown = open::bounded(probe, ghost, |root, ghost| {
        open::beside(root, ghost, Share::One)
    });
    let beside = walk(&grown, cx.at, cx.m)
        .leaves()
        .iter()
        .any(|l| l.id() == Some(id));
    if beside { Opens::Beside } else { Opens::Over }
}

/// A leaf of the tree an open would make: a tile, or the page the open adds.
#[derive(Clone)]
enum Probe<'t, P> {
    Tile(&'t Leaf<P>),
    Ghost(Shape),
}

impl<P: Page> Tile for Probe<'_, P> {
    fn id(&self) -> Option<Id> {
        match self {
            Probe::Tile(l) => Some(l.id()),
            Probe::Ghost(_) => None,
        }
    }
    fn pin(&self) -> Pin {
        match self {
            Probe::Tile(l) => l.page().pin(),
            Probe::Ghost(_) => Pin::Free,
        }
    }
    fn share(&self) -> Share {
        match self {
            Probe::Tile(l) => l.page().share(),
            Probe::Ghost(_) => Share::One,
        }
    }
    fn shape(&self) -> Shape {
        match self {
            Probe::Tile(l) => l.page().shape(),
            Probe::Ghost(s) => *s,
        }
    }
}

#[derive(Clone, Copy)]
enum End {
    First,
    Last,
}

/// The drawn leaf of the child nearest the divider on `end`'s side of it.
fn nearest<P>(drawn: &Drawn<'_, Leaf<P>>, end: End) -> Id {
    match drawn {
        Drawn::Leaf(l) => l.id(),
        Drawn::Node { kids, .. } => match end {
            End::First => nearest(kids.first(), end),
            End::Last => nearest(kids.last(), end),
        },
    }
}

fn along(s: Size, axis: Axis) -> Px {
    match axis {
        Axis::Across => s.w,
        Axis::Down => s.h,
    }
}

fn start(r: Rect, axis: Axis) -> Px {
    match axis {
        Axis::Across => r.x,
        Axis::Down => r.y,
    }
}

fn length(r: Rect, axis: Axis) -> Px {
    match axis {
        Axis::Across => r.w,
        Axis::Down => r.h,
    }
}

/// The part of `r` from `at` to `at + len` along `axis`.
fn cut(r: Rect, axis: Axis, at: Px, len: Px) -> Rect {
    match axis {
        Axis::Across => Rect {
            x: r.x + at,
            w: len,
            ..r
        },
        Axis::Down => Rect {
            y: r.y + at,
            h: len,
            ..r
        },
    }
}
