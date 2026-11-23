//! The app's tiles: the bar's destinations as the root's workspaces and a page's route as a
//! leaf, arranged for the room the tiles stand in and printed for the host
//! (`web/src/shell/tiles.ts`), which draws what this answers and decides nothing.

use serde::Serialize;
use ts_rs::TS;
use viewtree::tiles::{
    self, Arranged, At, Axis, Fit, Measures, Node, Page, Part, Px, Rect, Share, Tiles, Tree, Unfit,
    Viewport,
};

use crate::measure;
use crate::route::Route;
use crate::runtime::Opens;

pub struct Desk {
    tiles: Tiles<Route>,
}

/// What the host draws, every rectangle in the room's frame: the box the tiles stand in, under
/// the window's chrome. The root's row is the bar, which the chrome draws (`nav.ts`) until the
/// address carries the tree, so it is not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Drawing {
    pub scroll: Scroller,
    pub tiles: Vec<Drawn>,
    pub rows: Vec<Crumbs>,
    pub dividers: Vec<Line>,
}

/// A drawn tile; `key` names its element and its program across drawings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Drawn {
    #[ts(type = "number")]
    pub key: u64,
    /// The leaf's route, `#` first.
    pub href: String,
    pub rect: Area,
    /// Where the page stands in `rect`: all of it, for every page this app has.
    pub page: Area,
    pub opens: Opens,
}

/// A rectangle in pixels at the 16px root, from the room's top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// One tile alone scrolls the document; tiles drawn together each scroll themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Scroller {
    Document,
    Tiles,
}

/// A node's crumb row, named by the node's place: each child's index from the root down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Crumbs {
    #[ts(type = "number[]")]
    pub node: Vec<usize>,
    pub rect: Area,
    /// Every child of the row's node, in its order.
    pub crumbs: Vec<Crumb>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Crumb {
    /// The route of the leaf a tap on it reaches, `#` first.
    pub href: String,
    pub at: Stands,
}

/// Where a crumb's child stands against the drawn ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Stands {
    Before,
    Drawn,
    After,
}

/// A divider between two drawn children of the node at `node`, named by the tile before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Line {
    #[ts(type = "number[]")]
    pub node: Vec<usize>,
    #[ts(type = "number")]
    pub before: u64,
    pub rect: Area,
    pub axis: Along,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Along {
    Across,
    Down,
}

/// The arrangement's lengths, from the tokens. A crumb row below the root shows every crumb. The
/// bar's band stands at the top (`foot` zero): the chrome places the bar, and the band is cut off
/// before the host sees a rectangle. Every page's least height is zero, so a short window takes
/// no tile away: every tile scrolls, and a tablet's keyboard must not unmount the list beside a
/// todo. A node down (`Below`) brings a least height back.
#[must_use]
pub fn measures() -> Measures {
    Measures {
        divider: Px(measure::DIVIDER),
        row: Px(measure::TAP),
        crumb: Px(0),
        tall: Px(0),
        foot: Px(0),
    }
}

impl Desk {
    /// The root: the bar's destinations, never side by side, each its home page alone.
    ///
    /// # Errors
    ///
    /// `Unfit` where the destinations are not a well-formed home.
    pub fn new() -> Result<Desk, Unfit> {
        let part = |r: Route| Part::new(Share::One, Tree::Leaf(r));
        let [first, rest @ ..] = Route::destinations();
        let home = Node::new(
            Axis::Across,
            Fit::Never,
            part(first),
            rest.into_iter().map(part).collect(),
            0,
        );
        Tiles::new(home).map(|tiles| Desk { tiles })
    }

    /// The address moved: its tree, read against these tiles.
    pub fn go(&mut self, hash: &str) {
        self.tiles = tiles::parse(&Route::parse(hash).print(), &self.tiles);
    }

    #[must_use]
    pub fn tiles(&self) -> &Tiles<Route> {
        &self.tiles
    }

    /// `arrange` for the room the tiles stand in, with each tile's route.
    #[must_use]
    pub fn drawing(&self, room: Viewport) -> Drawing {
        let m = measures();
        let at = Viewport {
            w: room.w,
            h: room.h + m.row,
        };
        let Arranged {
            tiles,
            dividers,
            rows,
        } = tiles::arrange(&self.tiles, at, &m);
        let area = |r: Rect| Area {
            x: r.x.0,
            y: (r.y - m.row).0,
            w: r.w.0,
            h: r.h.0,
        };
        // Every id `arrange` names is a leaf of these tiles.
        let href = |id| self.tiles.page(id).map(Route::href);
        let scroll = if tiles.iter().any(|t| t.scroll == tiles::Scroller::Document) {
            Scroller::Document
        } else {
            Scroller::Tiles
        };
        Drawing {
            scroll,
            tiles: tiles
                .into_iter()
                .filter_map(|t| {
                    Some(Drawn {
                        key: t.id.key(),
                        href: href(t.id)?,
                        rect: area(t.rect),
                        page: area(t.page),
                        opens: match t.opens {
                            tiles::Opens::Beside => Opens::Beside,
                            tiles::Opens::Over => Opens::Over,
                        },
                    })
                })
                .collect(),
            rows: rows
                .into_iter()
                .filter(|r| !r.node.is_empty())
                .map(|r| Crumbs {
                    rect: area(r.rect),
                    crumbs: r
                        .shown
                        .iter()
                        .chain(&r.more)
                        .filter_map(|c| {
                            Some(Crumb {
                                href: href(c.to)?,
                                at: match c.at {
                                    At::Before => Stands::Before,
                                    At::Drawn => Stands::Drawn,
                                    At::After => Stands::After,
                                },
                            })
                        })
                        .collect(),
                    node: r.node,
                })
                .collect(),
            dividers: dividers
                .into_iter()
                .map(|d| Line {
                    before: d.before.key(),
                    rect: area(d.rect),
                    axis: match d.axis {
                        Axis::Across => Along::Across,
                        Axis::Down => Along::Down,
                    },
                    node: d.node,
                })
                .collect(),
        }
    }
}

/// The desk as the browser holds it: `drawing` answers JSON of `Drawing`.
#[cfg(target_arch = "wasm32")]
mod host {
    use viewtree::tiles::{Px, Viewport};
    use wasm_bindgen::prelude::wasm_bindgen;

    #[wasm_bindgen(js_name = Desk)]
    pub struct Host {
        desk: super::Desk,
    }

    #[wasm_bindgen(js_class = Desk)]
    impl Host {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Result<Host, wasm_bindgen::JsError> {
            super::Desk::new()
                .map(|desk| Host { desk })
                .map_err(|e| wasm_bindgen::JsError::new(&format!("{e:?}")))
        }

        pub fn go(&mut self, hash: &str) {
            self.desk.go(hash);
        }

        /// The room's `w` and `h` in pixels at the 16px root.
        pub fn drawing(&self, w: u32, h: u32) -> Result<String, wasm_bindgen::JsError> {
            serde_json::to_string(&self.desk.drawing(Viewport { w: Px(w), h: Px(h) }))
                .map_err(|e| wasm_bindgen::JsError::new(&e.to_string()))
        }
    }
}
