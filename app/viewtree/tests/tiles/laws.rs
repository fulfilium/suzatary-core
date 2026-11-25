//! The laws of the layout, each over runs of acts, measures and viewports on generated trees of
//! any shape.

use std::cell::Cell;

use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, TestRunner};
use viewtree::tiles::{
    Act, Fit, Leaf, Node, Page, Pin, Scroller, Share, Tiles, Tree, act, arrange, parse, print,
};

use super::check::{
    bounds, covers, drawn, extent, given, holds, inside, least, least_of, nothing_between, pieces,
    region, screen, whole,
};
use super::page::Pg;
use super::runs::{ids, page, run, states, viewport};

/// The count a law over runs affords in the suite; `PROPTEST_CASES` raises it for one run. A
/// failing seed is kept beside this file.
pub(super) fn cases(n: u32) -> ProptestConfig {
    let n = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(n);
    ProptestConfig {
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))),
        ..ProptestConfig::with_cases(n)
    }
}

fn workspaces(t: &Tiles<Pg>) -> Vec<Vec<&Leaf<Pg>>> {
    t.root().parts().map(|p| p.tree.leaves()).collect()
}

fn pages(t: &Tiles<Pg>) -> Node<&Pg> {
    t.root().map(&Leaf::page)
}

/// Every node of the tree with its place, each child's index from the root down.
fn nodes<'t>(
    n: &'t Node<Leaf<Pg>>,
    path: &mut Vec<usize>,
    out: &mut Vec<(&'t Node<Leaf<Pg>>, Vec<usize>)>,
) {
    out.push((n, path.clone()));
    for (i, p) in n.parts().enumerate() {
        if let Tree::Node(c) = &p.tree {
            path.push(i);
            nodes(c, path, out);
            path.pop();
        }
    }
}

fn all_nodes(t: &Tiles<Pg>) -> Vec<(&Node<Leaf<Pg>>, Vec<usize>)> {
    let mut out = Vec::new();
    nodes(t.root(), &mut Vec::new(), &mut out);
    out
}

/// The places of the nodes on the focus path, the root first.
fn focus_path(t: &Tiles<Pg>) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    let mut n = t.root();
    let mut path = Vec::new();
    while let Tree::Node(c) = &n.focus.tree {
        path.push(n.index());
        out.push(path.clone());
        n = c;
    }
    out
}

