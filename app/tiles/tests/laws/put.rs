//! The laws of `Act::Put`: a leaf taken from where it stands and put beside another leaf of its
//! workspace.

use std::cell::Cell;

use proptest::prelude::*;
use tiles::{Act, Axis, Dir, Fit, Id, Leaf, Page, Pin, Tiles, Tree, act};

use super::dock::{end, pages, side, without};
use super::laws::{all_nodes, cases};
use super::page::Pg;
use super::runs::{Run, ids, run, states};

fn workspace_of(t: &Tiles<Pg>, id: Id) -> Option<usize> {
    t.root()
        .parts()
        .position(|w| w.tree.leaves().iter().any(|l| l.id() == id))
}

fn page_of(t: &Tiles<Pg>, id: Id) -> Option<Pg> {
    t.page(id).cloned()
}

fn pin_of(t: &Tiles<Pg>, id: Id) -> Option<Pin> {
    t.page(id).map(Page::pin)
}

/// A direct child leaf's place in each node that holds both leaves as children.
fn siblings(t: &Tiles<Pg>, x: Id, at: Id) -> Option<(Axis, usize, usize)> {
    all_nodes(t).into_iter().find_map(|(n, _)| {
        let at_of = |id: Id| {
            n.parts()
                .position(|p| matches!(&p.tree, Tree::Leaf(l) if l.id() == id))
        };
        Some((n.axis, at_of(x)?, at_of(at)?))
    })
}

/// The axis and fit of the node that holds the leaf `id` as a child.
fn parent_of(t: &Tiles<Pg>, id: Id) -> Option<(Axis, Fit)> {
    all_nodes(t).into_iter().find_map(|(n, _)| {
        n.parts()
            .any(|p| matches!(&p.tree, Tree::Leaf(l) if l.id() == id))
            .then_some((n.axis, n.fit))
    })
}

fn share_of(t: &Tiles<Pg>, id: Id) -> Option<tiles::Share> {
    all_nodes(t).into_iter().find_map(|(n, _)| {
        n.parts()
            .find(|p| matches!(&p.tree, Tree::Leaf(l) if l.id() == id))
            .map(|p| p.share)
    })
}

fn pick(t: &Tiles<Pg>, i: prop::sample::Index, j: prop::sample::Index) -> (Id, Id) {
    let ids = ids(t);
    (ids[i.index(ids.len())], ids[j.index(ids.len())])
}

fn axis_of(edge: Dir) -> Axis {
    match edge {
        Dir::Left | Dir::Right => Axis::Across,
        Dir::Up | Dir::Down => Axis::Down,
    }
}

/// Whether the act changes anything: a free leaf, another leaf, the same workspace.
fn moves(t: &Tiles<Pg>, x: Id, at: Id) -> bool {
    x != at && workspace_of(t, x) == workspace_of(t, at) && pin_of(t, x) == Some(Pin::Free)
}

proptest! {
    #![proptest_config(cases(256))]

    /// A put keeps every leaf, its page, its id and its share, and the focused leaf; it moves no
    /// leaf to another workspace and keeps the tree well formed, the pinned ends among it. A leaf
    /// that moves then stands next to `beside` in a node of the edge's axis, on the edge's side,
    /// and before a pinned `beside`. Any other put changes nothing.
    #[test]
    fn a_put_stands_the_leaf_beside_the_other_and_keeps_all_else(
        (home, _m, first, steps) in run(),
        (i, j, edge) in (any::<prop::sample::Index>(), any::<prop::sample::Index>(), prop_oneof![Just(Dir::Left), Just(Dir::Right), Just(Dir::Up), Just(Dir::Down)]),
    ) {
        for (_, _, t, _) in states(home, first, &steps) {
            let (x, at) = pick(&t, i, j);
            let put = act(t.clone(), Act::Put { leaf: x, beside: at, edge });
            prop_assert_eq!(put.check(), Ok(()));
            if !moves(&t, x, at) {
                prop_assert_eq!(&put, &t);
                continue;
            }
            let (mut was, mut now) = (ids(&t), ids(&put));
            was.sort();
            now.sort();
            prop_assert_eq!(was, now);
            for id in ids(&t) {
                prop_assert_eq!(page_of(&put, id), page_of(&t, id));
                prop_assert_eq!(workspace_of(&put, id), workspace_of(&t, id));
            }
            prop_assert_eq!(put.focused().id(), t.focused().id());
            prop_assert_eq!(share_of(&put, x), share_of(&t, x));
            let (axis, here, there) = siblings(&put, x, at).expect("the leaf stands beside the other");
            let before = matches!(edge, Dir::Left | Dir::Up) || pin_of(&t, at) == Some(Pin::Pinned);
            prop_assert_eq!(axis, axis_of(edge));
            prop_assert_eq!(here + usize::from(before), there + usize::from(!before));
        }
    }
}

