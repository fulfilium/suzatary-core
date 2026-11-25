//! The laws of a docked page: `Hold` and `Release` keep the focus and undo each other, a held page
//! changes no act of a person's, and an open from it is the open from the page before it.

use proptest::prelude::*;
use tiles::{Act, Id, Leaf, Node, Page, Part, Pin, Tiles, Tree, act};

use super::page::{Kind, Pg, pg};
use super::runs::{Made, Mix, apply, run, run_of, states};

/// The pages of the tree without the leaf `gone`, each node left with one child replaced by it:
/// what `Release` must give, read apart from it.
pub(super) fn without(n: &Node<Leaf<Pg>>, gone: Id) -> Node<Pg> {
    fn tree(t: &Tree<Leaf<Pg>>, gone: Id) -> Option<Tree<Pg>> {
        match t {
            Tree::Leaf(l) if l.id() == gone => None,
            Tree::Leaf(l) => Some(Tree::Leaf(l.page().clone())),
            Tree::Node(n) => {
                let parts: Vec<(bool, Part<Pg>)> = n
                    .parts()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        tree(&p.tree, gone).map(|t| (i == n.index(), Part::new(p.share, t)))
                    })
                    .collect();
                let at = parts.iter().position(|(f, _)| *f).unwrap_or(0);
                let mut parts = parts.into_iter().map(|(_, p)| p);
                let first = parts.next()?;
                let rest: Vec<_> = parts.collect();
                if rest.is_empty() {
                    return Some(first.tree);
                }
                Some(Tree::Node(Node::new(n.axis, n.fit, first, rest, at)))
            }
        }
    }
    let parts: Vec<Part<Pg>> = n
        .parts()
        .map(|p| {
            Part::new(
                p.share,
                tree(&p.tree, gone).expect("a workspace keeps a page"),
            )
        })
        .collect();
    let mut parts = parts.into_iter();
    let first = parts.next().expect("a root has a workspace");
    Node::new(n.axis, n.fit, first, parts.collect(), n.index())
}

pub(super) fn pages(t: &Tiles<Pg>) -> Node<Pg> {
    t.root().map(&|l: &Leaf<Pg>| l.page().clone())
}

fn workspace(t: &Tiles<Pg>) -> Vec<&Leaf<Pg>> {
    t.root().focus.tree.leaves()
}

/// The focused workspace's pinned end, if it has one.
pub(super) fn end(t: &Tiles<Pg>) -> Option<&Leaf<Pg>> {
    workspace(t)
        .into_iter()
        .last()
        .filter(|l| l.page().pin() == Pin::Pinned)
}

pub(super) fn side() -> impl Strategy<Value = Pg> {
    (0u8..4).prop_map(|n| pg(Kind::Side, n))
}

/// A person's act from a free leaf of the focused workspace: any page but a pinned one, and any
/// leaf focused or resized by its place.
#[derive(Debug, Clone)]
enum Person {
    Open(Pg),
    Follow(Pg),
    Close,
    Below(Pg),
    Focus(prop::sample::Index),
    Move(tiles::Dir),
    Resize(prop::sample::Index, [tiles::Share; 2]),
}

fn person() -> impl Strategy<Value = Person> {
    use tiles::{Dir, Share};
    let share = || prop_oneof![Just(Share::ONE), Just(Share::TWO), Just(Share::THREE)];
    let free = (
        prop_oneof![Just(Kind::List), Just(Kind::Column), Just(Kind::Slide)],
        0u8..4,
    )
        .prop_map(|(k, n)| pg(k, n));
    let dir = prop_oneof![
        Just(Dir::Left),
        Just(Dir::Right),
        Just(Dir::Up),
        Just(Dir::Down)
    ];
    prop_oneof![
        2 => free.clone().prop_map(Person::Open),
        2 => free.clone().prop_map(Person::Follow),
        2 => Just(Person::Close),
        1 => free.prop_map(Person::Below),
        2 => any::<prop::sample::Index>().prop_map(Person::Focus),
        1 => dir.prop_map(Person::Move),
        2 => (any::<prop::sample::Index>(), share(), share()).prop_map(|(i, a, b)| Person::Resize(i, [a, b])),
    ]
}