proptest! {
    #![proptest_config(cases(256))]

    /// The tiles, the dividers and the rows cover the viewport exactly, with no overlap, and each
    /// drawn leaf is drawn once; a paged leaf's page stands inside its tile.
    #[test]
    fn arrange_covers_the_viewport_exactly((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            prop_assert_eq!(covers(&pieces(&a), screen(at)), Ok(()));
            prop_assert_eq!(drawn(&a).len(), a.tiles.len());
            for p in &a.tiles {
                prop_assert!(inside(p.fill, p.rect));
            }
        }
    }

    /// Every tile and every crumb carries the page of the leaf it names.
    #[test]
    fn every_tile_and_crumb_carries_its_leafs_page((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            for p in &a.tiles {
                prop_assert_eq!(t.page(p.id), Some(&p.page));
            }
            for c in a.rows.iter().flat_map(|r| r.shown.iter().chain(&r.more)) {
                prop_assert_eq!(t.page(c.to), Some(&c.page));
            }
        }
    }

    /// The drawn region is a rectangle, and no leaf behind stands between two drawn leaves.
    #[test]
    fn the_drawn_region_is_a_rectangle_with_nothing_behind_between((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            prop_assert_eq!(region(&a), Ok(()));
            prop_assert_eq!(nothing_between(t.root(), &drawn(&a)), Ok(()));
        }
    }

    /// The focused leaf is drawn on every viewport.
    #[test]
    fn the_focused_leaf_is_drawn((home, m, first, steps) in run(), more in prop::collection::vec(viewport(), 1..4)) {
        for (_, _, t, at) in states(home, first, &steps) {
            for v in more.iter().chain([&at]) {
                prop_assert!(drawn(&arrange(&t, *v, &m)).contains(&t.focused().id()));
            }
        }
    }

    /// Focusing any leaf, then arranging, draws it.
    #[test]
    fn every_leaf_is_reachable((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            for id in ids(&t) {
                let focused = act(t.clone(), Act::Focus(id));
                prop_assert!(drawn(&arrange(&focused, at, &m)).contains(&id));
            }
        }
    }

    /// Every drawn tile is at least its page's least width and height, but a focused leaf drawn
    /// alone, which takes what the viewport gives.
    #[test]
    fn every_drawn_tile_has_its_least_size_but_a_focused_leaf_alone((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            if a.tiles.len() == 1 {
                continue;
            }
            for p in &a.tiles {
                let (w, h) = least_of(&p.page, &m);
                prop_assert!(u64::from(p.rect.w.0) >= w && u64::from(p.rect.h.0) >= h, "{:?} under {}x{}", p.rect, w, h);
            }
        }
    }

    /// Every `Never` node with a drawn leaf draws its focused child alone. A `Fits` node drawn in
    /// part is on the focus path, or a child of a `Fits` node of its own axis; and one on the focus
    /// path whose children all fit, beside a row for every node above it of two children or more,
    /// draws them all.
    #[test]
    fn a_never_node_draws_its_focused_child_and_a_node_that_fits_draws_all((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            let lit = drawn(&a);
            let on = focus_path(&t);
            let all = all_nodes(&t);
            for (n, path) in &all {
                let tree = Tree::Node((*n).clone());
                if !holds(&tree, &lit) {
                    continue;
                }
                match n.fit {
                    Fit::Never => {
                        let shown: Vec<usize> = n.parts().enumerate().filter(|(_, p)| holds(&p.tree, &lit)).map(|(i, _)| i).collect();
                        prop_assert_eq!(shown, vec![n.index()]);
                    }
                    Fit::Fits => {
                        let (w, h) = least(&tree, &m);
                        let rows: u64 = all
                            .iter()
                            .filter(|(q, p)| p.len() < path.len() && path.starts_with(p) && q.len() > 1)
                            .map(|_| u64::from(m.row.0))
                            .sum();
                        let room = u64::from(at.h.0).saturating_sub(rows);
                        if on.contains(path) && w <= u64::from(at.w.0) && h <= room {
                            prop_assert!(whole(&tree, &lit));
                        }
                        if !whole(&tree, &lit) && !on.contains(path) {
                            let parent = all.iter().find(|(_, q)| q.len() + 1 == path.len() && path.starts_with(q));
                            prop_assert!(parent.is_some_and(|(p, _)| p.fit == Fit::Fits && p.axis == n.axis));
                        }
                    }
                }
            }
        }
    }

    /// Where no least size binds, each drawn child of a node gets its share of the node's length,
    /// to the pixel.
    #[test]
    fn lengths_follow_the_shares_where_no_least_binds((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            let lit = drawn(&a);
            for (n, path) in all_nodes(&t) {
                let kids: Vec<(usize, &Tree<Leaf<Pg>>, Share)> = n.parts().enumerate().filter(|(_, p)| holds(&p.tree, &lit)).map(|(i, p)| (i, &p.tree, p.share)).collect();
                if kids.len() < 2 {
                    continue;
                }
                let along = |r: viewtree::tiles::Rect, e: (u64, u64)| match n.axis {
                    viewtree::tiles::Axis::Across => (u64::from(r.w.0), e.0),
                    viewtree::tiles::Axis::Down => (u64::from(r.h.0), e.1),
                };
                let mut lens = Vec::new();
                for (i, tree, s) in &kids {
                    let mut at_child = path.clone();
                    at_child.push(*i);
                    let r = given(&a, tree, &at_child).expect("a drawn child has a rectangle");
                    let (len, least) = along(r, extent(tree, &lit, &m));
                    lens.push((len, least, u64::from(weight(*s))));
                }
                let total: u64 = lens.iter().map(|l| l.0).sum();
                let shares: u64 = lens.iter().map(|l| l.2).sum();
                if lens.iter().all(|&(_, least, s)| total * s >= least * shares) {
                    for &(len, _, s) in &lens {
                        prop_assert!((len * shares).abs_diff(total * s) < shares, "{} of {} at share {}/{}", len, total, s, shares);
                    }
                }
            }
        }
    }

    /// A tile `Opens::Beside` exactly when opening, from it, a page of the shape it declares that
    /// its workspace does not show, and of another kind than the leaf the open is made from,
    /// draws the tile and that page together.
    #[test]
    fn opens_says_whether_an_open_from_the_tile_is_drawn_beside_it((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            for p in arrange(&t, at, &m).tiles {
                let page = p.page.clone();
                let from = act(t.clone(), Act::Focus(p.id));
                // A page the workspace shows already is focused, not opened; one of the kind of the
                // free leaf a pinned tile opens from replaces that leaf.
                if from.root().focus.tree.leaves().iter().any(|l| *l.page() == page.opened()) {
                    continue;
                }
                let origin = match page.pin() {
                    Pin::Pinned => super::dock::before_end(&from).and_then(|id| from.page(id)),
                    Pin::Free => None,
                };
                if origin.is_some_and(|o| o.kind() == page.opened().kind()) {
                    continue;
                }
                let opened = act(from.clone(), Act::Open(page.opened()));
                let before = ids(&from);
                let new = ids(&opened).into_iter().find(|id| !before.contains(id));
                let lit = drawn(&arrange(&opened, at, &m));
                let together = lit.contains(&p.id) && new.is_some_and(|id| lit.contains(&id));
                prop_assert_eq!(p.opens == viewtree::tiles::Opens::Beside, together);
            }
        }
    }

    /// An open of a free page keeps every workspace's pinned leaves; an open of a pinned page of
    /// another kind makes it the focused workspace's end, focused, and leaves the other
    /// workspaces as they were.
    #[test]
    fn an_open_keeps_the_pinned_leaves((home, _m, first, steps) in run(), p in page()) {
        for (_, _, t, _) in states(home, first, &steps) {
            let opened = act(t.clone(), Act::Open(p.clone()));
            let pinned = |ls: &[&Leaf<Pg>]| ls.iter().filter(|l| l.page().pin() == Pin::Pinned).map(|l| l.id()).collect::<Vec<_>>();
            let w = t.root().index();
            match p.pin() {
                Pin::Free => for (was, now) in workspaces(&t).iter().zip(workspaces(&opened)) {
                    prop_assert_eq!(pinned(was), pinned(&now));
                },
                Pin::Pinned if t.focused().page().kind() != p.kind() => {
                    let now = workspaces(&opened);
                    prop_assert_eq!(now[w].last().map(|l| l.page()), Some(&p));
                    prop_assert_eq!(opened.focused().page(), &p);
                    for (i, (was, now)) in workspaces(&t).iter().zip(&now).enumerate() {
                        if i != w {
                            prop_assert_eq!(was, now);
                        }
                    }
                }
                Pin::Pinned => {}
            }
        }
    }

    /// A workspace of one leaf arranges to one rectangle: the viewport less the root's row,
    /// scrolling the document, with no divider and no row of its own.
    #[test]
    fn a_workspace_of_one_leaf_is_one_rectangle_under_the_bar((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let Tree::Leaf(leaf) = &t.root().focus.tree else { continue };
            if t.root().fit == Fit::Fits {
                continue;
            }
            let a = arrange(&t, at, &m);
            let bar = usize::from(t.root().len() > 1);
            prop_assert_eq!(a.tiles.len(), 1);
            prop_assert_eq!(a.tiles[0].id, leaf.id());
            prop_assert_eq!(a.tiles[0].scroll, Scroller::Document);
            prop_assert!(a.dividers.is_empty());
            prop_assert_eq!(a.rows.len(), bar);
            prop_assert!(a.rows.iter().all(|r| r.node.is_empty()));
            let row = u32::try_from(bar).unwrap_or(1) * m.row.0.min(at.h.0);
            prop_assert_eq!(a.tiles[0].rect.h.0 + row, at.h.0);
            prop_assert_eq!(bounds(&a).map(|b| b.w), Some(at.w));
        }
    }

    /// `parse(print(t))` is `t`, with fresh ids.
    #[test]
    fn the_address_round_trips((home, _m, first, steps) in run()) {
        for (_, _, t, _) in states(home, first, &steps) {
            let s = print(&t);
            let back = parse(&s, &t);
            prop_assert_eq!(pages(&back), pages(&t), "{}", s);
            prop_assert_eq!(print(&back), s);
            let old = ids(&t);
            prop_assert!(ids(&back).iter().all(|id| !old.contains(id)));
        }
    }

    /// Every act keeps the tree well formed: the root's shape the home's, two children below the
    /// root, at most `MOST` free tiles a workspace, a pinned leaf only as its workspace's end. Closing
    /// a workspace's last tile changes nothing.
    #[test]
    fn every_act_keeps_the_tree_well_formed((home, _m, first, steps) in run()) {
        for (_, _, t, _) in states(home, first, &steps) {
            prop_assert_eq!(t.check(), Ok(()));
            if let Tree::Leaf(_) = &t.root().focus.tree {
                prop_assert_eq!(act(t.clone(), Act::Close), t.clone());
            }
        }
    }
}

