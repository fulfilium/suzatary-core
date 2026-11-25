//! The laws of a layout kept as plain data: what `load` reads back of what `save` wrote, and what
//! it refuses.

use proptest::prelude::*;
use std::num::NonZeroU8;
use tiles::{Axis, Fit, Node, Page, Part, Pin, Share, Tiles, Tree, Unfit, load, save};

use super::laws::cases;
use super::page::{Kind, Pg, pg};
use super::runs::{home, run, states};

fn free(t: &Tiles<Pg>) -> usize {
    t.root()
        .parts()
        .map(|w| {
            let leaves = w.tree.leaves();
            leaves
                .iter()
                .filter(|l| l.page().pin() == Pin::Free)
                .count()
        })
        .max()
        .unwrap_or(0)
}

proptest! {
    #![proptest_config(cases(256))]

    /// `load` of what `save` wrote is the same pages, shares, axes, fits and focus, into a fresh
    /// lineage and into the tiles it was saved from; ids are a running page's, so they are fresh.
    #[test]
    fn a_saved_layout_loads_as_the_same_tree((home, policy) in home(), run in run()) {
        let (_, _, first, steps) = run;
        let fresh = Tiles::new(home.clone(), policy).expect("the generated home is well formed");
        for (_, _, t, _) in states((home, policy), first, &steps) {
            let saved = save(&t);
            let back = load(&saved, &fresh);
            prop_assert_eq!(back.as_ref().map(save), Ok(saved.clone()));
            let again = load(&saved, &t);
            prop_assert_eq!(again.as_ref().map(save), Ok(saved));
            prop_assert_eq!(again.map(|a| a.check()), Ok(Ok(())));
        }
    }

    /// Under a smaller bound, a saved layout is refused as crowded where a workspace holds more
    /// free leaves than the bound, and loads as it was saved where none does.
    #[test]
    fn a_layout_crowded_under_the_receivers_bound_is_refused(
        (home, policy) in home(),
        run in run(),
        small in 1u8..8,
    ) {
        let (_, _, first, steps) = run;
        let Some(small) = NonZeroU8::new(small) else { return Ok(()) };
        let Ok(lineage) = Tiles::new(home.clone(), policy.with_most(small)) else {
            return Ok(());
        };
        for (_, _, t, _) in states((home, policy), first, &steps) {
            let loaded = load(&save(&t), &lineage);
            if free(&t) > usize::from(small.get()) {
                prop_assert_eq!(loaded.map(|_| ()), Err(Unfit::Crowded));
            } else {
                prop_assert_eq!(loaded.map(|l| save(&l)), Ok(save(&t)));
            }
        }
    }
}

fn leaf(kind: Kind, n: u8) -> Tree<Pg> {
    Tree::Leaf(pg(kind, n))
}

fn across(parts: Vec<Tree<Pg>>) -> Node<Pg> {
    let mut parts = parts.into_iter().map(|t| Part::new(Share::ONE, t));
    let first = parts.next().expect("a node has a child");
    Node::new(Axis::Across, Fit::Fits, first, parts.collect(), 0)
}

/// Each way a tree is not well formed is refused with its own reason.
#[test]
fn a_layout_that_is_not_well_formed_is_refused_with_the_reason() {
    let home = Node::of(
        Axis::Across,
        Fit::Never,
        Part::new(Share::ONE, leaf(Kind::List, 0)),
    );
    let lineage = Tiles::new(home.clone(), tiles::Policy::default()).expect("a list is a home");
    let root = |tree: Tree<Pg>| Node::of(Axis::Across, Fit::Never, Part::new(Share::ONE, tree));
    let node = |parts: Vec<Tree<Pg>>| Tree::Node(across(parts));
    let one_child = Tree::Node(Node::of(
        Axis::Down,
        Fit::Fits,
        Part::new(Share::ONE, leaf(Kind::List, 0)),
    ));
    let cases = [
        (root(leaf(Kind::List, 1)), Ok(())),
        (root(one_child), Err(Unfit::Single)),
        (
            root(node(vec![leaf(Kind::Side, 0), leaf(Kind::List, 1)])),
            Err(Unfit::Pin),
        ),
        (
            root(node(vec![leaf(Kind::Side, 0), leaf(Kind::Side, 1)])),
            Err(Unfit::Pin),
        ),
        (
            root(node((0..5).map(|n| leaf(Kind::List, n)).collect())),
            Err(Unfit::Crowded),
        ),
        (
            Node::of(
                Axis::Down,
                Fit::Never,
                Part::new(Share::ONE, leaf(Kind::List, 0)),
            ),
            Err(Unfit::Root),
        ),
        (
            Node::new(
                Axis::Across,
                Fit::Never,
                Part::new(Share::ONE, leaf(Kind::List, 0)),
                vec![Part::new(Share::ONE, leaf(Kind::List, 1))],
                0,
            ),
            Err(Unfit::Root),
        ),
    ];
    for (tree, want) in cases {
        let got = load(&tree, &lineage).map(|t| save(&t));
        assert_eq!(got, want.map(|()| tree), "{want:?}");
    }
}
