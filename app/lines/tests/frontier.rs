//! A conversation's frontier as the skeleton keeps it, line by line as nodes come and go
//! (`Outlines::frontier`), is its definition read afresh from the nodes held: closed exactly when
//! every name a line of it rests on is held, and then its heads are its lines no line of it rests
//! on.

use std::collections::BTreeSet;

use proptest::prelude::*;
use suzatary_base::conversation_id::ConversationId;
use suzatary_lines::secretary::outlines::{Frontier, Outlines, Parsed};
use suzatary_lines::secretary::step::Step;
use suzatary_lines::wire::lines::{LineWriter, NodeHeader, SkeletonNode};

/// How many lines the law draws from.
const LINES: usize = 10;

fn name(n: usize) -> String {
    format!("{n:064x}")
}

/// The genesis every line rests on.
fn genesis() -> String {
    "f".repeat(64)
}

/// Line `n`: of conversation `a` or `b`, resting on the genesis and on lines drawn before it.
#[derive(Debug, Clone)]
struct Line {
    of: &'static str,
    parents: BTreeSet<usize>,
}

fn node(n: usize, line: &Line) -> Parsed {
    let mut parents: Vec<String> = line.parents.iter().map(|p| name(*p)).collect();
    parents.push(genesis());
    parents.sort();
    Parsed::of(SkeletonNode {
        hash: name(n),
        parents,
        size: 1,
        header: NodeHeader::Line {
            conversation: line.of.to_owned(),
            at: "2026-11-17T09:00:00".to_owned(),
            kind: "Said".to_owned(),
            keys: Vec::new(),
            step: Step::Asks,
            writer: LineWriter::Other,
        },
    })
    .expect("a node")
}

fn genesis_node() -> Parsed {
    Parsed::of(SkeletonNode {
        hash: genesis(),
        parents: Vec::new(),
        size: 1,
        header: NodeHeader::Structure {
            kind: "Genesis".to_owned(),
        },
    })
    .expect("a node")
}

fn lines() -> impl Strategy<Value = Vec<Line>> {
    (0..LINES)
        .map(|n| {
            (
                prop_oneof![Just("a"), Just("b")],
                proptest::collection::btree_set(0..n.max(1), 0..3),
            )
                .prop_map(move |(of, parents)| Line {
                    of,
                    parents: parents.into_iter().filter(|p| *p < n).collect(),
                })
        })
        .collect::<Vec<_>>()
}

/// One move of the skeleton: lines held and lines let go, the genesis among either.
fn moves() -> impl Strategy<Value = Vec<(Vec<usize>, Vec<usize>)>> {
    let some = || proptest::collection::vec(0..=LINES, 0..5);
    proptest::collection::vec((some(), some()), 1..12)
}

/// The frontier of `of` read afresh from `held`, as owned sets.
fn afresh(
    lines: &[Line],
    held: &BTreeSet<usize>,
    of: &str,
) -> Option<Result<BTreeSet<String>, BTreeSet<String>>> {
    let own: Vec<usize> = held
        .iter()
        .copied()
        .filter(|n| *n < LINES && lines[*n].of == of)
        .collect();
    if own.is_empty() {
        return None;
    }
    let genesis_held = held.contains(&LINES);
    let lacking: BTreeSet<String> = own
        .iter()
        .flat_map(|n| lines[*n].parents.iter().copied())
        .filter(|p| !held.contains(p))
        .map(name)
        .chain((!genesis_held).then(genesis))
        .collect();
    if !lacking.is_empty() {
        return Some(Err(lacking));
    }
    let under: BTreeSet<usize> = own
        .iter()
        .flat_map(|n| lines[*n].parents.iter().copied())
        .collect();
    Some(Ok(own
        .iter()
        .filter(|n| !under.contains(n))
        .map(|n| name(*n))
        .collect()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn the_frontier_kept_is_the_frontier_read_afresh(lines in lines(), moves in moves()) {
        let mut outlines = Outlines::default();
        let mut held: BTreeSet<usize> = BTreeSet::new();
        let parsed = |n: usize| if n == LINES { genesis_node() } else { node(n, &lines[n]) };
        let hash = |n: usize| prodrome_core::event::Hash::new(if n == LINES { genesis() } else { name(n) }).expect("a name");
        for (add, drop) in moves {
            let drop: BTreeSet<usize> = drop.into_iter().filter(|n| !add.contains(n)).collect();
            outlines.hold(add.iter().map(|n| parsed(*n)).collect(), &drop.iter().map(|n| hash(*n)).collect::<Vec<_>>());
            held.extend(&add);
            held.retain(|n| !drop.contains(n));
            for of in ["a", "b"] {
                let id = ConversationId::new(of).expect("an id");
                let kept = outlines.frontier(&id).map(|frontier| match frontier {
                    Frontier::Closed { heads, lines: count } => {
                        Ok((heads.iter().map(|h| h.as_str().to_owned()).collect::<BTreeSet<_>>(), count))
                    }
                    Frontier::Open(lacking) => Err(lacking.iter().map(|h| h.as_str().to_owned()).collect::<BTreeSet<_>>()),
                });
                let count = held.iter().filter(|n| **n < LINES && lines[**n].of == of).count();
                let read = afresh(&lines, &held, of).map(|frontier| frontier.map(|heads| (heads, count)));
                prop_assert_eq!(kept, read, "conversation {}", of);
            }
        }
    }
}

/// What a seal pays for its heads on a conversation of 20,000 lines, each resting on the one
/// before: its line held beside the others, then the frontier read, as a seal and the move after
/// it do. Printed as a measurement; run with `--release --ignored`.
#[test]
#[ignore = "a measurement, run by hand"]
fn the_frontier_of_a_long_conversation_measured() {
    const LONG: usize = 20_000;
    let id = ConversationId::new("a").expect("an id");
    let line = |n: usize| {
        let parents = if n == 0 {
            BTreeSet::new()
        } else {
            BTreeSet::from([n - 1])
        };
        node(n, &Line { of: "a", parents })
    };
    let mut outlines = Outlines::default();
    let built = std::time::Instant::now();
    outlines.hold(
        std::iter::once(genesis_node())
            .chain((0..LONG).map(line))
            .collect(),
        &[],
    );
    let built = built.elapsed();
    let mut each = Vec::new();
    for n in LONG..LONG + 100 {
        let started = std::time::Instant::now();
        outlines.hold(vec![line(n)], &[]);
        let heads = match outlines.frontier(&id) {
            Some(Frontier::Closed { heads, .. }) => heads.len(),
            other => panic!("a whole conversation: {other:?}"),
        };
        each.push(started.elapsed());
        assert_eq!(heads, 1);
    }
    each.sort();
    println!(
        "the frontier of a conversation of {LONG} lines: held whole in {built:?}; one line more \
         and its frontier, median {:?}, slowest {:?}",
        each[each.len() / 2],
        each[each.len() - 1]
    );
}
