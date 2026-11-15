//! What a replica wrote (`replica::writes`), over a node that is only a name and its parents.
//!
//! THE ROLLBACK LAW: for writes resting on rows that hold some nodes and no longer hold some
//! prodromes, and any set F of the writes refused for good, what stands and what is unsent are what
//! they would be had the replica never written F or anything resting on F; what stands holds no
//! write of a prodrome gone, nor one resting on it; and the refusals name F exactly. An answer
//! only climbs, so the moves joined in any order hold the same writes.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use proptest::prelude::*;
use suzatary_store::replica::held::Node;
use suzatary_store::replica::writes::{Answer, Own, Refusal, Writes};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Bare {
    name: Hash,
    rests_on: Vec<Hash>,
    prodrome: Option<Hash>,
}

impl Node for Bare {
    fn name(&self) -> &Hash {
        &self.name
    }

    fn rests_on(&self) -> &[Hash] {
        &self.rests_on
    }

    fn prodrome(&self) -> Option<&Hash> {
        self.prodrome.as_ref()
    }
}

fn name(i: usize) -> Hash {
    Hash::new(format!("{i:064x}")).expect("64 hex")
}

/// One of three prodromes, by `i`.
fn prodrome(i: usize) -> Hash {
    Hash::new(format!("{:064x}", 0xf00 + i % 3)).expect("64 hex")
}

/// Node `i` rests on the earlier nodes `parents` picks, so the nodes are parents first; each names
/// one of three prodromes.
fn dag(parents: &[Vec<bool>]) -> Vec<Bare> {
    parents
        .iter()
        .enumerate()
        .map(|(i, picks)| Bare {
            name: name(i),
            prodrome: Some(prodrome(i)),
            rests_on: picks
                .iter()
                .take(i)
                .enumerate()
                .filter(|(_, p)| **p)
                .map(|(j, _)| name(j))
                .collect(),
        })
        .collect()
}

/// `refused` and every node of `own` resting on one of them.
fn resting_on(own: &[Bare], refused: &BTreeSet<Hash>) -> BTreeSet<Hash> {
    let mut out: BTreeSet<Hash> = refused.clone();
    for n in own {
        if n.rests_on.iter().any(|p| out.contains(p)) {
            out.insert(n.name.clone());
        }
    }
    out
}

fn names<'a>(nodes: impl IntoIterator<Item = &'a Bare>) -> BTreeSet<Hash> {
    nodes.into_iter().map(|n| n.name.clone()).collect()
}

fn answer(n: u8) -> Answer {
    match n % 4 {
        0 => Answer::Unsent,
        1 => Answer::Refused {
            kind: Refusal::Trust,
            reason: "later".to_owned(),
        },
        2 => Answer::Refused {
            kind: Refusal::Final,
            reason: "never".to_owned(),
        },
        _ => Answer::Taken,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn law_a_refusal_for_good_takes_back_what_rests_on_it(
        parents in prop::collection::vec(prop::collection::vec(any::<bool>(), 10), 1..10),
        held in 0usize..10,
        refusing in prop::collection::vec(any::<bool>(), 10),
        dropped in prop::collection::vec(any::<bool>(), 3),
    ) {
        let all = dag(&parents);
        let held = held.min(all.len());
        let rows: BTreeSet<Hash> = names(&all[..held]);
        let in_rows = |n: &Hash| rows.contains(n);
        let own: Vec<Bare> = all[held..].to_vec();
        let f: BTreeSet<Hash> = own
            .iter()
            .zip(&refusing)
            .filter(|(_, r)| **r)
            .map(|(n, _)| n.name.clone())
            .collect();

        let mut refused = Writes::default();
        refused.write(own.clone());
        refused.refused(f.iter().map(|n| (n.clone(), "refused".to_owned())).collect());
        let taken_back = resting_on(&own, &f);
        let mut never = Writes::default();
        never.write(own.iter().filter(|n| !taken_back.contains(&n.name)).cloned().collect());

        let gone: BTreeSet<Hash> = (0..3).filter(|i| dropped[*i]).map(prodrome).collect();
        prop_assert_eq!(names(refused.standing(in_rows, &gone)), names(never.standing(in_rows, &gone)));
        prop_assert_eq!(names(refused.unsent(in_rows)), names(never.unsent(in_rows)));
        let of_gone: BTreeSet<Hash> = own
            .iter()
            .filter(|n| n.prodrome.as_ref().is_some_and(|p| gone.contains(p)))
            .map(|n| n.name.clone())
            .collect();
        let lost = resting_on(&own, &of_gone);
        let standing: BTreeSet<Hash> = own
            .iter()
            .map(|n| n.name.clone())
            .filter(|n| !taken_back.contains(n) && !lost.contains(n))
            .collect();
        prop_assert_eq!(names(never.standing(in_rows, &gone)), standing);
        prop_assert_eq!(names(refused.refusals().into_iter().map(|(n, _)| n)), f);
    }

    #[test]
    fn law_the_moves_of_the_writes_are_joins(
        parents in prop::collection::vec(prop::collection::vec(any::<bool>(), 10), 1..8),
        answers in prop::collection::vec((0usize..8, any::<u8>()), 0..16),
        order in any::<u64>(),
    ) {
        let all = dag(&parents);
        let mut made = Writes::default();
        let mut moves: Vec<Vec<Own<Bare>>> = vec![made.write(all.clone())];
        for (i, a) in &answers {
            if let Some(n) = all.get(*i) {
                moves.push(made.answered([(n.name.clone(), answer(*a))]));
            }
        }
        // The same moves, rotated and each taken twice.
        let k = usize::try_from(order % moves.len() as u64).unwrap_or(0);
        let mut shuffled = Writes::default();
        for m in moves[k..].iter().chain(&moves[..k]).chain(&moves) {
            shuffled.apply(m);
        }
        let held: BTreeMap<Hash, Answer> = made.iter().map(|o| (o.node.name.clone(), o.answer.clone())).collect();
        let again: BTreeMap<Hash, Answer> = shuffled.iter().map(|o| (o.node.name.clone(), o.answer.clone())).collect();
        prop_assert_eq!(held, again);
    }
}
