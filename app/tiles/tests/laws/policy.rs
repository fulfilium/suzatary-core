//! The laws of the policy a host gives: no bound makes the address reader unbounded, where a
//! bare address lands is the bound's business nowhere, and an address printed under a larger
//! bound reads under a smaller one.

use std::num::NonZeroU8;

use proptest::prelude::*;
use tiles::{Address, Axis, Fit, Leaf, Node, Page, Pin, Policy, Share, Tiles, Tree, parse, print};

use super::page::{Kind, Pg, pg};
use super::runs::{home, page, policy, run, states};

fn pages(t: &Tiles<Pg>) -> Node<&Pg> {
    t.root().map(&Leaf::page)
}

/// A root of one list per workspace, the first `lists` of them.
fn lists(lists: u8, policy: Policy) -> Tiles<Pg> {
    let part = |n: u8| tiles::Part::new(Share::One, Tree::Leaf(pg(Kind::List, n)));
    let home = Node::new(
        Axis::Across,
        Fit::Never,
        part(0),
        (1..lists).map(part).collect(),
        0,
    );
    Tiles::new(home, policy).expect("a home of lists is well formed")
}

/// The head of an address: what stands before a `~` that starts a tree, else all of it.
fn head(s: &str) -> &str {
    match s.rsplit_once('~') {
        Some((h, t)) if t.starts_with(['|', '-']) => h,
        Some(_) | None => s,
    }
}

proptest! {
    /// Under the largest bound a policy can hold, `parse` answers a well-formed tree for any
    /// string, nested nodes by the thousand among them.
    #[test]
    fn parse_is_total_under_the_largest_bound(
        (home, _) in home(),
        s in prop_oneof![
            "[|!()'*,~%:0-9a-z/ -]{0,60}",
            (1usize..20000, "[|-]", "[!]?").prop_map(|(n, a, f)| format!("list/0~|!({}", format!("{a}{f}(").repeat(n))),
            (1usize..5000).prop_map(|n| format!("list/0~|!({}*{}", "|(a,".repeat(n), ")".repeat(n + 1))),
        ],
    ) {
        let most = Policy::default().with_most(NonZeroU8::MAX);
        let Ok(t) = Tiles::new(home, most) else {
            return Ok(());
        };
        let back = parse(&s, &t);
        prop_assert_eq!(back.check(), Ok(()));
        prop_assert_eq!(print(&parse(&print(&back), &back)), print(&back));
    }

    /// The workspace a bare address is read into is the same under every policy.
    #[test]
    fn a_bare_address_lands_in_one_workspace_under_every_bound(
        n in 1u8..8,
        p in page(),
        a in policy(),
        b in policy(),
    ) {
        let (ta, tb) = (lists(n, a), lists(n, b));
        let s = p.print();
        prop_assert_eq!(parse(&s, &ta).root().index(), parse(&s, &tb).root().index());
    }
}

proptest! {
    #![proptest_config(super::laws::cases(256))]

    /// An address printed under a bound reads under a smaller one as a well-formed tree, and a
    /// tree crowded under the smaller bound reads as its head alone.
    #[test]
    fn an_address_from_a_larger_bound_reads_under_a_smaller(
        ((home, big), _m, first, steps) in run(),
        small in 1u8..8,
    ) {
        let Some(small) = NonZeroU8::new(small).filter(|s| *s < big.most()) else {
            return Ok(());
        };
        let Ok(lineage) = Tiles::new(home.clone(), big.with_most(small)) else {
            return Ok(());
        };
        for (_, _, t, _) in states((home.clone(), big), first, &steps) {
            let s = print(&t);
            let back = parse(&s, &lineage);
            prop_assert_eq!(back.check(), Ok(()));
            let crowded = t.root().parts().any(|w| {
                let leaves = w.tree.leaves();
                leaves.iter().filter(|l| l.page().pin() == Pin::Free).count() > usize::from(small.get())
            });
            if crowded {
                let alone = parse(head(&s), &lineage);
                prop_assert_eq!(pages(&back), pages(&alone), "{}", s);
            }
        }
    }
}
