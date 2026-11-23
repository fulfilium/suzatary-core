//! THE ROLLBACK LAW: for any set F of the writes refused for good, what stands and what is unsent
//! are what they would be had the replica never written F or anything resting on F; what stands
//! holds no write of a gone prodrome, nor one resting on it; and the refusals name F exactly. A
//! set the owner let go is taken back the same way, and names no refusal. Two replicas that write,
//! hear answers and let go in any order converge when joined, a write the box took is never hidden,
//! no write is held back unsaid, and a write is held back exactly while something under it is
//! refused for good or let go.

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
    match n % 5 {
        0 => Answer::Unsent,
        1 => Answer::Refused {
            kind: Refusal::Trust,
            reason: "later".to_owned(),
        },
        2 => Answer::Refused {
            kind: Refusal::Final,
            reason: "never".to_owned(),
        },
        3 => Answer::Taken,
        _ => Answer::LetGo,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

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
        // Stored, a refusal is the box's alone: what rests on one is held back by derivation.
        let refused_now: BTreeSet<Hash> = refused
            .iter()
            .filter(|o| matches!(&o.answer, Answer::Refused { kind: Refusal::Final | Refusal::Outdated, .. }))
            .map(|o| o.node.name.clone())
            .collect();
        prop_assert_eq!(refused_now, f);
    }

    /// A write the owner let go and what rests on it stand nowhere and are posted never, as if
    /// never written; a refusal of one keeps it let go; and none of it names a refusal.
    #[test]
    fn law_a_write_let_go_takes_back_what_rests_on_it(
        parents in prop::collection::vec(prop::collection::vec(any::<bool>(), 10), 1..10),
        held in 0usize..10,
        letting in prop::collection::vec(any::<bool>(), 10),
    ) {
        let all = dag(&parents);
        let held = held.min(all.len());
        let rows: BTreeSet<Hash> = names(&all[..held]);
        let in_rows = |n: &Hash| rows.contains(n);
        let own: Vec<Bare> = all[held..].to_vec();
        let g: BTreeSet<Hash> = own
            .iter()
            .zip(&letting)
            .filter(|(_, l)| **l)
            .map(|(n, _)| n.name.clone())
            .collect();

        let mut let_go = Writes::default();
        let_go.write(own.clone());
        let moved = let_go.let_go(&g);
        let_go.refused(g.iter().map(|n| (n.clone(), (Grounds::Final, "refused".to_owned()))).collect());
        let taken_back = resting_on(&own, &g);
        prop_assert_eq!(names(moved.iter().map(|o| &o.node)), taken_back.clone());
        let mut never = Writes::default();
        never.write(own.iter().filter(|n| !taken_back.contains(&n.name)).cloned().collect());

        prop_assert_eq!(names(let_go.standing(in_rows, &BTreeSet::new())), names(never.standing(in_rows, &BTreeSet::new())));
        prop_assert_eq!(names(let_go.unsent(in_rows)), names(never.unsent(in_rows)));
        prop_assert!(let_go.refusals().is_empty());
        prop_assert_eq!(names(let_go.untaken().map(|o| &o.node)), names(never.untaken().map(|o| &o.node)));
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

/// One step of a replica's writes: a write, an answer of the box, or the owner letting a write go.
#[derive(Debug, Clone)]
enum Step {
    Write(usize),
    Answer(usize, u8),
    LetGo(usize),
}

fn step(n: usize) -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..n).prop_map(Step::Write),
        3 => (0..n, any::<u8>()).prop_map(|(i, a)| Step::Answer(i, a)),
        2 => (0..n).prop_map(Step::LetGo),
    ]
}

