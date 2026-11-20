//! The order every reading of a conversation's lines takes ([`sequenced`]), over generated sets of
//! nodes that rest on one another, some of them the spine's (the box's lines), concurrent ones
//! included. The laws:
//! - it is a linear extension of the causal order, and a function of the set;
//! - restricted to the spine, it is [`causal`]'s order, so a store whose every turn the box wrote
//!   reads its turns as it read them before;
//! - a node that arrives later, that nothing rests on and that is not the spine's, moves no other
//!   node, and lands after the spine's last; one that is the spine's, written over every node
//!   held as the box writes under its lock, moves no node at or before the spine's last.

use proptest::prelude::*;
use suzatary_lines::secretary::sequence::{Node, Order, causal, sequenced};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Writer {
    Spine,
    Other,
}

/// One generated node: its name, instant, parents (as earlier nodes' names) and writer.
#[derive(Debug, Clone)]
struct Generated {
    name: u16,
    at: u8,
    parents: Vec<u16>,
    writer: Writer,
}

/// Up to 24 nodes, each resting on any earlier ones, with few instants so ties are common, and
/// names a shuffle so the name breaks ties in no fixed direction.
fn nodes() -> impl Strategy<Value = Vec<Generated>> {
    (1usize..24).prop_flat_map(|n| {
        let names = Just((0..u16::try_from(n).expect("small")).collect::<Vec<_>>()).prop_shuffle();
        let each = proptest::collection::vec(
            (
                0u8..4,
                proptest::collection::vec(any::<prop::sample::Index>(), 0..3),
                prop_oneof![Just(Writer::Spine), Just(Writer::Other)],
            ),
            n,
        );
        (names, each).prop_map(|(names, each)| {
            each.into_iter()
                .enumerate()
                .map(|(i, (at, parents, writer))| {
                    let mut parents: Vec<u16> = if i == 0 {
                        Vec::new()
                    } else {
                        parents.iter().map(|p| names[p.index(i)]).collect()
                    };
                    parents.sort_unstable();
                    parents.dedup();
                    Generated {
                        name: names[i],
                        at,
                        parents,
                        writer,
                    }
                })
                .collect()
        })
    })
}

fn view(nodes: &[Generated]) -> Vec<Node<'_, u16, u8, Writer>> {
    nodes
        .iter()
        .map(|node| Node {
            name: &node.name,
            at: node.at,
            parents: &node.parents,
            writer: node.writer,
        })
        .collect()
}

fn spine(writer: Writer) -> bool {
    match writer {
        Writer::Spine => true,
        Writer::Other => false,
    }
}

/// The names, in order.
fn names(nodes: &[Generated], order: &Order) -> Vec<u16> {
    order.iter().map(|(at, _)| nodes[*at].name).collect()
}

fn ordered(nodes: &[Generated]) -> Vec<u16> {
    names(
        nodes,
        &sequenced(&view(nodes), |writer: &Writer| spine(*writer)),
    )
}

/// Where the spine's last node stands in `order`; `None` for a set with no spine node.
fn last_spine(nodes: &[Generated], order: &[u16]) -> Option<usize> {
    order.iter().rposition(|name| {
        nodes
            .iter()
            .any(|node| node.name == *name && node.writer == Writer::Spine)
    })
}

proptest! {
    #[test]
    fn it_is_a_linear_extension_and_a_function_of_the_set(
        nodes in nodes(),
        seed in any::<u64>(),
    ) {
        let order = sequenced(&view(&nodes), |writer: &Writer| spine(*writer));
        prop_assert_eq!(order.len(), nodes.len());
        for (position, (_, rests_on)) in order.iter().enumerate() {
            prop_assert!(rests_on.iter().all(|&parent| parent < position));
        }
        let mut placed: Vec<usize> = order.iter().map(|(at, _)| *at).collect();
        placed.sort_unstable();
        prop_assert_eq!(placed, (0..nodes.len()).collect::<Vec<_>>());
        let mut shuffled = nodes.clone();
        let mut state = seed;
        for i in (1..shuffled.len()).rev() {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let j = usize::try_from(state >> 33).expect("fits") % (i + 1);
            shuffled.swap(i, j);
        }
        prop_assert_eq!(ordered(&shuffled), ordered(&nodes));
    }

    #[test]
    fn the_spine_keeps_today_s_order(nodes in nodes()) {
        let spine_of = |order: Vec<u16>| -> Vec<u16> {
            order
                .into_iter()
                .filter(|name| nodes.iter().any(|n| n.name == *name && n.writer == Writer::Spine))
                .collect()
        };
        prop_assert_eq!(
            spine_of(ordered(&nodes)),
            spine_of(names(&nodes, &causal(&view(&nodes))))
        );
    }

    #[test]
    fn a_later_line_moves_nothing_the_spine_placed(
        nodes in nodes(),
        at in 0u8..4,
        parents in proptest::collection::vec(any::<prop::sample::Index>(), 0..3),
        writer in prop_oneof![Just(Writer::Spine), Just(Writer::Other)],
    ) {
        let before = ordered(&nodes);
        let mut after = nodes.clone();
        // The box writes under its lock, over every line it holds; another writer over some.
        let mut rests_on: Vec<u16> = match writer {
            Writer::Spine => nodes.iter().map(|node| node.name).collect(),
            Writer::Other => parents.iter().map(|p| nodes[p.index(nodes.len())].name).collect(),
        };
        rests_on.sort_unstable();
        rests_on.dedup();
        let name = u16::try_from(nodes.len()).expect("small");
        after.push(Generated { name, at, parents: rests_on, writer });
        let then = ordered(&after);
        let kept = last_spine(&nodes, &before).map_or(0, |last| last + 1);
        prop_assert_eq!(&then[..kept], &before[..kept]);
        match writer {
            Writer::Other => {
                let others: Vec<u16> = then.iter().copied().filter(|n| *n != name).collect();
                prop_assert_eq!(others, before);
                let at = then.iter().position(|n| *n == name).expect("placed");
                prop_assert!(at >= kept);
            }
            Writer::Spine => {}
        }
    }
}