fn weight(s: Share) -> u32 {
    match s {
        Share::One => 1,
        Share::Two => 2,
        Share::Three => 3,
    }
}

/// An open of one page, which inserted a tile beside a free leaf, then `Close`, gives the tree
/// back, the pinned ends let go on both sides; an open that opens a chain first (`Act::Open`) is
/// left out. The law reaches its
/// assertion in at least a third of its cases.
#[test]
fn open_then_close_is_the_identity_where_the_open_inserted() {
    let config = cases(256);
    let total = config.cases;
    let mut runner = TestRunner::new(config);
    let reached = Cell::new(0u32);
    let tried = Cell::new(0u32);
    let free = prop_oneof![
        Just(super::page::Kind::List),
        Just(super::page::Kind::Column),
        Just(super::page::Kind::Slide)
    ];
    let open = (free, 0u8..4).prop_map(|(k, n)| super::page::pg(k, n));
    let result = runner.run(&(run(), open), |((home, _m, first, steps), p)| {
        for (_, _, t, _) in states(home, first, &steps) {
            if t.focused().page().pin() == Pin::Pinned
                || t.focused().page().kind() == p.kind()
                || chained(&p, t.focused().page())
            {
                continue;
            }
            tried.set(tried.get() + 1);
            let opened = act(t.clone(), Act::Open(p.clone()));
            let w = t.root().index();
            if workspaces(&opened)[w].len() == workspaces(&t)[w].len() + 1 {
                reached.set(reached.get() + 1);
                let closed = act(opened, Act::Close);
                // Beside a held page the close is the one made without it, which drops a share a
                // page left alone beside it had: the trees agree without their held pages.
                let (a, b) = (act(closed, Act::Release), act(t.clone(), Act::Release));
                prop_assert_eq!(pages(&a), pages(&b));
            }
        }
        Ok(())
    });
    result.expect("open then close is the identity");
    let (reached, tried) = (reached.get(), tried.get());
    eprintln!(
        "open then close: {reached} of {tried} opens from a free leaf inserted, over {total} runs"
    );
    assert!(reached * 3 >= tried, "{reached} of {tried}");
}