/// The moves `steps` make of `writes`, over the nodes `all`.
fn run(writes: &mut Writes<Bare>, all: &[Bare], steps: &[Step]) -> Vec<Vec<Own<Bare>>> {
    steps
        .iter()
        .map(|s| match s {
            Step::Write(i) => writes.write(vec![all[*i].clone()]),
            Step::Answer(i, a) => writes.answered([(all[*i].name.clone(), answer(*a))]),
            Step::LetGo(i) => {
                let before: BTreeMap<Hash, Answer> = writes
                    .iter()
                    .map(|o| (o.node.name.clone(), o.answer.clone()))
                    .collect();
                let moved = writes.let_go(&BTreeSet::from([all[*i].name.clone()]));
                for o in &moved {
                    assert_ne!(
                        before.get(&o.node.name),
                        Some(&Answer::Taken),
                        "a taken write is let go"
                    );
                }
                moved
            }
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    /// Two replicas from one start, each writing, hearing answers and letting go in its own order,
    /// joined either way, hold the same writes; in the join a write the box took stands, a write let
    /// go stands nowhere and is never posted, and every write that does not stand is a refusal named.
    #[test]
    fn law_two_replicas_converge_a_taken_write_is_never_hidden_and_nothing_is_held_back_unsaid(
        parents in prop::collection::vec(prop::collection::vec(any::<bool>(), 8), 1..8),
        start in prop::collection::vec(step(8), 0..6),
        a in prop::collection::vec(step(8), 0..10),
        b in prop::collection::vec(step(8), 0..10),
    ) {
        let all = dag(&parents);
        let fit = |s: &[Step]| -> Vec<Step> {
            s.iter()
                .filter(|s| match s {
                    Step::Write(i) | Step::Answer(i, _) | Step::LetGo(i) => *i < all.len(),
                })
                .cloned()
                .collect()
        };
        let mut left = Writes::default();
        let common = run(&mut left, &all, &fit(&start));
        let mut right = Writes::default();
        for m in &common {
            right.apply(m);
        }
        let from_left = run(&mut left, &all, &fit(&a));
        let from_right = run(&mut right, &all, &fit(&b));
        for m in &from_right {
            left.apply(m);
        }
        for m in &from_left {
            right.apply(m);
        }
        let read = |w: &Writes<Bare>| -> BTreeMap<Hash, Answer> {
            w.iter().map(|o| (o.node.name.clone(), o.answer.clone())).collect()
        };
        prop_assert_eq!(read(&left), read(&right));

        // Held back, derived from the answers as they stand now: an untaken write refused for good,
        // or resting on a write let go or held back. Nothing else is.
        let answers = read(&left);
        let mut back: BTreeSet<Hash> = BTreeSet::new();
        loop {
            let more: Vec<Hash> = left
                .untaken()
                .map(|o| &o.node)
                .filter(|n| !back.contains(&n.name))
                .filter(|n| {
                    matches!(answers.get(&n.name), Some(Answer::Refused { kind: Refusal::Final | Refusal::Outdated, .. }))
                        || n.rests_on.iter().any(|p| back.contains(p) || answers.get(p) == Some(&Answer::LetGo))
                })
                .map(|n| n.name.clone())
                .collect();
            if more.is_empty() {
                break;
            }
            back.extend(more);
        }

        let none = BTreeSet::new();
        let standing = names(left.standing(|_| false, &none));
        let unsent = names(left.unsent(|_| false));
        let refused = names(left.refusals().into_iter().map(|(n, _, _)| n));
        prop_assert_eq!(&refused, &back, "a write is held back only while something under it is refused or let go");
        for o in left.iter() {
            match o.answer {
                Answer::Taken => prop_assert!(standing.contains(&o.node.name), "a taken write is hidden"),
                Answer::LetGo => prop_assert!(!standing.contains(&o.node.name) && !unsent.contains(&o.node.name)),
                Answer::Unsent | Answer::Refused { .. } => prop_assert!(
                    standing.contains(&o.node.name) || refused.contains(&o.node.name),
                    "{} held back unsaid", o.node.name
                ),
            }
        }
    }
}
