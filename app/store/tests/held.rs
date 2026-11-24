//! The rows' join: a later generation is kept whole over an earlier one, whatever it holds, and of
//! two frontiers the join keeps the later (the later era, then the larger count, then the greater
//! digest), as a semilattice.
//!
//! The closed check: a pull cut at any piece leaves a row that is closed, whether its kind lands
//! each piece (the lines) or each whole pull (the stores of objects), since the box sends every
//! piece as a prefix of the causal order; and a device that lost the frames of the first pull is
//! closed exactly when nothing it kept rests on what it lost (and, landing whole, it holds the
//! heads its word names).

use prodrome_data::event::Hash;
use proptest::prelude::*;
use std::collections::BTreeSet;

use suzatary_store::replica::frontier::Frontier;
use suzatary_store::replica::held::{Held, Node, Said};
use suzatary_store::replica::objects::NoKeys;
use suzatary_store::replica::pulling::{Delta, Lands, Pulling};
use suzatary_store::wire::store::SinceRead;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bare {
    name: Hash,
    rests_on: Vec<Hash>,
}

impl Node for Bare {
    fn name(&self) -> &Hash {
        &self.name
    }

    fn rests_on(&self) -> &[Hash] {
        &self.rests_on
    }

    fn prodrome(&self) -> Option<&Hash> {
        None
    }
}

fn name(i: u8) -> Hash {
    Hash::new(format!("{i:064x}")).expect("64 hex")
}

fn frontier() -> impl Strategy<Value = Frontier> {
    prop_oneof![
        1 => Just(Frontier::Nothing),
        4 => (0u64..3, 0u64..4, 0u8..4).prop_map(|(era, count, d)| Frontier::Named { era, count, digest: name(d) }),
    ]
}

