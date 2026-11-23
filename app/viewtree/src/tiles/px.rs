//! Lengths in CSS pixels, and the rectangles `arrange` answers.

use std::ops::{Add, Sub};

/// A length in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Px(pub u32);

impl Add for Px {
    type Output = Px;
    fn add(self, other: Px) -> Px {
        Px(self.0.saturating_add(other.0))
    }
}

/// Stops at zero: a length is never negative.
impl Sub for Px {
    type Output = Px;
    fn sub(self, other: Px) -> Px {
        Px(self.0.saturating_sub(other.0))
    }
}

/// `x` and `y` from the viewport's top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: Px,
    pub y: Px,
    pub w: Px,
    pub h: Px,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Viewport {
    pub w: Px,
    pub h: Px,
}

impl Rect {
    /// The rectangle cut into a band of `h` at its top (or foot) and the rest.
    pub(crate) fn band(self, h: Px, at: Edge) -> (Rect, Rect) {
        let h = Px(h.0.min(self.h.0));
        let rest = self.h - h;
        match at {
            Edge::Top => (
                Rect { h, ..self },
                Rect {
                    y: self.y + h,
                    h: rest,
                    ..self
                },
            ),
            Edge::Foot => (
                Rect {
                    y: self.y + rest,
                    h,
                    ..self
                },
                Rect { h: rest, ..self },
            ),
        }
    }
}

/// Where a node's crumb row stands in its rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Top,
    Foot,
}

/// A least extent: a width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Size {
    pub w: Px,
    pub h: Px,
}

impl Px {
    /// A length past `u32::MAX` px stops there, as `+` does.
    pub(crate) fn saturating(x: u64) -> Px {
        Px(u32::try_from(x).unwrap_or(u32::MAX))
    }

    /// A part of the length `of`: at most `of`, so no `u32` is lost.
    pub(crate) fn part(x: u128, of: Px) -> Px {
        u32::try_from(x.min(u128::from(of.0))).map_or(of, Px)
    }
}
