//! The app's tiles: the bar's destinations as the root's workspaces and a page's route as a
//! leaf, arranged for the room the tiles stand in and printed for the host
//! (`web/src/shell/tiles.ts`), which draws what this answers and decides nothing.
//!
//! The desk holds the tree. An open from a tile and a resize at a divider are acts on it; any
//! other address (Back, a typed hash, the bar) is read with `tiles::parse`. The address is the
//! tree's print, a bare hash wherever the tree is a workspace's home or a home with one page
//! opened from it.

use serde::Serialize;
use ts_rs::TS;
use viewtree::tiles::{
    self, Act, Arranged, At, Axis, Fit, Id, Leaf, Measures, Node, Page, Part, Px, Rect, Share,
    Tiles, Tree, Unfit, Viewport,
};

use crate::measure;
use crate::route::{Kind, Route};
use crate::runtime::Opens;

pub struct Desk {
    tiles: Tiles<Route>,
    /// The shares set at dividers in this tab: each row a resize left, by the kinds of its
    /// children in order, drawn on every node of those kinds the desk holds. The address does not
    /// carry them.
    resized: Vec<(Vec<Kind>, Vec<Share>)>,
}

/// What the host draws, every rectangle in the room's frame: the box the tiles stand in, under
/// the window's chrome. The root's row is the bar, which the window's chrome draws (`nav.ts`), so
/// it is not here.
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

/// A divider between two drawn children of the node at `node`, named by the tiles either side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Line {
    #[ts(type = "number[]")]
    pub node: Vec<usize>,
    #[ts(type = "number")]
    pub before: u64,
    #[ts(type = "number")]
    pub after: u64,
    pub rect: Area,
    pub axis: Along,
    /// Where it may stand, in order along its axis; `now` indexes the one it stands at.
    pub stops: Vec<Notch>,
    #[ts(type = "number")]
    pub now: usize,
}

/// A place a divider snaps to: its start along its axis in the room's frame, and the weights, 1
/// to 3, of the shares either side of it there, which `Desk::resize` takes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct Notch {
    pub at: u32,
    pub shares: [u32; 2],
}

fn weight(s: Share) -> u32 {
    match s {
        Share::One => 1,
        Share::Two => 2,
        Share::Three => 3,
    }
}

