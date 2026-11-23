//! A node: its children as a zipper, with its axis, its fit and each child's share.

use super::tree::Tree;

/// The focused child is the hole of the zipper, so a focus that names no child cannot be written.
/// Every state of these fields is a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node<L> {
    pub axis: Axis,
    pub fit: Fit,
    /// Nearest last.
    pub before: Vec<Part<L>>,
    pub focus: Box<Part<L>>,
    /// Nearest first.
    pub after: Vec<Part<L>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part<L> {
    pub share: Share,
    pub tree: Tree<L>,
}

/// `Across`: side by side. `Down`: one above the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Axis {
    Across,
    Down,
}

/// `Fits` draws as many children side by side as fit, and stacks the rest behind; `Never` draws
/// only its focused child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Fits,
    Never,
}

/// A child's weight in its node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Share {
    One,
    Two,
    Three,
}

impl Share {
    pub(crate) fn weight(self) -> u32 {
        match self {
            Share::One => 1,
            Share::Two => 2,
            Share::Three => 3,
        }
    }
}

impl<L> Node<L> {
    /// A node of one focused child.
    pub fn of(axis: Axis, fit: Fit, focus: Part<L>) -> Node<L> {
        Node {
            axis,
            fit,
            before: Vec::new(),
            focus: Box::new(focus),
            after: Vec::new(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.before.len() + 1 + self.after.len()
    }

    /// Never true: a node has its focused child.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The focused child's place among the children.
    #[must_use]
    pub fn index(&self) -> usize {
        self.before.len()
    }

    /// The children in order.
    pub fn parts(&self) -> impl Iterator<Item = &Part<L>> {
        self.before
            .iter()
            .chain(std::iter::once(&*self.focus))
            .chain(&self.after)
    }

    /// A node of the children `first` and `rest`, focused on child `at`, or on the last if `at` is
    /// past the end.
    pub fn new(axis: Axis, fit: Fit, first: Part<L>, rest: Vec<Part<L>>, at: usize) -> Node<L> {
        Node {
            after: rest,
            ..Node::of(axis, fit, first)
        }
        .select(at)
    }

    /// The same children focused on child `i`, or on the last if `i` is past the end.
    pub(crate) fn select(mut self, i: usize) -> Node<L> {
        while self.before.len() > i {
            let Some(prev) = self.before.pop() else { break };
            let old = std::mem::replace(&mut *self.focus, prev);
            self.after.insert(0, old);
        }
        while self.before.len() < i && !self.after.is_empty() {
            let next = self.after.remove(0);
            let old = std::mem::replace(&mut *self.focus, next);
            self.before.push(old);
        }
        self
    }

    /// The node with its focused child's tree changed by `f`, and what `f` answered beside it.
    pub(crate) fn on_focus<R>(self, f: impl FnOnce(Tree<L>) -> (Tree<L>, R)) -> (Node<L>, R) {
        let Node {
            axis,
            fit,
            before,
            focus,
            after,
        } = self;
        let Part { share, tree } = *focus;
        let (tree, r) = f(tree);
        let focus = Box::new(Part { share, tree });
        (
            Node {
                axis,
                fit,
                before,
                focus,
                after,
            },
            r,
        )
    }

    pub fn map<'a, M>(&'a self, f: &impl Fn(&'a L) -> M) -> Node<M> {
        Node {
            axis: self.axis,
            fit: self.fit,
            before: self.before.iter().map(|p| p.map(f)).collect(),
            focus: Box::new(self.focus.map(f)),
            after: self.after.iter().map(|p| p.map(f)).collect(),
        }
    }
}

impl<L> Part<L> {
    pub fn new(share: Share, tree: Tree<L>) -> Part<L> {
        Part { share, tree }
    }

    pub fn map<'a, M>(&'a self, f: &impl Fn(&'a L) -> M) -> Part<M> {
        Part {
            share: self.share,
            tree: self.tree.map(f),
        }
    }
}