proptest! {
    /// `parse` answers a well-formed tree for any string: grammar characters, escapes, and
    /// thousands of nested nodes among them.
    #[test]
    fn parse_answers_a_well_formed_tree_for_any_string(
        home in super::runs::home(),
        s in prop_oneof![
            "[|!()'*,~%:0-9a-z/ -]{0,60}",
            ".{0,40}",
            (1usize..5000, "[|-]", "[!]?").prop_map(|(n, a, f)| format!("list/0~|!({}", format!("{a}{f}(").repeat(n))),
            (1usize..3000).prop_map(|n| format!("list/0~|!({}*{}", "|(a,".repeat(n), ")".repeat(n + 1))),
        ],
    ) {
        let t = Tiles::new(home).expect("the generated home is well formed");
        let back = parse(&s, &t);
        prop_assert_eq!(back.check(), Ok(()));
        prop_assert_eq!(print(&parse(&print(&back), &back)), print(&back));
    }
}

/// Whether an open of `p` from `from` opens pages of `p`'s `home` chain before it: `from` is on
/// the chain past its first page.
fn chained(p: &super::page::Pg, from: &super::page::Pg) -> bool {
    let mut at = p.home();
    let mut first = true;
    while at != *p {
        if at == *from {
            return !first;
        }
        let next = at.home();
        if next == at {
            return false;
        }
        at = next;
        first = false;
    }
    false
}

/// The tree with every share One: what a resize may not change.
fn unshared(n: &Node<Leaf<Pg>>) -> Node<Leaf<Pg>> {
    let part = |p: &viewtree::tiles::Part<Leaf<Pg>>| viewtree::tiles::Part {
        share: Share::One,
        tree: match &p.tree {
            Tree::Leaf(l) => Tree::Leaf(l.clone()),
            Tree::Node(c) => Tree::Node(unshared(c)),
        },
    };
    Node {
        axis: n.axis,
        fit: n.fit,
        before: n.before.iter().map(part).collect(),
        focus: Box::new(part(&n.focus)),
        after: n.after.iter().map(part).collect(),
    }
}

