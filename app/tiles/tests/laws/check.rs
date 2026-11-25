//! Readings of an arrangement that the laws compare: the rectangle checker, the drawn set, and
//! least sizes computed apart from the module's own.

use std::collections::BTreeSet;

use tiles::{Arranged, Axis, Fit, Id, Leaf, Measures, Node, Page, Rect, Shape, Tree, Viewport};

use super::page::Pg;

pub fn pieces(a: &Arranged<Pg>) -> Vec<Rect> {
    a.tiles
        .iter()
        .map(|t| t.rect)
        .chain(a.dividers.iter().map(|d| d.rect))
        .chain(a.rows.iter().map(|r| r.rect))
        .collect()
}

fn area(r: Rect) -> u64 {
    u64::from(r.w.0) * u64::from(r.h.0)
}

pub fn inside(r: Rect, outer: Rect) -> bool {
    r.x >= outer.x
        && r.y >= outer.y
        && r.x.0 + r.w.0 <= outer.x.0 + outer.w.0
        && r.y.0 + r.h.0 <= outer.y.0 + outer.h.0
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x.0 < b.x.0 + b.w.0 && b.x.0 < a.x.0 + a.w.0 && a.y.0 < b.y.0 + b.h.0 && b.y.0 < a.y.0 + a.h.0
}

/// `rects` lie inside `outer`, overlap pairwise nowhere, and their areas sum to its area: so
/// they cover it exactly.
pub fn covers(rects: &[Rect], outer: Rect) -> Result<(), String> {
    if let Some(r) = rects.iter().find(|r| !inside(**r, outer)) {
        return Err(format!("{r:?} leaves {outer:?}"));
    }
    for (i, a) in rects.iter().enumerate() {
        if let Some(b) = rects[i + 1..].iter().find(|b| overlap(*a, **b)) {
            return Err(format!("{a:?} overlaps {b:?}"));
        }
    }
    let sum: u64 = rects.iter().map(|r| area(*r)).sum();
    if sum == area(outer) {
        Ok(())
    } else {
        Err(format!("areas sum to {sum}, not {}", area(outer)))
    }
}

pub fn screen(at: Viewport) -> Rect {
    Rect {
        x: tiles::Px(0),
        y: tiles::Px(0),
        w: at.w,
        h: at.h,
    }
}

/// The smallest rectangle around the drawn tiles and the dividers between them.
pub fn bounds(a: &Arranged<Pg>) -> Option<Rect> {
    let all: Vec<Rect> = a
        .tiles
        .iter()
        .map(|t| t.rect)
        .chain(a.dividers.iter().map(|d| d.rect))
        .collect();
    bound(&all)
}

fn bound(all: &[Rect]) -> Option<Rect> {
    let left = all.iter().map(|r| r.x.0).min()?;
    let top = all.iter().map(|r| r.y.0).min()?;
    let right = all.iter().map(|r| r.x.0 + r.w.0).max()?;
    let bottom = all.iter().map(|r| r.y.0 + r.h.0).max()?;
    let px = tiles::Px;
    Some(Rect {
        x: px(left),
        y: px(top),
        w: px(right - left),
        h: px(bottom - top),
    })
}

/// The pieces that meet the region lie inside it and cover it.
pub fn region(a: &Arranged<Pg>) -> Result<(), String> {
    let Some(b) = bounds(a) else {
        return Err("nothing drawn".to_owned());
    };
    let within: Vec<Rect> = pieces(a)
        .into_iter()
        .filter(|r| area(*r) > 0 && overlap(*r, b))
        .collect();
    covers(&within, b)
}

pub fn drawn(a: &Arranged<Pg>) -> BTreeSet<Id> {
    a.tiles.iter().map(|t| t.id).collect()
}

