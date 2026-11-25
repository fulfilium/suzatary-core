//! Cases that once failed a law, kept as plain tests (one more had a home that the pin rule now
//! refuses, so it cannot be built). Each row is a home, the viewport of its first
//! state, the acts with the viewport each was seen on, and what the law that failed also drew: a
//! page `p`, acts for the erasure law, an index `i`. A tree reads `a` (across) or `d` (down), `!`
//! for a node that never fits, then its children in brackets: a share digit, `*` before the
//! focused child, and a leaf (`L` list, `C` column, `S` side, `D` slide, then its number). An act
//! is `F<index>` focus, `M<L|R|U|D>` move, `X` close, `O<leaf>` open, `B<leaf>` below,
//! `R<index>.<a>.<b>` resize, then `@<w>x<h>`. An index names place `index * leaves >> 64`.

use tiles::{
    Act, Address, Axis, Dir, Fit, Leaf, Node, Page, Part, Pin, Policy, Px, Share, Tiles, Tree,
    Viewport, act, arrange,
};

use super::check::{covers, drawn, pieces, region, screen};
use super::dock::{end, pages, without};
use super::page::{Kind, MEASURES, Pg, pg};
use super::runs::ids;

type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    u64,
);

const ROWS: [Row; 15] = [
    (
        "a!(*1a(*1L0,1a(*1L0,1S0)))",
        "0x0",
        "F12297829382473034411@0x0 F0@0x0",
        "S0",
        "",
        0,
    ),
    (
        "a!(*1a(*1C0,1L0))",
        "0x0",
        "F9223372036854775808@0x0",
        "S0",
        "XOL0",
        0,
    ),
    ("a!(*1a(*1L0,1a(*1C0,1S0)))", "0x0", "OL2@0x0", "L2", "", 0),
    (
        "a!(*1a(*1L0,1S0),1L0)",
        "0x0",
        "OS0@0x0 F12297829382473034411@0x0 OS0@0x0",
        "S0",
        "",
        0,
    ),
    (
        "a!(*1d(*1L0,1a(*1L0,1L0)))",
        "320x560",
        "F6148914691236517206@320x844 MD@2081x1288",
        "S0",
        "",
        0,
    ),
    (
        "a(*1L0,1L0)",
        "1024x744",
        "F13478991404860502670@1179x997 OS2@1123x1425 X@895x324 X@1471x728 X@1719x1380 X@1362x1034 MU@761x1228 ML@914x325 F9938986129697724681@922x1210",
        "S0",
        "",
        0,
    ),
    (
        "a!(1a(*1a!(*1L0,1L0),1L0),*1L0,1L0,1a(*1L0,1L0))",
        "324x734",
        "MR@1659x958 R0.1.1@564x391 OS1@1638x1097 X@791x1243 OC3@860x475 OS2@703x340 BL2@908x385 OD2@323x627 F12078222889696983751@462x564 F951927046065619411@1178x848 OC1@2088x1069 X@1120x865 OS0@1063x928",
        "S0",
        "",
        0,
    ),
    ("a!(*1a(*1L0,1L0))", "0x0", "BS0@0x0", "S0", "", 0),
    (
        "a!(*1a(*1L0,1a(*1L0,1L0)),1d(*1D0,1S0),1L0,1L0,1L0)",
        "0x0",
        "F9223372036854775808@0x0 BD0@1024x696",
        "S0",
        "",
        0,
    ),
    (
        "a!(1L0,1L0,1a(*1L0,1L0),*3d(1d(*2C2,2L1),*2a(*2D3,2D3)))",
        "1317x960",
        "F3872061434653756627@1927x834 OL2@763x1348 F912935306611628501@896x415 X@1262x957",
        "S0",
        "",
        15_229_322_776_939_092_263,
    ),
    (
        "a!(*1a(*1a(*1L0,1D1),1L0))",
        "0x0",
        "",
        "S0",
        "",
        6_148_914_691_236_517_206,
    ),
    ("a!(*1a(*1L0,1a(*1L0,1S0)))", "0x0", "OS0@0x0", "S0", "", 0),
    ("a!(*1a(*1L0,1a(1L0,*1S0)))", "0x0", "X@0x0", "S0", "", 0),
    (
        "a!(*1a(*1L0,1a(1C0,*1S0)),1L0)",
        "0x0",
        "X@744x1024",
        "S0",
        "",
        0,
    ),
    (
        "a!(*1d(*1L0,1a(1L0,*2S0)))",
        "0x0",
        "X@1104x600",
        "S0",
        "",
        0,
    ),
];

