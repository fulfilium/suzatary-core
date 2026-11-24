//! What `arrange` answers: the drawn tiles, the dividers between them and the crumb rows. Together
//! they cover the viewport exactly, without overlap.

use super::leaf::Id;
use super::node::{Axis, Share};
use super::px::{Px, Rect};

/// Each tile and crumb carries the page of the leaf it names, so a host joins no id to a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arranged<P> {
    /// In the tree's order.
    pub tiles: Vec<Placed<P>>,
    pub dividers: Vec<Divider>,
    /// One for each node with children behind, outermost first.
    pub rows: Vec<Row<P>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed<P> {
    pub id: Id,
    pub page: P,
    pub rect: Rect,
    /// Where the page stands in `rect`: all of it, or a paged leaf's centred page.
    pub fill: Rect,
    pub scroll: Scroller,
    pub opens: Opens,
}

/// A tile drawn alone scrolls the document, so a phone's browser hides its address bar; tiles
/// drawn together scroll themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroller {
    Document,
    Tile,
}

/// Whether a page this tile opens would be drawn beside it, on this viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opens {
    Beside,
    Over,
}

/// Between two drawn children of a node; `before` and `after` are the drawn leaves of each child
/// nearest the divider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divider {
    pub rect: Rect,
    /// The node's place: each child's index from the root down, empty for the root.
    pub node: Vec<usize>,
    pub axis: Axis,
    pub before: Id,
    pub after: Id,
    /// Where it may stand, in order along its axis (`Act::Resize` to a stop's shares puts it
    /// there); `now` indexes the place it stands at.
    pub stops: Vec<Stop>,
    pub now: usize,
}

/// A place a divider may stand: its start along its node's axis, in the viewport's frame, and
/// the shares of the children either side of it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    pub at: Px,
    pub shares: [Share; 2],
}

/// A node's crumbs, every child in order, at the top of the node's rectangle (the root's at its
/// foot on a narrow viewport). As many as fit its width are `shown`, and the rest are `more`,
/// with the last shown place given to the overflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<P> {
    pub rect: Rect,
    /// The node's place: each child's index from the root down, empty for the root (the bar).
    pub node: Vec<usize>,
    pub axis: Axis,
    pub shown: Vec<Crumb<P>>,
    pub more: Vec<Crumb<P>>,
}

/// A child of a row's node, named by the leaf it focuses, which `Act::Focus` takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crumb<P> {
    pub to: Id,
    pub page: P,
    pub at: At,
}

/// Where a child stands against the drawn ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Before,
    Drawn,
    After,
}