pub fn holds(tree: &Tree<Leaf<Pg>>, drawn: &BTreeSet<Id>) -> bool {
    tree.leaves().iter().any(|l| drawn.contains(&l.id()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

/// What is drawn of `tree` reaches its edge on `side` of `axis`: no leaf behind stands between
/// it and that edge. The children behind a `Never` node are its crumbs, not tiles, so only its
/// focused child counts.
pub fn full(tree: &Tree<Leaf<Pg>>, axis: Axis, side: Side, drawn: &BTreeSet<Id>) -> bool {
    match tree {
        Tree::Leaf(l) => drawn.contains(&l.id()),
        Tree::Node(n) if n.fit == Fit::Never => full(&n.focus.tree, axis, side, drawn),
        Tree::Node(n) => {
            let parts: Vec<_> = n.parts().collect();
            let lit: Vec<usize> = (0..parts.len())
                .filter(|&i| holds(&parts[i].tree, drawn))
                .collect();
            if n.axis == axis {
                let (edge, end) = match side {
                    Side::Before => (lit.first(), 0),
                    Side::After => (lit.last(), parts.len() - 1),
                };
                edge.is_some_and(|&i| i == end && full(&parts[i].tree, axis, side, drawn))
            } else {
                !lit.is_empty() && lit.iter().all(|&i| full(&parts[i].tree, axis, side, drawn))
            }
        }
    }
}

/// Each node's drawn children are consecutive, and nothing behind stands between two of them.
pub fn nothing_between(n: &Node<Leaf<Pg>>, drawn: &BTreeSet<Id>) -> Result<(), String> {
    let parts: Vec<_> = n.parts().collect();
    let lit: Vec<usize> = (0..parts.len())
        .filter(|&i| holds(&parts[i].tree, drawn))
        .collect();
    if lit.windows(2).any(|w| w[1] != w[0] + 1) {
        return Err(format!("drawn children {lit:?} are not consecutive"));
    }
    if n.fit == Fit::Fits {
        for w in lit.windows(2) {
            let (a, b) = (&parts[w[0]].tree, &parts[w[1]].tree);
            if !full(a, n.axis, Side::After, drawn) || !full(b, n.axis, Side::Before, drawn) {
                return Err(format!(
                    "a leaf behind stands between children {} and {}",
                    w[0], w[1]
                ));
            }
        }
    }
    for p in parts {
        if let Tree::Node(c) = &p.tree {
            nothing_between(c, drawn)?;
        }
    }
    Ok(())
}

/// A leaf's least width and height.
pub fn least_of(p: &Pg, m: &Measures) -> (u64, u64) {
    match p.shape() {
        Shape::Flows { least } => (u64::from(least.0), u64::from(m.tall.0)),
        Shape::Paged { least, ratio } => (
            u64::from(least.0),
            (u64::from(least.0) * u64::from(ratio.h.get())).div_ceil(u64::from(ratio.w.get())),
        ),
    }
}

fn combine(all: &[(u64, u64)], axis: Axis, m: &Measures) -> (u64, u64) {
    let gaps = u64::from(m.divider.0) * (all.len().max(1) as u64 - 1);
    let (sum_w, sum_h): (u64, u64) = (all.iter().map(|s| s.0).sum(), all.iter().map(|s| s.1).sum());
    let (max_w, max_h) = (
        all.iter().map(|s| s.0).max().unwrap_or(0),
        all.iter().map(|s| s.1).max().unwrap_or(0),
    );
    match axis {
        Axis::Across => (sum_w + gaps, max_h),
        Axis::Down => (max_w, sum_h + gaps),
    }
}

/// The least extent of a tree drawn whole, by the rule of the design note.
pub fn least(tree: &Tree<Leaf<Pg>>, m: &Measures) -> (u64, u64) {
    match tree {
        Tree::Leaf(l) => least_of(l.page(), m),
        Tree::Node(n) if n.fit == Fit::Never => {
            let (w, h) = least(&n.focus.tree, m);
            (w, h + if n.len() > 1 { u64::from(m.row.0) } else { 0 })
        }
        Tree::Node(n) => {
            let all: Vec<_> = n.parts().map(|p| least(&p.tree, m)).collect();
            combine(&all, n.axis, m)
        }
    }
}

/// The least extent of what is drawn of a tree: its drawn children, and a row where a child is
/// behind.
pub fn extent(tree: &Tree<Leaf<Pg>>, lit: &BTreeSet<Id>, m: &Measures) -> (u64, u64) {
    match tree {
        Tree::Leaf(l) => least_of(l.page(), m),
        Tree::Node(n) => {
            let drawn: Vec<_> = n
                .parts()
                .filter(|p| holds(&p.tree, lit))
                .map(|p| extent(&p.tree, lit, m))
                .collect();
            let (w, h) = combine(&drawn, n.axis, m);
            let row = if drawn.len() < n.len() {
                u64::from(m.row.0)
            } else {
                0
            };
            (w, h + row)
        }
    }
}

/// The rectangle a child at `path` was given: around its tiles, and the rows and dividers of
/// its nodes.
pub fn given(a: &Arranged<Pg>, tree: &Tree<Leaf<Pg>>, path: &[usize]) -> Option<Rect> {
    let ids: BTreeSet<Id> = tree.leaves().iter().map(|l| l.id()).collect();
    let all: Vec<Rect> = a
        .tiles
        .iter()
        .filter(|t| ids.contains(&t.id))
        .map(|t| t.rect)
        .chain(
            a.rows
                .iter()
                .filter(|r| r.node.starts_with(path))
                .map(|r| r.rect),
        )
        .chain(
            a.dividers
                .iter()
                .filter(|d| d.node.starts_with(path))
                .map(|d| d.rect),
        )
        .collect();
    bound(&all)
}

/// The tree is drawn whole: every child of a `Fits` node, the focused child of a `Never` node.
pub fn whole(tree: &Tree<Leaf<Pg>>, drawn: &BTreeSet<Id>) -> bool {
    match tree {
        Tree::Leaf(l) => drawn.contains(&l.id()),
        Tree::Node(n) if n.fit == Fit::Never => whole(&n.focus.tree, drawn),
        Tree::Node(n) => n.parts().all(|p| whole(&p.tree, drawn)),
    }
}