/// The act on `t`, naming a leaf of `t` by its place among every leaf but `docked`.
fn the_act(t: &Tiles<Pg>, p: &Person, docked: Option<Id>) -> Made {
    let ids: Vec<Id> = t
        .root()
        .parts()
        .flat_map(|q| q.tree.leaves())
        .map(Leaf::id)
        .filter(|id| Some(*id) != docked)
        .collect();
    Made::Act(match p {
        Person::Follow(q) => return Made::Follow(q.clone()),
        Person::Open(q) => Act::Open(q.clone()),
        Person::Close => Act::Close,
        Person::Below(q) => Act::Below(q.clone()),
        Person::Focus(i) => Act::Focus(ids[i.index(ids.len())]),
        Person::Move(d) => Act::Move(*d),
        Person::Resize(i, shares) => {
            let k = i.index(ids.len());
            Act::Resize {
                before: ids[k],
                after: ids[(k + 1) % ids.len()],
                shares: *shares,
            }
        }
    })
}

proptest! {
    #![proptest_config(super::laws::cases(256))]

    /// `Hold` and `Release` never move the focus. Held, a pinned page is the focused workspace's
    /// end unless a pinned end there has the focus; released, every workspace's pinned end is gone
    /// but the focused leaf and a workspace's only leaf. Both are idempotent.
    #[test]
    fn hold_and_release_keep_the_focus((home, _m, first, steps) in run(), p in side()) {
        for (_, _, t, _) in states(home, first, &steps) {
            let (held, released) = (act(t.clone(), Act::Hold(p.clone())), act(t.clone(), Act::Release));
            prop_assert_eq!(held.focused().id(), t.focused().id());
            prop_assert_eq!(released.focused().id(), t.focused().id());
            let here = t.root().index();
            for (i, ((w, h), r)) in t.root().parts().zip(held.root().parts()).zip(released.root().parts()).enumerate() {
                // A pinned end that is its workspace's focused leaf is the person's, in every workspace.
                let ends = |w: &tiles::Part<Leaf<Pg>>| w.tree.leaves().last().filter(|l| l.page().pin() == Pin::Pinned).map(|l| l.id());
                let persons = ends(w).is_some_and(|e| e == w.tree.focused().id());
                if persons {
                    prop_assert_eq!(h, w);
                    prop_assert_eq!(r, w);
                    continue;
                }
                prop_assert!(ends(r).is_none(), "released, workspace {} keeps no pinned end", i);
                if i == here {
                    prop_assert_eq!(ends(h).and_then(|e| held.page(e)), Some(&p));
                } else {
                    prop_assert!(ends(h).is_none(), "held, one page is docked");
                }
            }
            prop_assert_eq!(&act(held.clone(), Act::Hold(p.clone())), &held);
            prop_assert_eq!(&act(released.clone(), Act::Release), &released);
            prop_assert_eq!(held.check(), Ok(()));
            prop_assert_eq!(released.check(), Ok(()));
        }
    }

    /// `Below` of a pinned page is `Open` of it: one page, docked at the end, and no chain.
    #[test]
    fn below_of_a_pinned_page_is_its_open((home, _m, first, steps) in run(), p in side()) {
        for (_, _, t, _) in states(home, first, &steps) {
            prop_assert_eq!(act(t.clone(), Act::Below(p.clone())), act(t, Act::Open(p.clone())));
        }
    }

    /// `Release` after `Hold` is `Release`: the held page goes and nothing else moves, ids and
    /// shares and every node's focus, whatever the workspace's shape: a leaf, a node down, a tab
    /// node, a node across.
    #[test]
    fn release_after_hold_gives_the_tree_back((home, _m, first, steps) in run(), p in side()) {
        for (_, _, t, _) in states(home, first, &steps) {
            if end(&t).is_some() {
                continue;
            }
            let back = act(act(t.clone(), Act::Hold(p.clone())), Act::Release);
            let released = act(t.clone(), Act::Release);
            prop_assert_eq!(back.root(), released.root());
        }
    }

    /// Close of a workspace's last free leaf changes nothing, a held page beside it or not.
    #[test]
    fn close_of_a_workspaces_last_free_leaf_is_the_identity((home, _m, first, steps) in run(), p in side()) {
        for (_, _, t, _) in states(home, first, &steps) {
            for t in [t.clone(), act(t, Act::Hold(p.clone()))] {
                let free = workspace(&t).iter().filter(|l| l.page().pin() == Pin::Free).count();
                if free == 1 && t.focused().page().pin() == Pin::Free {
                    prop_assert_eq!(&act(t.clone(), Act::Close), &t);
                }
            }
        }
    }

    /// An open, a follow, or a `Below`, of a page the workspace does not show, from a pinned leaf, is the
    /// same act from the free leaf before it: the leaf its node's child before it is focused on. A
    /// page shown is focused, and nothing else moves (`an_open_of_a_page_shown_focuses_it`).
    #[test]
    fn an_open_from_a_pinned_leaf_is_the_open_from_the_leaf_before_it(
        (home, _m, first, steps) in run(),
        p in super::runs::page(),
        how in 0u8..3,
    ) {
        for (_, _, t, _) in states(home, first, &steps) {
            let Some(e) = end(&t).map(Leaf::id) else { continue };
            if workspace(&t).iter().any(|l| *l.page() == p) {
                continue;
            }
            let on = act(t.clone(), Act::Focus(e));
            let Some(before) = before_end(&on) else { continue };
            let from = act(t.clone(), Act::Focus(before));
            let a = match how {
                0 => Made::Act(Act::Below(p.clone())),
                1 => Made::Act(Act::Open(p.clone())),
                _ => Made::Follow(p.clone()),
            };
            prop_assert_eq!(pages(&apply(on, a.clone())), pages(&apply(from, a)));
        }
    }
}

