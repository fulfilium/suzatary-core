//! The arrangement: a function of the tiles and the viewport alone.

use super::apportion::apportion;
use super::arranged::{Arranged, At, Crumb, Divider, Opens, Placed, Row, Scroller};
use super::leaf::{Id, Leaf};
use super::node::{Axis, Node, Share};
use super::open::{self, MOST};
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
pub fn arrange<P: Page>(tiles: &Tiles<P>, at: Viewport, m: &Measures) -> Arranged {
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
    out: &mut Arranged,
) {
    let m = cx.m;
    match drawn {
        Drawn::Leaf(l) => out.tiles.push(Placed {
            id: l.id(),
            rect,
            page: l.page().shape().fill(rect),
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
            let shares = node.parts().skip(*first).map(|p| p.share);
            let leasts: Vec<(Px, u32)> = kids
                .iter()
                .zip(shares)
                .map(|(k, s)| (along(k.extent(m), node.axis), Share::weight(s)))
                .collect();
            let (sizes, gaps) = lengths(length(body, node.axis), &leasts, m.divider);
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
                if let (Some(next), Some(&gap)) = (kids.get(j + 1), gaps.get(j)) {
                    out.dividers.push(Divider {
                        rect: cut(body, node.axis, at, gap),
                        node: path.clone(),
                        axis: node.axis,
                        before: nearest(kid, End::Last),
                        after: nearest(next, End::First),
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

fn row<P>(
    node: &Node<Leaf<P>>,
    first: usize,
    drawn: usize,
    rect: Rect,
    path: &[usize],
    m: &Measures,
) -> Row {
    let crumbs: Vec<Crumb> = node
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
            Crumb {
                to: p.tree.focused().id(),
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
    let probe = open::focus(cx.root.map(&Probe::Tile), id);
    let was = open::tiles(&probe);
    let grown = open::beside(probe, Probe::Ghost(tile.page().opens()), Share::One);
    let now = open::tiles(&grown);
    if now > was && now > MOST {
        return Opens::Over;
    }
    let beside = walk(&grown, cx.at, cx.m)
        .leaves()
        .iter()
        .any(|l| l.id() == Some(id));
    if beside { Opens::Beside } else { Opens::Over }
}

/// A leaf of the tree an open would make: a tile, or the page the open adds.
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
