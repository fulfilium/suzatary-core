//! The app's tiles: the bar's destinations as the root's workspaces and a page's route as a
//! leaf, arranged for the room the tiles stand in and printed for the host
//! (`web/src/shell/tiles.ts`), which draws what this answers and decides nothing.
//!
//! The desk holds the tree. An open from a tile and a resize at a divider are acts on it; any
//! other address (Back, a typed hash, the bar) is read with `tiles::parse`. The address is the
//! tree's print, a bare hash wherever the tree is a workspace's home or a home with one page
//! opened from it.
//!
//! The secretary beside a page is the pinned leaf `Route::Aside`, which the shell holds or lets go
//! by its `Dock` say (`web/src/chat/aside.ts`), never by the width, through the module's `Hold`
//! and `Release`. The address carries it only where it has the focus, so every hash a person has
//! prints as before whether it stands or not.

use serde::{Deserialize, Serialize};
use tiles::{
    Act, Arranged, At, Axis, Fit, Home, Id, Leaf, Measures, Node, Page, Part, Pin, Policy, Px,
    Rect, RootRow, Share, Tiles, Tree, Unfit, Viewport,
};
use ts_rs::TS;

use crate::measure;
use crate::route::{Kind, Route};
use crate::runtime::Opens;

pub struct Desk {
    tiles: Tiles<Route>,
    /// The shares set at dividers in this tab, each row a resize left, drawn on every node whose
    /// children are of its kinds. The address does not carry them.
    resized: Vec<Resized>,
    dock: Dock,
}

/// A row of shares a resize left, keyed by the kinds of the node's free children, so the
/// secretary docked beside them or not changes nothing a person set; the share of the child that
/// holds the secretary is kept beside them.
struct Resized {
    kinds: Vec<Kind>,
    free: Vec<Share>,
    pinned: Option<Share>,
}

/// What the shell says of the secretary beside the focused workspace's pages, from the page's
/// reference, the conversation's talk about it and the person's presses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type")]
pub enum Dock {
    /// Not beside the pages: the focused workspace is the secretary's own, or nothing shows it.
    Absent,
    /// Docked at the focused workspace's end (`Act::Hold`): drawn beside its pages where the room
    /// holds it, and behind them, one crumb away, where it does not.
    Held { about: String },
}

/// What the host draws, every rectangle in the room's frame: the box the tiles stand in, under
/// the window's chrome. The root's row is the bar, which the window's chrome draws (`nav.ts`), so
/// it is not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Drawing {
    pub scroll: Scroller,
    pub tiles: Vec<Drawn>,
    /// Every other leaf the tree holds, in its order: a page behind a drawn one, and each page of
    /// the workspaces not drawn. A host keeps a page it has running while its leaf is here.
    pub behind: Vec<Undrawn>,
    /// How many of them a host keeps running at most: each workspace's page but the drawn one's,
    /// and those a workspace holds behind its drawn page.
    #[ts(type = "number")]
    pub keep: usize,
    pub rows: Vec<Crumbs>,
    pub dividers: Vec<Line>,
    pub pinned: Pinned,
}

/// Where the focused workspace's pinned leaf stands, with the address a door to it or from it
/// goes to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum Pinned {
    /// Drawn in the tile `key`, its leaf's route `href`; `without` is the address of the tree
    /// without it, the focus where it stood or on the page before it, and, where it stands alone
    /// in its workspace, the page it is about (else the workspace's home).
    Drawn {
        #[ts(type = "number")]
        key: u64,
        href: String,
        without: String,
    },
    /// Behind the drawn tiles; `to` is the address of the tree focused on it.
    Behind {
        to: String,
    },
    None,
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

/// A leaf the tree holds that is not drawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct Undrawn {
    #[ts(type = "number")]
    pub key: u64,
    /// The leaf's route, `#` first.
    pub href: String,
}