/// The share of a weight `weight` gives back; none for any other number.
#[must_use]
pub fn share(weight: u32) -> Option<Share> {
    match weight {
        1 => Some(Share::One),
        2 => Some(Share::Two),
        3 => Some(Share::Three),
        _ => None,
    }
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
        Tiles::new(home).map(|tiles| Desk {
            tiles,
            resized: Vec::new(),
        })
    }

    /// The address moved: its tree, read against these tiles, unless it is the address of the
    /// tree the desk holds, whose tiles then stay as they are.
    pub fn go(&mut self, address: &str) {
        if address != self.address() {
            let bare = address.strip_prefix('#').unwrap_or(address);
            self.tiles = tiles::parse(bare, &self.tiles);
        }
    }

    /// The focused workspace, as the bar's link to it, `#` first: the bar lights it at the act,
    /// before any page of it has drawn.
    #[must_use]
    pub fn workspace(&self) -> String {
        self.tiles.workspace().href()
    }

    /// The tree's print, `#` first.
    #[must_use]
    pub fn address(&self) -> String {
        format!("#{}", tiles::print(&self.tiles))
    }

    /// A link followed in the tile `key`: the page `href` names opens from that tile, by the
    /// layout's one rule. A workspace's home, as the bar's own links, is that workspace, and so is
    /// any page opened from a tile no longer drawn.
    pub fn open(&mut self, key: u32, href: &str) {
        let page = Route::parse(href);
        match self.leaf(key) {
            Some(id) if page.home() != page => {
                let focused = tiles::act(self.tiles.clone(), Act::Focus(id));
                self.tiles = tiles::act(focused, Act::Open(page));
            }
            Some(_) | None => self.go(&page.href()),
        }
    }

    /// The divider between the tiles `before` and `after` set to `shares`: the two children
    /// beside it take them, and every other child of their node keeps its share. The row of shares
    /// that leaves is what the desk keeps, for every node whose children are of those kinds, so a
    /// later resize of another divider of the row starts from the row as it stands, and no pair
    /// set apart from its row undoes another.
    pub fn resize(&mut self, before: u32, after: u32, shares: [Share; 2]) {
        let (Some(b), Some(a)) = (self.leaf(before), self.leaf(after)) else {
            return;
        };
        let acted = tiles::act(
            self.shown(),
            Act::Resize {
                before: b,
                after: a,
                shares,
            },
        );
        if let Some(row) = row(acted.root(), b, a) {
            let kinds = kinds(row);
            self.resized.retain(|(k, _)| *k != kinds);
            self.resized
                .push((kinds, row.parts().map(|p| p.share).collect()));
        }
    }

    #[must_use]
    pub fn tiles(&self) -> &Tiles<Route> {
        &self.tiles
    }

    fn leaf(&self, key: u32) -> Option<Id> {
        self.tiles
            .root()
            .parts()
            .flat_map(|p| p.tree.leaves())
            .map(tiles::Leaf::id)
            .find(|id| id.key() == u64::from(key))
    }

    /// The tree with the rows of shares set in this tab.
    fn shown(&self) -> Tiles<Route> {
        self.tiles.clone().reshared(&|n: &Node<Leaf<Route>>| {
            let k = kinds(n);
            self.resized
                .iter()
                .find(|(kinds, _)| *kinds == k)
                .map(|(_, s)| s.clone())
        })
    }

    /// `arrange` for the room the tiles stand in, with each tile's route.
    #[must_use]
    pub fn drawing(&self, room: Viewport) -> Drawing {
        let m = measures();
        let at = Viewport {
            w: room.w,
            h: room.h + m.row,
        };
        let shown = self.shown();
        let Arranged {
            tiles,
            dividers,
            rows,
        } = tiles::arrange(&shown, at, &m);
        let area = |r: Rect| Area {
            x: r.x.0,
            y: (r.y - m.row).0,
            w: r.w.0,
            h: r.h.0,
        };
        let scroll = if tiles.iter().any(|t| t.scroll == tiles::Scroller::Document) {
            Scroller::Document
        } else {
            Scroller::Tiles
        };
        Drawing {
            scroll,
            tiles: tiles
                .into_iter()
                .map(|t| Drawn {
                    key: t.id.key(),
                    href: t.page.href(),
                    rect: area(t.rect),
                    page: area(t.fill),
                    opens: match t.opens {
                        tiles::Opens::Beside => Opens::Beside,
                        tiles::Opens::Over => Opens::Over,
                    },
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
                        .map(|c| Crumb {
                            href: c.page.href(),
                            at: match c.at {
                                At::Before => Stands::Before,
                                At::Drawn => Stands::Drawn,
                                At::After => Stands::After,
                            },
                        })
                        .collect(),
                    node: r.node,
                })
                .collect(),
            dividers: dividers
                .into_iter()
                .map(|d| {
                    let row = match d.axis {
                        Axis::Across => Px(0),
                        Axis::Down => m.row,
                    };
                    Line {
                        before: d.before.key(),
                        after: d.after.key(),
                        rect: area(d.rect),
                        axis: match d.axis {
                            Axis::Across => Along::Across,
                            Axis::Down => Along::Down,
                        },
                        now: d.now,
                        stops: d
                            .stops
                            .into_iter()
                            .map(|s| Notch {
                                at: (s.at - row).0,
                                shares: s.shares.map(weight),
                            })
                            .collect(),
                        node: d.node,
                    }
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

        pub fn go(&mut self, address: &str) {
            self.desk.go(address);
        }

        pub fn address(&self) -> String {
            self.desk.address()
        }

        pub fn workspace(&self) -> String {
            self.desk.workspace()
        }

        pub fn open(&mut self, key: u32, href: &str) {
            self.desk.open(key, href);
        }

        /// `before` and `after` the shares' weights, 1 to 3; any other weight changes nothing.
        pub fn resize(&mut self, from: u32, to: u32, before: u32, after: u32) {
            if let (Some(b), Some(a)) = (super::share(before), super::share(after)) {
                self.desk.resize(from, to, [b, a]);
            }
        }

        /// The room's `w` and `h` in pixels at the 16px root.
        pub fn drawing(&self, w: u32, h: u32) -> Result<String, wasm_bindgen::JsError> {
            serde_json::to_string(&self.desk.drawing(Viewport { w: Px(w), h: Px(h) }))
                .map_err(|e| wasm_bindgen::JsError::new(&e.to_string()))
        }
    }
}

/// A node's children, each by the kind of the leaf it shows.
fn kinds(n: &Node<Leaf<Route>>) -> Vec<Kind> {
    n.parts().map(|p| p.tree.focused().page().kind()).collect()
}

/// The node whose child holding `before` stands right before its child holding `after`.
fn row(n: &Node<Leaf<Route>>, before: Id, after: Id) -> Option<&Node<Leaf<Route>>> {
    let holds = |id: Id| {
        n.parts()
            .position(|p| p.tree.leaves().iter().any(|l| l.id() == id))
    };
    match (holds(before), holds(after)) {
        (Some(i), Some(j)) if j == i + 1 => Some(n),
        (Some(i), Some(j)) if i == j => n.parts().nth(i).and_then(|p| match &p.tree {
            Tree::Node(c) => row(c, before, after),
            Tree::Leaf(_) => None,
        }),
        (Some(_) | None, Some(_) | None) => None,
    }
}
