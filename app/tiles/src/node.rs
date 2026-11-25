//! A node: its children as a zipper, with its axis, its fit and each child's share.

use std::cmp::Ordering;
use std::num::NonZeroU8;

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

/// A child's weight in its node, a whole number from 1 to 255. A node divides its length among
/// its drawn children in proportion to their weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Share(NonZeroU8);

impl Share {
    pub const ONE: Share = Share(NonZeroU8::MIN);
    pub const TWO: Share = Share(NonZeroU8::MIN.saturating_add(1));
    pub const THREE: Share = Share(NonZeroU8::MIN.saturating_add(2));

    /// `None` for 0.
    #[must_use]
    pub fn of(weight: u8) -> Option<Share> {
        NonZeroU8::new(weight).map(Share)
    }

    #[must_use]
    pub fn weight(self) -> u8 {
        self.0.get()
    }

    /// The shares from one up to this one, in order.
    pub(crate) fn up_to(self) -> impl Iterator<Item = Share> {
        std::iter::successors(Some(Share::ONE), move |s| {
            s.0.checked_add(1).filter(|n| *n <= self.0).map(Share)
        })
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

    /// The same node with each child, by its place among the children, made by `f`.
    pub(crate) fn map_parts(self, mut f: impl FnMut(usize, Part<L>) -> Part<L>) -> Node<L> {
        let at = self.index();
        let Node {
            axis,
            fit,
            before,
            focus,
            after,
        } = self;
        let before = before
            .into_iter()
            .enumerate()
            .map(|(k, p)| f(k, p))
            .collect();
        let focus = Box::new(f(at, *focus));
        let after = after
            .into_iter()
            .enumerate()
            .map(|(k, p)| f(at + 1 + k, p))
            .collect();
        Node {
            axis,
            fit,
            before,
            focus,
            after,
        }
    }

    /// The same node with child `k` made by `f`, the focus where it stands.
    pub(crate) fn on_child(mut self, k: usize, f: impl FnOnce(Part<L>) -> Part<L>) -> Node<L> {
        match k.cmp(&self.before.len()) {
            Ordering::Less => {
                let child = self.before.remove(k);
                self.before.insert(k, f(child));
            }
            Ordering::Equal => self.focus = Box::new(f(*self.focus)),
            Ordering::Greater => {
                let k = k - self.before.len() - 1;
                if k < self.after.len() {
                    let child = self.after.remove(k);
                    self.after.insert(k, f(child));
                }
            }
        }
        self
    }

    /// The same node with `part` as child `k`, or last where `k` is past the end; the focused
    /// child stays focused.
    pub(crate) fn insert(mut self, k: usize, part: Part<L>) -> Node<L> {
        if k <= self.before.len() {
            self.before.insert(k, part);
        } else {
            let k = (k - self.before.len() - 1).min(self.after.len());
            self.after.insert(k, part);
        }
        self
    }

    /// The node without child `k` and that child, the focus where it stands: a focused child that
    /// goes passes it to the one before, else the one after. The node itself, with no child
    /// removed, where it has one child or none is at `k`.
    pub(crate) fn without(mut self, k: usize) -> Result<(Node<L>, Part<L>), Node<L>> {
        if k < self.before.len() {
            let gone = self.before.remove(k);
            return Ok((self, gone));
        }
        if k == self.before.len() {
            let next = match self.before.pop() {
                Some(prev) => prev,
                None if self.after.is_empty() => return Err(self),
                None => self.after.remove(0),
            };
            let gone = std::mem::replace(&mut *self.focus, next);
            return Ok((self, gone));
        }
        let k = k - self.before.len() - 1;
        if k < self.after.len() {
            let gone = self.after.remove(k);
            return Ok((self, gone));
        }
        Err(self)
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
