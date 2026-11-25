//! What a page declares about itself, which the tree reads and never computes, and its address.

use std::num::NonZeroU32;

use super::node::Share;
use super::px::{Px, Rect};

/// What a page declares to the layout, which the acts and `arrange` read and never compute.
///
/// Contract: pages of one kind have one pin.
pub trait Page: Clone + Eq {
    type Kind: Copy + Eq;
    /// An open of a page of the focused leaf's kind replaces that leaf.
    fn kind(&self) -> Self::Kind;
    fn shape(&self) -> Shape;
    fn pin(&self) -> Pin;
    /// The share an open gives it where it inserts it.
    fn share(&self) -> Share;
    /// The shape of the pages it opens, which decides its tile's `Opens`.
    fn opens(&self) -> Shape;
}

/// A page opened from another: what `follow` reads, and a bare address.
pub trait Home: Page {
    /// The page this one is opened from; a workspace's home answers itself. Followed, it reaches
    /// a page that answers itself; a bare address and `follow` open each page on the way.
    #[must_use]
    fn home(&self) -> Self;
}

/// A page that has an address: what `print` and `parse` read.
///
/// Contract: `parse(&p.print()) == Some(p)`.
pub trait Address: Home {
    /// The leaf's address, which the tree's print escapes.
    fn print(&self) -> String;
    /// `None` for a string that names no page; `tiles::parse` then reads the tree's head alone,
    /// or the declared home.
    fn parse(s: &str) -> Option<Self>;
}

/// How a page fills a rectangle: a flowing page takes all of it and reads well from `least` wide;
/// a paged one is typeset to a fixed page and drawn at the largest size of its ratio inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Flows { least: Px },
    Paged { least: Px, ratio: Ratio },
}

/// Width to height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    pub w: NonZeroU32,
    pub h: NonZeroU32,
}

/// A pinned leaf stays where it stands when an open replaces the pages beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    Free,
    Pinned,
}

impl Shape {
    /// The least width, and the least height: `tall` for a flowing page, `least / ratio` rounded
    /// up for a paged one.
    pub(crate) fn least(self, tall: Px) -> (Px, Px) {
        match self {
            Shape::Flows { least } => (least, tall),
            Shape::Paged { least, ratio } => {
                let h = (u64::from(least.0) * u64::from(ratio.h.get()))
                    .div_ceil(u64::from(ratio.w.get()));
                (least, Px::saturating(h))
            }
        }
    }

    /// Where the page stands in `rect`: all of it, or the largest box of its ratio, centred.
    pub(crate) fn fill(self, rect: Rect) -> Rect {
        match self {
            Shape::Flows { .. } => rect,
            Shape::Paged { ratio, .. } => {
                let (rw, rh) = (u128::from(ratio.w.get()), u128::from(ratio.h.get()));
                let (w, h) = (u128::from(rect.w.0), u128::from(rect.h.0));
                let (pw, ph) = if w * rh <= h * rw {
                    (w, w * rh / rw)
                } else {
                    (h * rw / rh, h)
                };
                let (pw, ph) = (Px::part(pw, rect.w), Px::part(ph, rect.h));
                Rect {
                    x: rect.x + Px((rect.w - pw).0 / 2),
                    y: rect.y + Px((rect.h - ph).0 / 2),
                    w: pw,
                    h: ph,
                }
            }
        }
    }
}