/// Two concurrent spine nodes, and a node of another writer resting on one of them: still one
/// order, the spine's two in causal order, and the other node after the spine node it rests on.
#[test]
fn concurrent_spine_nodes_are_ordered_as_today() {
    let nodes = [
        Generated {
            name: 0,
            at: 0,
            parents: Vec::new(),
            writer: Writer::Spine,
        },
        Generated {
            name: 2,
            at: 2,
            parents: vec![0],
            writer: Writer::Spine,
        },
        Generated {
            name: 1,
            at: 1,
            parents: vec![0],
            writer: Writer::Spine,
        },
        Generated {
            name: 3,
            at: 0,
            parents: vec![2],
            writer: Writer::Other,
        },
    ];
    assert_eq!(ordered(&nodes), [0, 1, 2, 3]);
}

/// A line of another writer that the box's later line rests on is placed just before that line,
/// whatever its clock says; one that no box line rests on goes after the box's last.
#[test]
fn a_tab_s_line_stands_before_the_box_line_that_rests_on_it() {
    let nodes = [
        Generated {
            name: 0,
            at: 5,
            parents: Vec::new(),
            writer: Writer::Spine,
        },
        Generated {
            name: 1,
            at: 9,
            parents: vec![0],
            writer: Writer::Other,
        },
        Generated {
            name: 2,
            at: 6,
            parents: vec![0],
            writer: Writer::Spine,
        },
        Generated {
            name: 3,
            at: 7,
            parents: vec![2, 1],
            writer: Writer::Spine,
        },
        Generated {
            name: 4,
            at: 1,
            parents: vec![0],
            writer: Writer::Other,
        },
    ];
    // Today's order puts 4 (its clock 1) and then 2 ahead of 1: the clock places a tab's line
    // among the box's.
    assert_eq!(names(&nodes, &causal(&view(&nodes))), [0, 4, 2, 1, 3]);
    assert_eq!(ordered(&nodes), [0, 2, 1, 3, 4]);
}

fn node(name: u16, at: u8, parents: &[u16], writer: Writer) -> Generated {
    Generated {
        name,
        at,
        parents: parents.to_vec(),
        writer,
    }
}

/// Two tabs' chains written before the box placed either line of them are not kept apart: §3.1
/// orders the lines between two box lines by their writers' clocks, so A1 → A2 and B1 → B2,
/// written in turn, read A1, B1, A2, B2. Each chain stays in its own order, as every linear
/// extension keeps it; nothing promises a tab's lines stand together.
#[test]
fn two_tabs_chains_before_the_box_read_by_their_clocks() {
    let nodes = [
        node(0, 0, &[], Writer::Spine),
        node(10, 1, &[0], Writer::Other),
        node(20, 2, &[0], Writer::Other),
        node(11, 3, &[10], Writer::Other),
        node(21, 4, &[20], Writer::Other),
    ];
    assert_eq!(ordered(&nodes), [0, 10, 20, 11, 21]);
}

/// A pair after the box's last line is drawn provisionally: X and Y, read X, Y by their clocks,
/// read Y, S, X once the box writes S over Y alone (it had not taken X when it wrote), since Y now
/// stands before the box line that rests on it and X still after the box's last. A pair at or
/// before the box's last line never flips (`a_later_line_moves_nothing_the_spine_placed`).
#[test]
fn a_pair_after_the_box_s_last_line_can_flip_when_the_box_writes_over_one() {
    let mut nodes = vec![
        node(0, 0, &[], Writer::Spine),
        node(1, 1, &[0], Writer::Other),
        node(2, 2, &[0], Writer::Other),
    ];
    assert_eq!(ordered(&nodes), [0, 1, 2]);
    nodes.push(node(3, 3, &[2], Writer::Spine));
    assert_eq!(ordered(&nodes), [0, 2, 3, 1]);
}