/// A free leaf that is not the focused child of a node of three children or more that fits, with
/// a free neighbour: the leaf, the neighbour, and the edge that puts the leaf back on the
/// neighbour's side of where it stood.
fn neighbour(t: &Tiles<Pg>, which: prop::sample::Index) -> Option<(Id, Id, Dir)> {
    let nodes: Vec<_> = all_nodes(t)
        .into_iter()
        .filter(|(node, path)| !path.is_empty() && node.fit == Fit::Fits && node.len() >= 3)
        .collect();
    let (node, _) = nodes.get(which.index(nodes.len().max(1)))?;
    let leaves: Vec<Option<&Leaf<Pg>>> = node
        .parts()
        .map(|part| match &part.tree {
            Tree::Leaf(leaf) => Some(leaf),
            Tree::Node(_) => None,
        })
        .collect();
    let free = |leaf: &&Leaf<Pg>| leaf.page().pin() == Pin::Free;
    let (after, before) = match node.axis {
        Axis::Across => (Dir::Right, Dir::Left),
        Axis::Down => (Dir::Down, Dir::Up),
    };
    (0..leaves.len())
        .filter(|&place| place != node.index())
        .find_map(|place| {
            let leaf = leaves[place].filter(free)?;
            let prev = place.checked_sub(1).and_then(|k| leaves[k]).filter(free);
            let next = leaves.get(place + 1).copied().flatten().filter(free);
            match (prev, next) {
                (Some(prev), _) => Some((leaf.id(), prev.id(), after)),
                (None, Some(next)) => Some((leaf.id(), next.id(), before)),
                (None, None) => None,
            }
        })
}

/// A put and a put back: a free leaf that is not a node's focused child, taken from a node of
/// three children or more that fits and put beside a free leaf as a sibling (no node is made), goes back beside the neighbour
/// it left, and the tree is the one it was, ids, shares and every node's focus.
#[test]
fn a_put_and_a_put_back_is_the_identity() {
    let config = cases(256);
    let total = config.cases;
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let reached = Cell::new(0u32);
    let strategy = (
        run(),
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        prop_oneof![
            Just(Dir::Left),
            Just(Dir::Right),
            Just(Dir::Up),
            Just(Dir::Down)
        ],
    );
    runner
        .run(
            &strategy,
            |((home, _m, first, steps), k, j, edge): (Run, _, _, _)| {
                let mut any = false;
                for (_, _, t, _) in states(home, first, &steps) {
                    let Some((x, near, back)) = neighbour(&t, k) else {
                        continue;
                    };
                    let frees: Vec<Id> = ids(&t)
                        .into_iter()
                        .filter(|id| *id != x && pin_of(&t, *id) == Some(Pin::Free))
                        .collect();
                    let Some(&at) = frees.get(j.index(frees.len().max(1))) else {
                        continue;
                    };
                    let sibling = parent_of(&t, at) == Some((axis_of(edge), Fit::Fits));
                    if !sibling {
                        continue;
                    }
                    any = true;
                    let away = act(
                        t.clone(),
                        Act::Put {
                            leaf: x,
                            beside: at,
                            edge,
                        },
                    );
                    let home = act(
                        away,
                        Act::Put {
                            leaf: x,
                            beside: near,
                            edge: back,
                        },
                    );
                    prop_assert_eq!(&home, &t);
                }
                reached.set(reached.get() + u32::from(any));
                Ok(())
            },
        )
        .expect("a put and a put back is the identity");
    eprintln!(
        "a put and a put back: {} of {total} runs compared",
        reached.get()
    );
    // 65 of 256 runs on average, a standard deviation of 7: the floor stands four below.
    assert!(reached.get() >= 36, "{} of {total}", reached.get());
}

/// Erasure for one put: a page held at the end of the focused workspace changes no put of a
/// person's. The put on the tree with the held page, the page then taken away, is the put on the
/// tree without it. A run of acts that ends in an open reads shares in a way that does not erase
/// (`a_held_page_changes_no_act_of_a_person` leaves it out), so the law is stated for the put alone.
#[test]
fn a_held_page_changes_no_put() {
    let config = cases(256);
    let total = config.cases;
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let reached = Cell::new(0u32);
    let strategy = (
        run(),
        side(),
        any::<prop::sample::Index>(),
        any::<prop::sample::Index>(),
        prop_oneof![
            Just(Dir::Left),
            Just(Dir::Right),
            Just(Dir::Up),
            Just(Dir::Down)
        ],
    );
    runner
        .run(
            &strategy,
            |((home, _m, first, steps), held_page, i, j, edge): (Run, _, _, _, _)| {
                let mut any = false;
                for (_, _, t, _) in states(home, first, &steps) {
                    if end(&t).is_some() || t.focused().page().pin() == Pin::Pinned {
                        continue;
                    }
                    let (held, bare) = (
                        act(t.clone(), Act::Hold(held_page.clone())),
                        act(t, Act::Release),
                    );
                    let docked = end(&held)
                        .map(Leaf::id)
                        .expect("a held page stands at the end");
                    let named = ids(&bare);
                    let (leaf, beside) = (named[i.index(named.len())], named[j.index(named.len())]);
                    let put = Act::Put { leaf, beside, edge };
                    any = true;
                    prop_assert_eq!(
                        without(act(held, put.clone()).root(), docked),
                        pages(&act(bare, put))
                    );
                }
                reached.set(reached.get() + u32::from(any));
                Ok(())
            },
        )
        .expect("a held page changes no put");
    eprintln!(
        "a held page, a put: {} of {total} runs compared",
        reached.get()
    );
    assert!(reached.get() * 2 >= total, "{} of {total}", reached.get());
}