fn held() -> impl Strategy<Value = Held<Bare>> {
    (
        0u64..3,
        prop::collection::btree_set(0u8..6, 0..4),
        frontier(),
    )
        .prop_map(|(generation, nodes, f)| {
            Held::of(
                generation,
                nodes.into_iter().map(|i| Bare {
                    name: name(i),
                    rests_on: Vec::new(),
                }),
                Vec::new(),
                f,
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    #[test]
    fn law_frontiers_join_as_the_later(a in frontier(), b in frontier(), c in frontier()) {
        prop_assert_eq!(a.clone().later(b.clone()), b.clone().later(a.clone()));
        prop_assert_eq!(a.clone().later(b.clone()).later(c.clone()), a.clone().later(b.clone().later(c)));
        prop_assert_eq!(a.clone().later(a.clone()), a.clone());
        let joined = a.clone().later(b.clone());
        prop_assert!(joined >= a && joined >= b);
        if let Frontier::Named { era, count, .. } = &joined {
            for x in [&a, &b] {
                if let Frontier::Named { era: e, count: n, .. } = x {
                    prop_assert!((era, count) >= (e, n));
                }
            }
        }
    }

    #[test]
    fn law_a_later_generation_is_kept_whole(a in held(), b in held()) {
        let joined = a.clone().join(&b);
        prop_assert_eq!(&joined, &b.clone().join(&a));
        match a.generation().cmp(&b.generation()) {
            std::cmp::Ordering::Less => prop_assert_eq!(&joined, &b),
            std::cmp::Ordering::Greater => prop_assert_eq!(&joined, &a),
            std::cmp::Ordering::Equal => {
                prop_assert_eq!(joined.frontier(), &a.frontier().clone().later(b.frontier().clone()));
                let names: std::collections::BTreeSet<Hash> = a.nodes().chain(b.nodes()).map(|n| n.name.clone()).collect();
                prop_assert_eq!(joined.nodes().count(), names.len());
            }
        }
    }
}

/// Node `i` rests on the earlier nodes `picks` names: the nodes in a causal order.
fn dag(picks: &[Vec<bool>]) -> Vec<Bare> {
    picks
        .iter()
        .enumerate()
        .map(|(i, p)| Bare {
            name: name(u8::try_from(i).expect("few")),
            rests_on: p
                .iter()
                .take(i)
                .enumerate()
                .filter(|(_, r)| **r)
                .map(|(j, _)| name(u8::try_from(j).expect("few")))
                .collect(),
        })
        .collect()
}

/// The box's heads over `nodes`: each that nothing rests on.
fn heads(nodes: &[Bare]) -> BTreeSet<Hash> {
    let under: BTreeSet<&Hash> = nodes.iter().flat_map(|n| &n.rests_on).collect();
    nodes
        .iter()
        .map(|n| n.name.clone())
        .filter(|n| !under.contains(n))
        .collect()
}

/// The deltas of pulling `nodes` (one pull, the box's whole word on every piece, each frontier
/// counting the `held` the asker held before and what it was sent) cut after each index of `cuts`,
/// landing as `lands` does: each piece, or once after the last.
fn pulled(
    rows: &mut Pulling<Bare, NoKeys>,
    nodes: &[Bare],
    word: &BTreeSet<Hash>,
    cuts: &BTreeSet<usize>,
    lands: Lands,
    held: usize,
) -> Vec<Vec<Delta<Bare, NoKeys>>> {
    let mut pieces: Vec<Vec<Bare>> = vec![Vec::new()];
    for (i, n) in nodes.iter().enumerate() {
        if cuts.contains(&i) && pieces.last().is_some_and(|p| !p.is_empty()) {
            pieces.push(Vec::new());
        }
        pieces.last_mut().expect("one").push(n.clone());
    }
    let mut count = u64::try_from(held).expect("few");
    let last = pieces.len() - 1;
    pieces
        .into_iter()
        .enumerate()
        .map(|(k, piece)| {
            count += u64::try_from(piece.len()).expect("few");
            let said = Said {
                heads: word.clone(),
                geneses: BTreeSet::new(),
                epoch: 1,
                nodes: piece,
                since: SinceRead::Known,
                digest: name(200 + u8::try_from(k).expect("few")),
                count,
            };
            let mut deltas = vec![rows.piece(said, NoKeys, 1)];
            match lands {
                Lands::ByPiece => deltas.push(rows.land()),
                Lands::Whole if k == last => deltas.push(rows.land()),
                Lands::Whole => {}
            }
            deltas
        })
        .collect()
}

fn lands() -> impl Strategy<Value = Lands> {
    prop_oneof![Just(Lands::ByPiece), Just(Lands::Whole)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    /// Every row a pull can land is closed: after each piece of a pull cut anywhere, landing each
    /// piece or each whole pull, and after a second pull from where the first stopped.
    #[test]
    fn law_a_pull_cut_at_any_piece_leaves_a_closed_row(
        picks in prop::collection::vec(prop::collection::vec(any::<bool>(), 12), 1..12),
        split in 0usize..12,
        cuts in prop::collection::btree_set(1usize..12, 0..6),
        stop in 0usize..8,
        lands in lands(),
    ) {
        let all = dag(&picks);
        let split = split.min(all.len());
        let word = heads(&all[..split]);
        let mut rows: Pulling<Bare, NoKeys> = Pulling::default();
        let first = pulled(&mut rows, &all[..split], &word, &cuts, lands, 0);
        prop_assert!(rows.closed(lands));
        // A tab that replays the first pull's frames and stops after any of them holds a closed row.
        let mut replayed: Pulling<Bare, NoKeys> = Pulling::default();
        for piece in first.iter().take(stop) {
            for d in piece {
                replayed.apply(d);
                prop_assert!(replayed.closed(lands));
            }
        }
        let rest = heads(&all);
        pulled(&mut rows, &all[split..], &rest, &cuts, lands, split);
        prop_assert!(rows.closed(lands));
    }

    /// A device that lost the first pull's frames and kept the second's is never closed: its
    /// frontier counts what it lost. And it is closed by the other clauses exactly as they say.
    #[test]
    fn law_a_row_that_lost_its_early_frames_is_not_closed(
        picks in prop::collection::vec(prop::collection::vec(any::<bool>(), 12), 2..12),
        split in 1usize..12,
        cuts in prop::collection::btree_set(1usize..12, 0..6),
        lands in lands(),
    ) {
        let all = dag(&picks);
        let split = split.min(all.len() - 1);
        let mut rows: Pulling<Bare, NoKeys> = Pulling::default();
        pulled(&mut rows, &all[..split], &heads(&all[..split]), &cuts, lands, 0);
        let word = heads(&all);
        let second = pulled(&mut rows, &all[split..], &word, &cuts, lands, split);
        let mut kept: Pulling<Bare, NoKeys> = Pulling::default();
        for d in second.iter().flatten() {
            kept.apply(d);
        }
        prop_assert!(!kept.closed(lands), "a frontier counting {} nodes over {} held", all.len(), all.len() - split);
    }
}
