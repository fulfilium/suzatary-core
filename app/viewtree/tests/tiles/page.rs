//! An invented page type: a list, a column, a pinned side page and a paged slide. Its prints hold
//! every character of the grammar, a leading digit, and the empty string, so the address must
//! escape them all.

use std::num::NonZeroU32;

use viewtree::tiles::{Measures, Page, Pin, Px, Ratio, Shape, Share};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    List,
    Column,
    Side,
    Slide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pg {
    pub kind: Kind,
    pub n: u8,
}

pub fn pg(kind: Kind, n: u8) -> Pg {
    Pg { kind, n }
}

pub const SIXTEEN_NINE: Ratio = Ratio {
    w: NonZeroU32::new(16).expect("a literal"),
    h: NonZeroU32::new(9).expect("a literal"),
};

pub const MEASURES: Measures = Measures {
    divider: Px(24),
    row: Px(44),
    crumb: Px(72),
    tall: Px(320),
    foot: Px(992),
};

impl Pg {
    /// A page this one opens: of another kind, free, and of the shape `opens` declares.
    pub fn opened(&self) -> Pg {
        match self.kind {
            Kind::Column => pg(Kind::Slide, self.n.wrapping_add(1)),
            Kind::List | Kind::Side | Kind::Slide => pg(Kind::Column, self.n),
        }
    }
}

impl Page for Pg {
    type Kind = Kind;

    fn kind(&self) -> Kind {
        self.kind
    }

    fn shape(&self) -> Shape {
        match self.kind {
            Kind::List => Shape::Flows { least: Px(320) },
            Kind::Column => Shape::Flows { least: Px(640) },
            Kind::Side => Shape::Flows { least: Px(352) },
            Kind::Slide => Shape::Paged {
                least: Px(480),
                ratio: SIXTEEN_NINE,
            },
        }
    }

    fn pin(&self) -> Pin {
        match self.kind {
            Kind::Side => Pin::Pinned,
            Kind::List | Kind::Column | Kind::Slide => Pin::Free,
        }
    }

    fn share(&self) -> Share {
        match self.kind {
            Kind::List | Kind::Side => Share::One,
            Kind::Column | Kind::Slide => Share::Two,
        }
    }

    fn opens(&self) -> Shape {
        self.opened().shape()
    }

    /// A slide is opened from its todo, which is opened from the first list: a chain of two.
    fn home(&self) -> Pg {
        match self.kind {
            Kind::Column => pg(Kind::List, 0),
            Kind::Slide if self.n > 0 => pg(Kind::Column, self.n),
            Kind::List | Kind::Side | Kind::Slide => self.clone(),
        }
    }

    fn print(&self) -> String {
        let n = self.n;
        match self.kind {
            Kind::List => format!("list/{n}"),
            Kind::Column => format!("{n}todo-(~*)"),
            Kind::Side => format!("side'{n},^|!:"),
            Kind::Slide if n == 0 => String::new(),
            Kind::Slide => format!("deck {n}%"),
        }
    }

    fn parse(s: &str) -> Option<Pg> {
        let num = |t: &str| t.parse::<u8>().ok();
        if s.is_empty() {
            return Some(pg(Kind::Slide, 0));
        }
        if let Some(n) = s.strip_prefix("list/") {
            return Some(pg(Kind::List, num(n)?));
        }
        if let Some(n) = s.strip_suffix("todo-(~*)") {
            return Some(pg(Kind::Column, num(n)?));
        }
        if let Some(n) = s
            .strip_prefix("side'")
            .and_then(|t| t.strip_suffix(",^|!:"))
        {
            return Some(pg(Kind::Side, num(n)?));
        }
        let n = num(s.strip_prefix("deck ")?.strip_suffix('%')?)?;
        (n > 0).then(|| pg(Kind::Slide, n))
    }
}