struct Reader<'s>(&'s [u8]);

impl Reader<'_> {
    fn take(&mut self) -> u8 {
        let b = self.0[0];
        self.0 = &self.0[1..];
        b
    }

    fn peek(&self) -> u8 {
        self.0[0]
    }

    fn leaf(&mut self) -> Pg {
        let kind = match self.take() {
            b'L' => Kind::List,
            b'C' => Kind::Column,
            b'S' => Kind::Side,
            _ => Kind::Slide,
        };
        pg(kind, self.take() - b'0')
    }

    fn node(&mut self) -> Node<Pg> {
        let axis = if self.take() == b'a' {
            Axis::Across
        } else {
            Axis::Down
        };
        let fit = if self.peek() == b'!' {
            self.take();
            Fit::Never
        } else {
            Fit::Fits
        };
        self.take();
        let mut parts = Vec::new();
        let mut at = 0;
        loop {
            let starred = self.peek() == b'*';
            if starred {
                self.take();
                at = parts.len();
            }
            let share = Share::of(self.take() - b'0').expect("a share from one");
            let tree = if matches!(self.peek(), b'a' | b'd') {
                Tree::Node(self.node())
            } else {
                Tree::Leaf(self.leaf())
            };
            parts.push(Part::new(share, tree));
            if self.take() == b')' {
                break;
            }
        }
        let mut parts = parts.into_iter();
        let first = parts.next().expect("a node has a child");
        Node::new(axis, fit, first, parts.collect(), at)
    }
}

fn leaf_of(s: &str) -> Pg {
    Reader(s.as_bytes()).leaf()
}

fn place(index: u64, of: usize) -> usize {
    usize::try_from((u128::from(index) * of as u128) >> 64).expect("a place is below the length")
}

fn viewport_of(text: &str) -> Viewport {
    let (width, height) = text.split_once('x').expect("a viewport has a height");
    Viewport {
        w: Px(width.parse().expect("a width")),
        h: Px(height.parse().expect("a height")),
    }
}

/// The act a step names on `tiles`, and the viewport it was seen on.
fn step(tiles: &Tiles<Pg>, text: &str) -> (Act<Pg>, Viewport) {
    let (named, seen) = text.split_once('@').expect("a step has its viewport");
    let leaves = ids(tiles);
    let (head, rest) = named.split_at(1);
    let made = match head {
        "F" => Act::Focus(leaves[place(rest.parse().expect("an index"), leaves.len())]),
        "M" => Act::Move(match rest {
            "L" => Dir::Left,
            "R" => Dir::Right,
            "U" => Dir::Up,
            _ => Dir::Down,
        }),
        "X" => Act::Close,
        "O" => Act::Open(leaf_of(rest)),
        "B" => Act::Below(leaf_of(rest)),
        _ => {
            let mut fields = rest.split('.');
            let before = place(
                fields
                    .next()
                    .and_then(|i| i.parse().ok())
                    .expect("an index"),
                leaves.len(),
            );
            let mut share = || {
                let weight = fields.next().and_then(|w| w.parse().ok());
                Share::of(weight.expect("a weight")).expect("a weight from one")
            };
            let shares = [share(), share()];
            Act::Resize {
                before: leaves[before],
                after: leaves[(before + 1) % leaves.len()],
                shares,
            }
        }
    };
    (made, viewport_of(seen))
}

