//! A tree of pages: a leaf, or a node of children.

use super::node::{Node, Part};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tree<L> {
    Leaf(L),
    Node(Node<L>),
}

impl<L> Tree<L> {
    pub fn map<'a, M>(&'a self, f: &impl Fn(&'a L) -> M) -> Tree<M> {
        match self {
            Tree::Leaf(l) => Tree::Leaf(f(l)),
            Tree::Node(n) => Tree::Node(n.map(f)),
        }
    }

    /// The leaves in order.
    pub fn leaves(&self) -> Vec<&L> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a L>) {
        match self {
            Tree::Leaf(l) => out.push(l),
            Tree::Node(n) => n.parts().for_each(|p| p.tree.collect(out)),
        }
    }

    /// The last leaf, reached through the last child of each node, and the children's places on
    /// the way to it.
    pub(crate) fn end(&self) -> (&L, Vec<usize>) {
        match self {
            Tree::Leaf(l) => (l, Vec::new()),
            Tree::Node(n) => {
                let (l, mut path) = n.after.last().unwrap_or(&n.focus).tree.end();
                path.insert(0, n.len() - 1);
                (l, path)
            }
        }
    }

    /// The leaf the focus path through this tree ends at.
    pub fn focused(&self) -> &L {
        match self {
            Tree::Leaf(l) => l,
            Tree::Node(n) => n.focus.tree.focused(),
        }
    }

    /// A node of one child is replaced by that child, below the root.
    pub(crate) fn normal(self) -> Tree<L> {
        match self {
            Tree::Leaf(l) => Tree::Leaf(l),
            Tree::Node(n) => {
                let n = n.normal_children();
                if n.before.is_empty() && n.after.is_empty() {
                    n.focus.tree
                } else {
                    Tree::Node(n)
                }
            }
        }
    }
}

impl<L> Node<L> {
    pub(crate) fn normal_children(self) -> Node<L> {
        let part = |p: Part<L>| Part {
            share: p.share,
            tree: p.tree.normal(),
        };
        Node {
            axis: self.axis,
            fit: self.fit,
            before: self.before.into_iter().map(part).collect(),
            focus: Box::new(part(*self.focus)),
            after: self.after.into_iter().map(part).collect(),
        }
    }
}
