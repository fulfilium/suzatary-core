//! One tree of pages, arranged for a viewport.
//!
//! `Tiles` holds the tree. Its root's children are the workspaces, and a node's children are a
//! zipper on the focused one. `act` changes the tree and reads no viewport. `arrange` answers what
//! is drawn where on a viewport. `print` and `parse` are the address. The page type `P` declares
//! its kind, shape, pin and address through `Page`; nothing here names a page.

mod act;
mod address;
mod apportion;
mod arrange;
mod arranged;
mod escape;
mod form;
mod hole;
mod leaf;
mod node;
mod open;
mod page;
mod px;
mod tree;
mod walk;

use std::cell::Cell;
use std::rc::Rc;

pub use act::{Act, Dir, act};
pub use address::{parse, print};
pub use arrange::{Measures, arrange};
pub use arranged::{Arranged, At, Crumb, Divider, Opens, Placed, Row, Scroller, Stop};
pub use form::Unfit;
pub use leaf::{Id, Leaf};
pub use node::{Axis, Fit, Node, Part, Share};
pub use open::MOST;
pub use page::{Page, Pin, Ratio, Shape};
pub use px::{Px, Rect, Viewport};
pub use tree::Tree;

/// The root's children are the workspaces. Below the root, every node has two children or more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tiles<P> {
    pub(crate) root: Node<Leaf<P>>,
    pub(crate) home: Rc<Node<P>>,
    pub(crate) next: u64,
}

impl<P: Page> Tiles<P> {
    /// The tiles of `home`, which is also what an address leaves out (`print`). Refused unless
    /// `home` is well formed (`Unfit`).
    pub fn new(home: Node<P>) -> Result<Tiles<P>, Unfit> {
        form::check(&home, &home, &Page::pin)?;
        let next = Cell::new(0);
        let root = mint(&home, &next);
        Ok(Tiles {
            root,
            home: Rc::new(home),
            next: next.get(),
        })
    }

    /// Whether the tree is well formed against its home; `new`, `act` and `parse` keep it so.
    pub fn check(&self) -> Result<(), Unfit> {
        form::check(&self.root, &self.home, &|l: &Leaf<P>| l.page().pin())
    }

    #[must_use]
    pub fn root(&self) -> &Node<Leaf<P>> {
        &self.root
    }

    #[must_use]
    pub fn focused(&self) -> &Leaf<P> {
        self.root.focus.tree.focused()
    }

    /// The declared home of the focused workspace: the page a bar names it by.
    #[must_use]
    pub fn workspace(&self) -> P {
        let home = (*self.home).clone().select(self.root.index());
        home.focus.tree.focused().clone()
    }

    pub fn page(&self, id: Id) -> Option<&P> {
        self.root
            .parts()
            .flat_map(|p| p.tree.leaves())
            .find(|l| l.id() == id)
            .map(Leaf::page)
    }

    /// The same tiles, each node's children given the shares `f` answers for the node, where it
    /// answers one share for each child; nothing but shares changes.
    #[must_use]
    pub fn reshared(self, f: &impl Fn(&Node<Leaf<P>>) -> Option<Vec<Share>>) -> Tiles<P> {
        Tiles {
            root: reshare(self.root, f),
            ..self
        }
    }

    /// A tree of pages as tiles of this lineage: same home, ids from this counter on.
    pub(crate) fn grow(&self, root: &Node<P>) -> Tiles<P> {
        let next = Cell::new(self.next);
        let root = mint(root, &next);
        Tiles {
            root,
            home: Rc::clone(&self.home),
            next: next.get(),
        }
    }

    pub(crate) fn mint(&mut self, page: P) -> Leaf<P> {
        let leaf = Leaf::new(self.next, page);
        self.next = self.next.saturating_add(1);
        leaf
    }
}

fn reshare<L>(n: Node<L>, f: &impl Fn(&Node<L>) -> Option<Vec<Share>>) -> Node<L> {
    let shares = f(&n).filter(|s| s.len() == n.len());
    n.map_parts(|k, part| {
        let tree = match part.tree {
            Tree::Node(c) => Tree::Node(reshare(c, f)),
            leaf @ Tree::Leaf(_) => leaf,
        };
        let share = shares
            .as_ref()
            .and_then(|s| s.get(k).copied())
            .unwrap_or(part.share);
        Part { share, tree }
    })
}

fn mint<P: Clone>(home: &Node<P>, next: &Cell<u64>) -> Node<Leaf<P>> {
    home.map(&|p: &P| {
        let id = next.get();
        next.set(id.saturating_add(1));
        Leaf::new(id, p.clone())
    })
}

/// What the walk, the open rule and the arrangement read of a leaf.
pub(crate) trait Tile {
    /// `None` for the ghost a probe opens.
    fn id(&self) -> Option<Id>;
    fn pin(&self) -> Pin;
    fn share(&self) -> Share;
    fn shape(&self) -> Shape;
}

impl<P: Page> Tile for Leaf<P> {
    fn id(&self) -> Option<Id> {
        Some(Leaf::id(self))
    }
    fn pin(&self) -> Pin {
        self.page().pin()
    }
    fn share(&self) -> Share {
        self.page().share()
    }
    fn shape(&self) -> Shape {
        self.page().shape()
    }
}
