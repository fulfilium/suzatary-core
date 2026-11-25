//! A layout kept as plain data: the tree of pages, without the address grammar and without
//! `Address`, which a host stores however it stores values and reads back.

use super::Tiles;
use super::form::{self, Unfit};
use super::leaf::Leaf;
use super::node::Node;
use super::page::Page;

/// The tree as plain values of this crate's types: each leaf's page, each child's share, each
/// node's axis, fit and focused child. It holds no id, because an id names a page that is
/// running, and a stored layout runs nothing.
#[must_use]
pub fn save<P: Page>(tiles: &Tiles<P>) -> Node<P> {
    tiles.root.map(&|leaf: &Leaf<P>| leaf.page().clone())
}

/// The tiles of `lineage`'s home and policy that hold `root`, each leaf with a fresh id after
/// those of `lineage`. A tree that is not well formed under the lineage's home and policy is refused
/// with the reason (`Unfit`) and never read as another tree; `parse` reads any string as some tree.
///
/// # Errors
///
/// `Unfit` where `root` is not a well-formed tree under the lineage's home and policy: the
/// same cases `Tiles::check` names.
pub fn load<P: Page>(root: &Node<P>, lineage: &Tiles<P>) -> Result<Tiles<P>, Unfit> {
    form::check(root, &lineage.home, &Page::pin, lineage.policy.bound())?;
    Ok(lineage.grow(root))
}