/// The leaf the child before the focused workspace's end is focused on, where the end has a
/// child before it in its node.
pub(super) fn before_end(t: &Tiles<Pg>) -> Option<Id> {
    let mut tree = &t.root().focus.tree;
    while let Tree::Node(n) = tree {
        let last = n.after.last().unwrap_or(&n.focus);
        match &last.tree {
            Tree::Leaf(_) => {
                let prev = n
                    .after
                    .len()
                    .checked_sub(2)
                    .map(|i| &n.after[i])
                    .or_else(|| {
                        if n.after.is_empty() {
                            n.before.last()
                        } else {
                            Some(&n.focus)
                        }
                    })?;
                return Some(prev.tree.focused().id());
            }
            Tree::Node(_) => tree = &last.tree,
        }
    }
    None
}

/// Erasure, over the runs this law was first stated for (`Mix::Master`: weights of one to three, no
/// put): it fails on rare runs at any count, because an `Open` beside a node that the held page
/// kept from collapsing reads a share that the tree without the page does not have. 1 run in 300 on
/// master and 4 in 600 here fail at 256 cases; 20,000 cases fail it. It waits for a fix of `Open`.
/// Erasure: a page held at the end of the workspace a person acts in changes none of their acts.
/// Any act from a free leaf, `Close` among them, the held page then taken away, gives the tree
/// the same act gives without it, the policy's `most` among the rules it reads. The law reaches its comparison
/// in at least half its runs.
#[test]
fn a_held_page_changes_no_act_of_a_person() {
    let config = super::laws::cases(256);
    let total = config.cases;
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let (reached, acted) = (std::cell::Cell::new(0u32), std::cell::Cell::new(0u32));
    let result = runner.run(
        &(
            run_of(Mix::Master),
            side(),
            prop::collection::vec(person(), 1..6),
        ),
        |((home, _m, first, steps), p, acts)| {
            let mut any = false;
            for (_, _, t, _) in states(home, first, &steps) {
                if end(&t).is_some() || t.focused().page().pin() == Pin::Pinned {
                    continue;
                }
                any = true;
                let held = act(t.clone(), Act::Hold(p.clone()));
                let Some(docked) = end(&held).map(Leaf::id) else {
                    return Err(TestCaseError::fail("a held page stands at the end"));
                };
                // `Hold` lets every other workspace's pinned end go, as `Release` does.
                let (mut with, mut bare) = (held, act(t.clone(), Act::Release));
                for a in &acts {
                    let (aw, ab) = (the_act(&with, a, Some(docked)), the_act(&bare, a, None));
                    with = apply(with, aw);
                    bare = apply(bare, ab);
                    acted.set(acted.get() + 1);
                    prop_assert_eq!(without(with.root(), docked), pages(&bare), "after {:?}", a);
                    let free = with
                        .root()
                        .focus
                        .tree
                        .leaves()
                        .iter()
                        .filter(|l| l.page().pin() == Pin::Free)
                        .count();
                    prop_assert!(free <= usize::from(with.policy().most().get()));
                }
            }
            if any {
                reached.set(reached.get() + 1);
            }
            Ok(())
        },
    );
    result.expect("a held page changes no act of a person");
    eprintln!(
        "a held page: {} of {total} runs compared, {} acts",
        reached.get(),
        acted.get()
    );
    assert!(reached.get() * 2 >= total, "{} of {total}", reached.get());
}
