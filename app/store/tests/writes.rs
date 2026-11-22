//! THE ROLLBACK LAW: for any set F of the writes refused for good, what stands and what is unsent
//! are what they would be had the replica never written F or anything resting on F; what stands
//! holds no write of a gone prodrome, nor one resting on it; and the refusals name F exactly.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use proptest::prelude::*;
use suzatary_store::receipt::Grounds;
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

fn prodrome(i: usize) -> Hash {
    Hash::new(format!("{:064x}", 0xf00 + i % 3)).expect("64 hex")
}

/// Node `i` rests on the earlier nodes `parents` picks, so the nodes are parents first.
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
        refused.refused(
            f.iter()
                .enumerate()
                .map(|(i, n)| {
                    let kind = if i % 2 == 0 { Grounds::Final } else { Grounds::Outdated };
                    (n.clone(), (kind, "refused".to_owned()))
                })
                .collect(),
        );
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
        prop_assert_eq!(names(refused.refusals().into_iter().map(|(n, _, _)| n)), taken_back.clone());
        let refused_now: BTreeSet<Hash> = refused
            .iter()
            .filter(|o| matches!(&o.answer, Answer::Refused { kind: Refusal::Final | Refusal::Outdated, .. }))
            .map(|o| o.node.name.clone())
            .collect();
        prop_assert_eq!(refused_now, taken_back);
    }

    /// Moves taken in any order and any number of times hold the same writes, and a move taken
    /// in already is no news (`Writes::moves` is 0), which stops two tabs waking each other for
    /// ever over one refused line.
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
        let k = usize::try_from(order % moves.len() as u64).unwrap_or(0);
        let mut shuffled = Writes::default();
        for m in moves[k..].iter().chain(&moves[..k]).chain(&moves) {
            shuffled.apply(m);
        }
        let held: BTreeMap<Hash, Answer> = made.iter().map(|o| (o.node.name.clone(), o.answer.clone())).collect();
        let again: BTreeMap<Hash, Answer> = shuffled.iter().map(|o| (o.node.name.clone(), o.answer.clone())).collect();
        prop_assert_eq!(held, again);
        for m in &moves {
            prop_assert_eq!(shuffled.moves(m), 0, "a move taken in already is no news");
        }
        let news = Writes::<Bare>::default().moves(&moves[0]);
        prop_assert_eq!(news, moves[0].len(), "every write is news to a tab that holds none");
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// THE FIT LAW: the pieces hold each unsent write once, in order, and no piece of two or more
    /// goes over the post's count or bytes.
    #[test]
    fn law_every_piece_fits_a_post(
        parents in prop::collection::vec(prop::collection::vec(any::<bool>(), 12), 1..12),
        weights in prop::collection::vec(1usize..400, 12),
        most in 1usize..6,
        bytes in 1usize..900,
    ) {
        use suzatary_store::replica::writes::{Fit, Role};
        let all = dag(&parents);
        let mut made = Writes::default();
        let _ = made.write(all.clone());
        let weight = |n: &Bare| weights[n.name.as_str().chars().last().and_then(|c| c.to_digit(16)).unwrap_or(0) as usize % weights.len()];
        let fit = Fit { most, bytes };
        let pieces = made.pieces(|_| false, |_| Role::Change, fit, weight);
        let flat: Vec<&Hash> = pieces.iter().flatten().map(|n| &n.name).collect();
        let unsent: Vec<&Hash> = made.unsent(|_| false).into_iter().map(|n| &n.name).collect();
        prop_assert_eq!(flat, unsent);
        for piece in &pieces {
            let heavy: usize = piece.iter().map(|n| weight(n)).sum();
            prop_assert!(piece.len() <= most || piece.len() == 1, "{} over {most}", piece.len());
            prop_assert!(heavy <= bytes || piece.len() == 1, "{heavy} bytes over {bytes}");
        }
    }
}

/// A write answered later waits with its sentence and is posted again, never a refusal.
#[test]
fn a_write_answered_later_waits_and_goes_again() {
    use suzatary_store::replica::writes::Refusal;
    let all = dag(&[vec![], vec![true]]);
    let mut made = Writes::default();
    let _ = made.write(all.clone());
    let later = Answer::Refused {
        kind: Refusal::Later,
        reason: "it is dated 600 s ahead of the box's clock".to_owned(),
    };
    let _ = made.answered([(all[0].name.clone(), later)]);
    let waiting: Vec<(Hash, String)> = made
        .waiting()
        .into_iter()
        .map(|(n, why)| (n.name.clone(), why))
        .collect();
    assert_eq!(
        waiting,
        vec![(
            all[0].name.clone(),
            "it is dated 600 s ahead of the box's clock".to_owned()
        )]
    );
    assert!(made.refusals().is_empty(), "no refusal for good");
    assert_eq!(names(made.unsent(|_| false)), names(&all), "both go again");
}