fn state_laws(t: &Tiles<Pg>, at: Viewport) {
    assert_eq!(t.check(), Ok(()));
    let a = arrange(t, at, &MEASURES);
    assert_eq!(covers(&pieces(&a), screen(at)), Ok(()));
    assert_eq!(region(&a), Ok(()));
    assert!(drawn(&a).contains(&t.focused().id()));
}

/// The laws of a held page, from a state whose focus is free and whose workspace has no pinned end.
fn dock_laws(tiles: &Tiles<Pg>, page: &Pg, acts: &str) {
    if end(tiles).is_some()
        || tiles.focused().page().pin() == Pin::Pinned
        || page.pin() == Pin::Free
    {
        return;
    }
    let (held, released) = (
        act(tiles.clone(), Act::Hold(page.clone())),
        act(tiles.clone(), Act::Release),
    );
    assert_eq!(held.focused().id(), tiles.focused().id());
    assert_eq!(held.check(), Ok(()));
    assert_eq!(act(held.clone(), Act::Release).root(), released.root());
    let docked = end(&held)
        .map(Leaf::id)
        .expect("a held page stands at the end");
    let (mut with, mut bare) = (held, released);
    let mut rest = acts;
    while !rest.is_empty() {
        let n = if rest.starts_with('O') { 3 } else { 1 };
        let (one, tail) = rest.split_at(n);
        rest = tail;
        let at = Viewport { w: Px(0), h: Px(0) };
        let (a, _) = step(&with, &format!("{one}@0x0"));
        let (b, _) = step(&bare, &format!("{one}@{}x{}", at.w.0, at.h.0));
        with = act(with, a);
        bare = act(bare, b);
        assert_eq!(without(with.root(), docked), pages(&bare), "after {one}");
    }
}

/// Every case replays: each state is well formed and drawn exactly, a held page changes nothing a
/// person did, and an open of a page shown focuses it.
#[test]
fn the_cases_that_once_failed_a_law_hold_it() {
    for (n, (home, first, steps, p, acts, i)) in ROWS.into_iter().enumerate() {
        let mut tiles = Tiles::new(
            Reader(home.as_bytes()).node(),
            Policy::default().with_most(std::num::NonZeroU8::MAX),
        )
        .unwrap_or_else(|e| panic!("case {n}: {e:?}"));
        let mut at = viewport_of(first);
        let p = leaf_of(p);
        let mut states = vec![(tiles.clone(), at)];
        for text in steps.split_whitespace() {
            let (made, seen) = step(&tiles, text);
            tiles = act(tiles, made);
            at = seen;
            states.push((tiles.clone(), at));
        }
        for (t, at) in &states {
            state_laws(t, *at);
            dock_laws(t, &p, acts);
            let shown = t.root().focus.tree.leaves();
            let page = shown[place(i, shown.len())].page();
            let first = shown.iter().find(|l| l.page() == page).map(|l| l.id());
            let opened = tiles::follow(t.clone(), page.clone());
            assert_eq!(Some(opened.focused().id()), first, "case {n}");
        }
    }
}

/// A bare address names one workspace of a home of one list under a bound of one.
#[test]
fn a_bare_address_of_a_column_lands_in_the_only_workspace_under_a_bound_of_one() {
    let home = Node::of(
        Axis::Across,
        Fit::Never,
        Part::new(Share::ONE, Tree::Leaf(pg(Kind::List, 0))),
    );
    let policy = Policy::default().with_most(std::num::NonZeroU8::MIN);
    let lineage = Tiles::new(home, policy).expect("a list is a home");
    let t = tiles::parse(&pg(Kind::Column, 0).print(), &lineage);
    assert_eq!((t.root().index(), t.check()), (0, Ok(())));
}