/// A rectangle in pixels at the 16px root, from the room's top left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// One tile alone scrolls the document, so a phone's browser hides its address bar; tiles drawn
/// together each scroll themselves.
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
    /// The address of the tree focused on that leaf, `#` first.
    pub to: String,
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
/// bar's band stands at the top on every viewport (`RootRow::Top`), a phone's too: the chrome
/// places the bar, and the band is cut off before the host sees a rectangle. Every page's least
/// height is zero, so a short window takes no tile away: every tile scrolls, and a tablet's
/// keyboard must not unmount the list beside a todo. A node down (`Below`) brings a least height
/// back.
#[must_use]
pub fn measures() -> Measures {
    Measures {
        divider: Px(measure::DIVIDER),
        row: Px(measure::TAP),
        crumb: Px(0),
        tall: Px(0),
        root_row: RootRow::Top,
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
        Tiles::new(home, Policy::default()).map(|tiles| Desk {
            tiles,
            resized: Vec::new(),
            dock: Dock::Absent,
        })
    }

    /// The address moved: its tree, read against these tiles, unless it is the address of the
    /// tree the desk holds, whose tiles then stay as they are.
    pub fn go(&mut self, address: &str) {
        if address != self.address() {
            let bare = address.strip_prefix('#').unwrap_or(address);
            self.tiles = tiles::parse(bare, &self.tiles);
        }
        self.settle();
    }

    /// The shell's say on the secretary beside the pages, applied at once and after every act.
    pub fn dock(&mut self, dock: Dock) {
        self.dock = dock;
        self.settle();
    }

    /// The tree made to hold what the say says: the secretary held at the focused workspace's end
    /// or let go, and let go from every other workspace; one that has the focus is the person's
    /// and stays.
    fn settle(&mut self) {
        let act = match &self.dock {
            Dock::Absent => Act::Release,
            Dock::Held { about } => Act::Hold(Route::Aside {
                about: about.clone(),
            }),
        };
        self.tiles = tiles::act(self.tiles.clone(), act);
    }

    /// The focused workspace, as the bar's link to it, `#` first: the bar lights it at the act,
    /// before any page of it has drawn.
    #[must_use]
    pub fn workspace(&self) -> String {
        self.tiles.workspace().href()
    }

    /// The tree's print, `#` first, without the secretary beside the pages unless it has the
    /// focus: the shell holds it, and the address names what a person opened.
    #[must_use]
    pub fn address(&self) -> String {
        Desk::print(&tiles::act(self.tiles.clone(), Act::Release))
    }

    fn print(tiles: &Tiles<Route>) -> String {
        format!("#{}", tiles::print(tiles))
    }

    /// A link followed in the tile `key`: the page `href` names opens from that tile, by the
    /// layout's one rule. A workspace's home, as the bar's own links, is that workspace, and so is
    /// any page opened from a tile no longer drawn; so is the secretary, which stands beside the
    /// page it is about and no other.
    pub fn open(&mut self, key: u32, href: &str) {
        let page = Route::parse(href);
        match self.leaf(key) {
            Some(id) if page.home() != page && page.kind() != Kind::Aside => {
                let focused = tiles::act(self.tiles.clone(), Act::Focus(id));
                self.tiles = tiles::follow(focused, page);
            }
            Some(_) | None => return self.go(&page.href()),
        }
        self.settle();
    }

    /// The divider between the tiles `before` and `after` set to `shares`: the two children
    /// beside it take them, and every other child of their node keeps its share. The row of shares
    /// that leaves is what the desk keeps, for every node whose free children are of those kinds,
    /// so a later resize of another divider of the row starts from the row as it stands, no pair
    /// set apart from its row undoes another, and the secretary coming or going keeps them.
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
            let (kinds, free, pinned) = split(row);
            let kept = self
                .resized
                .iter()
                .position(|r| r.kinds == kinds)
                .map(|i| self.resized.remove(i));
            self.resized.push(Resized {
                kinds,
                free,
                pinned: pinned.or(kept.and_then(|r| r.pinned)),
            });
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
            let (kinds, _, _) = split(n);
            let r = self.resized.iter().find(|r| r.kinds == kinds)?;
            let mut free = r.free.iter();
            n.parts()
                .map(|p| {
                    if p.holds_pinned() {
                        Some(r.pinned.unwrap_or(p.share))
                    } else {
                        free.next().copied()
                    }
                })
                .collect()
        })
    }

    /// A crumb of a row: its leaf's route, and the address of the tree focused on that leaf.
    fn crumb(&self, c: &tiles::Crumb<Route>) -> Crumb {
        let focused = tiles::act(self.tiles.clone(), Act::Focus(c.to));
        Crumb {
            href: c.page.href(),
            to: Desk::print(&tiles::act(focused, Act::Release)),
            at: match c.at {
                At::Before => Stands::Before,
                At::Drawn => Stands::Drawn,
                At::After => Stands::After,
            },
        }
    }

    /// Where the focused workspace's pinned leaf stands in a drawing of `drawn` leaves.
    fn pinned(&self, drawn: &[Id]) -> Pinned {
        let leaves = self.tiles.root().focus.tree.leaves();
        let Some(pin) = leaves.last().filter(|l| l.page().pin() == Pin::Pinned) else {
            return Pinned::None;
        };
        let focused = self.tiles.focused().id() == pin.id();
        let drawn_ = |without: String| Pinned::Drawn {
            key: pin.id().key(),
            href: pin.page().href(),
            without,
        };
        match (drawn.contains(&pin.id()), focused, leaves.len()) {
            (true, true, 1) => drawn_(
                pin.page()
                    .subject()
                    .map_or_else(|| self.workspace(), |p| p.href()),
            ),
            (true, true, _) => drawn_(Desk::print(&tiles::act(self.tiles.clone(), Act::Close))),
            (true, false, _) => drawn_(self.address()),
            (false, _, _) => Pinned::Behind {
                to: Desk::print(&tiles::act(self.tiles.clone(), Act::Focus(pin.id()))),
            },
        }
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
        let scroll = if tiles.len() == 1 {
            Scroller::Document
        } else {
            Scroller::Tiles
        };
        let behind = shown
            .root()
            .parts()
            .flat_map(|p| p.tree.leaves())
            .filter(|l| tiles.iter().all(|t| t.id != l.id()))
            .map(|l| Undrawn {
                key: l.id().key(),
                href: l.page().href(),
            })
            .collect();
        let drawn: Vec<Id> = tiles.iter().map(|t| t.id).collect();
        Drawing {
            scroll,
            behind,
            keep: Route::destinations().len() - 1 + usize::from(shown.policy().most().get()),
            pinned: self.pinned(&drawn),
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
                        .map(|c| self.crumb(c))
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
    use tiles::{Px, Viewport};
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

        /// `dock` JSON of `Dock`.
        pub fn dock(&mut self, dock: &str) -> Result<(), wasm_bindgen::JsError> {
            let dock = serde_json::from_str(dock).map_err(|e| {
                wasm_bindgen::JsError::new(&format!("a say that does not read: {e}"))
            })?;
            self.desk.dock(dock);
            Ok(())
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

/// A node's free children by the kind of the leaf each shows, their shares, and the share of the
/// child that holds the secretary.
fn split(n: &Node<Leaf<Route>>) -> (Vec<Kind>, Vec<Share>, Option<Share>) {
    let (pinned, free): (Vec<_>, Vec<_>) = n.parts().partition(|p| p.holds_pinned());
    (
        free.iter()
            .map(|p| p.tree.focused().page().kind())
            .collect(),
        free.iter().map(|p| p.share).collect(),
        pinned.first().map(|p| p.share),
    )
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