proptest! {
    #![proptest_config(cases(256))]

    /// A resize changes nothing but shares, and twice is once.
    #[test]
    fn a_resize_changes_only_shares_and_twice_is_once((home, _m, first, steps) in run()) {
        for (was, a, now, _) in states(home, first, &steps) {
            if let Some(r @ Act::Resize { .. }) = a {
                prop_assert_eq!(unshared(now.root()), unshared(was.root()));
                prop_assert_eq!(&act(now.clone(), r), &now);
            }
        }
    }

    /// Every divider stands at its stop `now`, and its stops stand in order along its axis. A
    /// resize to a stop puts the divider at the stop's place and gives every drawn tile its least
    /// size, the same tiles drawn.
    #[test]
    fn a_resize_to_a_stop_puts_the_divider_there((home, m, first, steps) in run()) {
        for (_, _, t, at) in states(home, first, &steps) {
            let a = arrange(&t, at, &m);
            for d in &a.dividers {
                let along = |r: viewtree::tiles::Rect| match d.axis {
                    viewtree::tiles::Axis::Across => r.x,
                    viewtree::tiles::Axis::Down => r.y,
                };
                prop_assert_eq!(d.stops.get(d.now).map(|s| s.at), Some(along(d.rect)));
                prop_assert!(d.stops.windows(2).all(|w| w[0].at < w[1].at));
                for s in &d.stops {
                    let moved = act(t.clone(), Act::Resize { before: d.before, after: d.after, shares: s.shares });
                    let b = arrange(&moved, at, &m);
                    let line = b.dividers.iter().find(|e| (e.before, e.after) == (d.before, d.after));
                    prop_assert_eq!(line.map(|e| along(e.rect)), Some(s.at));
                    prop_assert_eq!(drawn(&b), drawn(&a));
                    if b.tiles.len() > 1 {
                        for p in &b.tiles {
                            let (w, h) = least_of(&p.page, &m);
                            prop_assert!(u64::from(p.rect.w.0) >= w && u64::from(p.rect.h.0) >= h);
                        }
                    }
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(cases(256))]

    /// An open of a page the focused workspace shows focuses the first tile showing it and moves
    /// nothing else, so an open never puts a page in a workspace twice.
    #[test]
    fn an_open_of_a_page_shown_focuses_it((home, _m, first, steps) in run(), i in any::<prop::sample::Index>()) {
        for (_, _, t, _) in states(home, first, &steps) {
            let shown = t.root().focus.tree.leaves();
            let page = shown[i.index(shown.len())].page();
            let first = shown.iter().find(|l| l.page() == page).map(|l| l.id());
            let opened = act(t.clone(), Act::Open(page.clone()));
            prop_assert_eq!(Some(opened.focused().id()), first);
            prop_assert_eq!(first.map(|id| act(t.clone(), Act::Focus(id))), Some(opened));
        }
    }
}

/// The pages `p` is opened from, nearest first, up to a workspace's home.
fn chain(p: &Pg) -> Vec<Pg> {
    let mut c = Vec::new();
    let mut at = p.clone();
    while c.len() < viewtree::tiles::MOST {
        let home = at.home();
        if home == at {
            break;
        }
        c.push(home.clone());
        at = home;
    }
    c
}

proptest! {
    #![proptest_config(cases(256))]

    /// From a workspace that is its chain's end alone, with the chain's pages down to the
    /// focused leaf opened from it, an open of a page not shown is the tree the page's bare
    /// address reads as; closed as many times as it inserted tiles, it gives the tree back.
    #[test]
    fn an_open_from_a_leaf_on_the_chain_is_its_bare_address_and_closes_back(
        lists in 1u8..8,
        p in page(),
        from in 0usize..3,
    ) {
        // Workspaces of one list each, the chains' end among them.
        let part = |n: u8| viewtree::tiles::Part::new(Share::One, Tree::Leaf(super::page::pg(super::page::Kind::List, n)));
        let home = Node::new(viewtree::tiles::Axis::Across, Fit::Never, part(0), (1..lists).map(part).collect(), 0);
        let t = Tiles::new(home).expect("a home of lists is well formed");
        let c = chain(&p);
        let Some(end) = c.last() else {
            return Ok(());
        };
        let Some(ws) = t.root().parts().position(|q| matches!(&q.tree, Tree::Leaf(l) if l.page() == end)) else {
            return Ok(());
        };
        let id = workspaces(&t)[ws][0].id();
        let mut t = act(t, Act::Focus(id));
        for q in c.iter().rev().skip(1).take(from) {
            t = act(t, Act::Open(q.clone()));
        }
        let shown = |t: &Tiles<Pg>| workspaces(t)[ws].iter().map(|l| l.page().clone()).collect::<Vec<_>>();
        if shown(&t).contains(&p) {
            return Ok(());
        }
        let opened = act(t.clone(), Act::Open(p.clone()));
        prop_assert_eq!(shown(&opened), shown(&parse(&p.print(), &t)));
        let mut closed = opened.clone();
        for _ in 0..shown(&opened).len() - shown(&t).len() {
            closed = act(closed, Act::Close);
        }
        prop_assert_eq!(pages(&closed), pages(&t));
    }
}
