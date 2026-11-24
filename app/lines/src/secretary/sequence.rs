//! One total order of a set of nodes that rest on one another, computed alike by every replica
//! from the set it holds.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

/// One node: its name, its writer's instant, the names it rests on (a name that is no node of the
/// set orders nothing), and its writer.
#[derive(Debug, Clone, Copy)]
pub struct Node<'a, N, T, W> {
    pub name: &'a N,
    pub at: T,
    pub parents: &'a [N],
    pub writer: W,
}

/// Each node by its position in the input, in order, beside the positions in that order of the
/// nodes it rests on, ascending.
pub type Order = Vec<(usize, Vec<usize>)>;

/// The nodes in causal order, ties among the nodes ready broken by their [`Stamp`].
/// A function of the set, whatever order it is given in.
#[must_use]
pub fn causal<N: Ord, T: Ord + Copy, W>(nodes: &[Node<'_, N, T, W>]) -> Order {
    let parents = parents(nodes);
    let order = kahn(nodes, &parents);
    placed(order, &parents)
}

/// [`causal`]'s order with the nodes whose writer `on_spine` names as its spine: every other node
/// placed just before the first spine node at or above it, else after the last. Total over any
/// set, concurrent spine nodes included. A later node no spine node rests on moves none at or
/// before the spine's last.
#[must_use]
pub fn sequenced<N: Ord, T: Ord + Copy, W>(
    nodes: &[Node<'_, N, T, W>],
    on_spine: impl Fn(&W) -> bool,
) -> Order {
    let parents = parents(nodes);
    let order = kahn(nodes, &parents);
    let mut rank = vec![None; nodes.len()];
    let mut spine = Vec::new();
    for &at in &order {
        if on_spine(&nodes[at].writer) {
            rank[at] = Some(spine.len());
            spine.push(at);
        }
    }
    // The rank of the first spine node at or above each node, the spine's length for none:
    // walking `order` backwards meets every child before its parents.
    let mut slot = vec![spine.len(); nodes.len()];
    for &at in order.iter().rev() {
        if let Some(own) = rank[at] {
            slot[at] = own;
        }
        for &parent in &parents[at] {
            slot[parent] = slot[parent].min(slot[at]);
        }
    }
    let mut stretches: Vec<Vec<usize>> = vec![Vec::new(); spine.len() + 1];
    for &at in &order {
        if rank[at].is_none() {
            stretches[slot[at]].push(at);
        }
    }
    let mut stretches = stretches.into_iter();
    let mut sequence = Vec::with_capacity(nodes.len());
    for &node in &spine {
        sequence.extend(stretches.next().into_iter().flatten());
        sequence.push(node);
    }
    sequence.extend(stretches.flatten());
    placed(sequence, &parents)
}

/// Each node's parents that are nodes of the set, by position.
fn parents<N: Ord, T, W>(nodes: &[Node<'_, N, T, W>]) -> Vec<Vec<usize>> {
    let index: BTreeMap<&N, usize> = nodes
        .iter()
        .enumerate()
        .map(|(at, node)| (node.name, at))
        .collect();
    nodes
        .iter()
        .map(|node| {
            node.parents
                .iter()
                .filter_map(|p| index.get(p).copied())
                .collect()
        })
        .collect()
}

/// The key the order breaks ties by among nodes that rest on nothing of each other: the writer's
/// instant, then the name. Every replica computes it from the node alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp<T, N> {
    pub at: T,
    pub name: N,
}

impl<'a, T: Copy, N> Stamp<T, &'a N> {
    #[must_use]
    pub fn of<W>(node: &Node<'a, N, T, W>) -> Self {
        Stamp {
            at: node.at,
            name: node.name,
        }
    }
}

/// The positions in causal order, the least ready node by its [`Stamp`] next (then its
/// position, for two nodes of one name).
fn kahn<N: Ord, T: Ord + Copy, W>(
    nodes: &[Node<'_, N, T, W>],
    parents: &[Vec<usize>],
) -> Vec<usize> {
    let mut waiting: Vec<usize> = parents.iter().map(Vec::len).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (later, earlier) in parents.iter().enumerate() {
        for &one in earlier {
            children[one].push(later);
        }
    }
    let key = |at: usize| Reverse((Stamp::of(&nodes[at]), at));
    let mut ready: BinaryHeap<_> = (0..nodes.len())
        .filter(|&at| waiting[at] == 0)
        .map(key)
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(Reverse((_, at))) = ready.pop() {
        for &next in &children[at] {
            waiting[next] -= 1;
            if waiting[next] == 0 {
                ready.push(key(next));
            }
        }
        order.push(at);
    }
    order
}

/// `order` with each node's parents named by their position in it.
fn placed(order: Vec<usize>, parents: &[Vec<usize>]) -> Order {
    let mut position = vec![usize::MAX; parents.len()];
    for (index, &at) in order.iter().enumerate() {
        position[at] = index;
    }
    order
        .into_iter()
        .map(|at| {
            let mut rests_on: Vec<usize> = parents[at].iter().map(|&p| position[p]).collect();
            rests_on.sort_unstable();
            (at, rests_on)
        })
        .collect()
}
