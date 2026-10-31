//! A summary tree with an index from each node's name to its place, so finding a node by name
//! costs the tree's depth. The Prodrome's [`Tree::node`] searches the whole tree, and a reading
//! that asks it once for each node costs the square of the tree's size.
//!
//! Nothing here names a Suzatary type: it is the Prodrome's tree, indexed, and it belongs in
//! `prodrome_core::digest` beside [`Tree`].

use std::collections::BTreeMap;

use prodrome_core::digest::{Node, Tree};
use prodrome_core::event::Hash;

/// `tree`, and the path of child positions from its root to each node it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedTree<M> {
    tree: Tree<M>,
    paths: BTreeMap<Hash, Vec<usize>>,
}

impl<M> IndexedTree<M> {
    /// `tree`, indexed. Where two nodes share a name, the index keeps the one [`Tree::node`]
    /// finds: it visits the nodes in that search's order and keeps the first.
    #[must_use]
    pub fn of(tree: Tree<M>) -> Self {
        let mut paths = BTreeMap::new();
        let mut stack: Vec<(&Node<M>, Vec<usize>)> = vec![(tree.root(), Vec::new())];
        while let Some((node, path)) = stack.pop() {
            for (at, child) in node.children().iter().enumerate() {
                let mut beneath = path.clone();
                beneath.push(at);
                stack.push((child, beneath));
            }
            paths.entry(node.name().clone()).or_insert(path);
        }
        Self { tree, paths }
    }

    /// The tree.
    #[must_use]
    pub fn tree(&self) -> &Tree<M> {
        &self.tree
    }

    /// The node named `name`, as [`Tree::node`] answers, in the tree's depth.
    #[must_use]
    pub fn node(&self, name: &Hash) -> Option<&Node<M>> {
        self.paths
            .get(name)?
            .iter()
            .try_fold(self.tree.root(), |node, &at| node.children().get(at))
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::digest::Leaf;

    use super::*;

    /// A log of `lines` lines, each resting on the one before.
    fn tree(lines: usize) -> Tree<()> {
        let names: Vec<Hash> = (0..lines)
            .map(|n| prodrome_core::memo::name(format!("line {n}").as_bytes(), []))
            .collect();
        let leaves = names
            .iter()
            .enumerate()
            .map(|(at, name)| Leaf {
                name: name.clone(),
                parents: at
                    .checked_sub(1)
                    .map(|before| names[before].clone())
                    .into_iter()
                    .collect(),
                bytes: 1,
                measure: (),
            })
            .collect();
        prodrome_core::digest::tree(leaves)
            .expect("no cycle")
            .expect("lines")
    }

    /// The index is a cache of [`Tree::node`]: for every name in the tree, and one it does not
    /// hold, both find the same node.
    #[test]
    fn the_index_finds_what_the_tree_finds() {
        let absent = prodrome_core::memo::name(b"no line", []);
        for lines in [1, 2, 3, 7, 40, 333] {
            let tree = tree(lines);
            let indexed = IndexedTree::of(tree.clone());
            let mut stack = vec![tree.root()];
            while let Some(node) = stack.pop() {
                stack.extend(node.children());
                assert_eq!(
                    indexed.node(node.name()).map(std::ptr::from_ref),
                    indexed.tree().node(node.name()).map(std::ptr::from_ref),
                    "{lines} lines"
                );
            }
            assert!(indexed.node(&absent).is_none());
        }
    }
}
