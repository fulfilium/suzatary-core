//! The secretary held beside the pages is erased by its absence: whatever the shell says of it,
//! a person's pages, their order, the focus among them, the address and the dividers' shares are
//! those of the same acts with the secretary never said.

use proptest::prelude::*;
use proptest::sample::Index;
use suzatary_pages::desk::{Desk, Dock, Pinned, share};
use suzatary_pages::route::{Kind, Route};
use tiles::{Act, Dir, Leaf, Node, Page, Pin, Px, Tiles, Tree, Viewport};

/// The shell's say on the secretary.
#[derive(Debug, Clone)]
enum Say {
    Absent,
    Held(String),
}

fn tell(desk: &mut Desk, s: &Say) {
    desk.dock(match s {
        Say::Absent => Dock::Absent,
        Say::Held(about) => Dock::Held {
            about: about.clone(),
        },
    });
}

/// A person's act, or the shell's say. An act in the secretary's own tile is made in the world
/// without it from the free leaf before it.
#[derive(Debug, Clone)]
enum Step {
    /// A link followed in the tile of the person's page at the index.
    Open(Index, String),
    /// A link followed in the tile of a page of any workspace, by its place among them all.
    OpenAny(Index, String),
    /// A link followed in the secretary's tile.
    OpenInAside(String),
    /// A crumb taken to the person's page at the index.
    Focus(Index),
    /// The secretary's crumb taken.
    FocusAside,
    /// Back, or Forward, to an address the run went through.
    Back(Index),
    /// A page of any workspace focused, its tree kept: the bar's visit and a crumb's return.
    Visit(Index),
    /// A tree address typed.
    Typed(String),
    /// The divider between two of the person's pages, the one at the index, set.
    Resize(Index, u32, u32),
    /// The divider beside the secretary set: a share no tree without it has.
    ResizeAside(u32, u32),
    /// A crumb taken, by its place in a drawing of a room of the width at the index.
    Crumb(Index, Index),
    /// Show where the secretary stands behind the drawn pages: the host goes to `Pinned::Behind`'s
    /// address.
    Show,
    /// Hide where the secretary has the focus: the host goes to `Pinned::Drawn`'s address without it.
    Hide,
    Say(Say),
}

/// The rooms crumbs are read at: a phone's, and a window that draws two tiles.
const ROOMS: [Viewport; 2] = [
    Viewport {
        w: Px(400),
        h: Px(800),
    },
    Viewport {
        w: Px(900),
        h: Px(800),
    },
];

fn about() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["", "todo:life/a", "todo:life/b", "mail:m"]).prop_map(str::to_owned)
}

fn say() -> impl Strategy<Value = Say> {
    prop_oneof![1 => Just(Say::Absent), 3 => about().prop_map(Say::Held)]
}

fn link() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => prop::sample::select(vec![
            "#/todo/a", "#/todo/b", "#/todo/c", "#/todo/d", "#/todo/e", "#/mail/m",
            "#/mail/thread/t", "#/secretary/c", "#/briefings/2026-01-02", "#/inbox", "#/",
        ]).prop_map(str::to_owned),
        1 => about().prop_map(|about| Route::Aside { about }.href()),
    ]
}

/// A tree act a typed address is made by: nested nodes, nodes down and tabs among them.
#[derive(Debug, Clone)]
enum Made {
    Open(String),
    /// A leaf of any workspace focused, by its place among them all.
    Switch(Index),
    Below(String),
    Close,
    Focus(Index),
    Move(Dir),
}

fn made() -> impl Strategy<Value = Made> {
    let free = || {
        prop::sample::select(vec![
            "#/todo/a",
            "#/todo/b",
            "#/todo/c",
            "#/mail/m",
            "#/mail/thread/t",
            "#/inbox",
            "#/secretary/c",
            "#/briefings/2026-01-02",
        ])
        .prop_map(str::to_owned)
    };
    let dir = prop_oneof![
        Just(Dir::Left),
        Just(Dir::Right),
        Just(Dir::Up),
        Just(Dir::Down)
    ];
    prop_oneof![
        3 => free().prop_map(Made::Open),
        2 => free().prop_map(Made::Below),
        1 => Just(Made::Close),
        2 => any::<Index>().prop_map(Made::Focus),
        1 => any::<Index>().prop_map(Made::Switch),
        1 => dir.prop_map(Made::Move),
    ]
}

fn typed() -> impl Strategy<Value = String> {
    prop::collection::vec(made(), 0..8).prop_map(|acts| {
        let desk = Desk::new().expect("the destinations are a home");
        let t = acts.into_iter().fold(desk.tiles().clone(), |t, m| {
            let a = match m {
                Made::Open(h) => return tiles::follow(t, Route::parse(&h)),
                Made::Below(h) => Act::Below(Route::parse(&h)),
                Made::Close => Act::Close,
                Made::Focus(i) => {
                    let ids: Vec<_> = t
                        .root()
                        .focus
                        .tree
                        .leaves()
                        .iter()
                        .map(|l| l.id())
                        .collect();
                    Act::Focus(ids[i.index(ids.len())])
                }
                Made::Switch(i) => {
                    let ids: Vec<_> = t
                        .root()
                        .parts()
                        .flat_map(|p| p.tree.leaves())
                        .map(Leaf::id)
                        .collect();
                    Act::Focus(ids[i.index(ids.len())])
                }
                Made::Move(d) => Act::Move(d),
            };
            tiles::act(t, a)
        });
        format!("#{}", tiles::print(&t))
    })
}

/// A typed address holding the secretary: bare, alone in its workspace, and in a node down.
fn typed_aside() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "#/aside/todo%3Alife%2Fa",
        "#/aside/mail%3Aabcdef12",
        "#/aside",
        "#/aside/todo%3Alife%2Fa~|!(*,,,,,,,)",
        "#/aside/todo%3Alife%2Fa~|!(-(2%2Ftodo%2Fa,*),,,,,,,)",
        "#/aside~|!(|(%2F,2%2Ftodo%2Fb,*),,,,,,,)",
    ])
    .prop_map(str::to_owned)
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (any::<Index>(), link()).prop_map(|(i, h)| Step::Open(i, h)),
        2 => link().prop_map(Step::OpenInAside),
        2 => any::<Index>().prop_map(Step::Focus),
        1 => Just(Step::FocusAside),
        2 => any::<Index>().prop_map(Step::Back),
        2 => any::<Index>().prop_map(Step::Visit),
        1 => typed().prop_map(Step::Typed),
        1 => typed_aside().prop_map(Step::Typed),
        2 => (any::<Index>(), link()).prop_map(|(i, h)| Step::OpenAny(i, h)),
        1 => (1u32..4, 1u32..4).prop_map(|(a, b)| Step::ResizeAside(a, b)),
        3 => (any::<Index>(), any::<Index>()).prop_map(|(i, w)| Step::Crumb(i, w)),
        1 => Just(Step::Show),
        1 => Just(Step::Hide),
        2 => (any::<Index>(), 1u32..4, 1u32..4).prop_map(|(i, a, b)| Step::Resize(i, a, b)),
        5 => say().prop_map(Step::Say),
    ]
}

fn workspace(t: &Tiles<Route>) -> Vec<&Leaf<Route>> {
    t.root().focus.tree.leaves()
}

fn free(t: &Tiles<Route>) -> Vec<&Leaf<Route>> {
    workspace(t)
        .into_iter()
        .filter(|l| l.page().pin() == Pin::Free)
        .collect()
}

fn pinned(t: &Tiles<Route>) -> Option<&Leaf<Route>> {
    workspace(t)
        .into_iter()
        .find(|l| l.page().pin() == Pin::Pinned)
}

/// The tree with every focused secretary closed, the focus on the page before it.
fn released(t: &Tiles<Route>) -> Tiles<Route> {
    let close = |t: Tiles<Route>, l: &Leaf<Route>| match l.page().pin() {
        Pin::Pinned => tiles::act(tiles::act(t, Act::Focus(l.id())), Act::Close),
        Pin::Free => t,
    };
    // Every other workspace's focused secretary first, the person's place in each kept.
    let here = t.root().index();
    let others: Vec<Leaf<Route>> = t
        .root()
        .parts()
        .enumerate()
        .filter(|(i, _)| *i != here)
        .map(|(_, p)| p.tree.focused().clone())
        .collect();
    let r = others.iter().fold(t.clone(), close);
    match t.focused().page().pin() {
        Pin::Pinned => close(r, t.focused()),
        Pin::Free => tiles::act(r, Act::Focus(t.focused().id())),
    }
}

/// Where the focus stands among the person's pages: the focused one, or, in the secretary, the
/// page before it.
fn focus(t: &Tiles<Route>) -> Option<usize> {
    let r = released(t);
    let at = r.focused().id();
    free(&r).iter().position(|l| l.id() == at)
}

/// The tree the address of `t` focused on `id` prints, as a crumb's address is made.
fn focused_on(t: &Tiles<Route>, id: tiles::Id) -> String {
    format!("#{}", tiles::print(&tiles::act(t.clone(), Act::Focus(id))))
}

const WIDE: Viewport = Viewport {
    w: Px(4000),
    h: Px(1200),
};

/// The dividers between two of a person's pages on a room that draws every tile: their pages
/// either side, and the weights of their shares.
fn dividers(desk: &Desk) -> Vec<(String, String, [u32; 2])> {
    let d = desk.drawing(WIDE);
    let href = |k: u64| {
        d.tiles
            .iter()
            .find(|t| t.key == k)
            .map(|t| t.href.clone())
            .unwrap_or_default()
    };
    let aside = |h: &str| matches!(Route::parse(h), Route::Aside { .. });
    d.dividers
        .iter()
        .map(|l| (href(l.before), href(l.after), l.stops[l.now].shares))
        .filter(|(b, a, _)| !aside(b) && !aside(a))
        .collect()
}

/// What a person sees of their own: each workspace's pages, the focused workspace, the focus
/// among its pages, the address and the dividers' shares.
type Seen = (
    Vec<Vec<Route>>,
    usize,
    Option<usize>,
    String,
    Vec<(String, String, [u32; 2])>,
);

fn seen(desk: &Desk) -> Seen {
    let t = desk.tiles();
    let r = released(t);
    let pages = r
        .root()
        .parts()
        .map(|p| {
            p.tree
                .leaves()
                .into_iter()
                .filter(|l| l.page().pin() == Pin::Free)
                .map(|l| l.page().clone())
                .collect()
        })
        .collect();
    let address = format!("#{}", tiles::print(&tiles::act(r.clone(), Act::Release)));
    (pages, r.root().index(), focus(t), address, dividers(desk))
}

fn key(l: &Leaf<Route>) -> u32 {
    u32::try_from(l.id().key()).expect("a run's keys are small")
}

fn keys_of(desk: &Desk, before: &str, after: &str) -> Option<(u32, u32)> {
    let d = desk.drawing(WIDE);
    let k = |h: &str| d.tiles.iter().find(|t| t.href == h).map(|t| t.key);
    let line = d
        .dividers
        .iter()
        .find(|l| Some(l.before) == k(before) && Some(l.after) == k(after))?;
    Some((
        u32::try_from(line.before).ok()?,
        u32::try_from(line.after).ok()?,
    ))
}

/// The free leaf at `i` among `leaves`, none where there is none.
fn at<'t>(leaves: &[&'t Leaf<Route>], i: Index) -> Option<&'t Leaf<Route>> {
    (!leaves.is_empty()).then(|| leaves[i.index(leaves.len())])
}

/// Every free leaf of every workspace, the workspaces in order.
fn every(t: &Tiles<Route>) -> Vec<&Leaf<Route>> {
    t.root()
        .parts()
        .flat_map(|p| p.tree.leaves())
        .filter(|l| l.page().pin() == Pin::Free)
        .collect()
}

/// The free leaf before the secretary in `said`, at its place among `bare`'s free leaves: what a
/// focus on the secretary is in the world without it, where that world holds none.
fn before_aside(said: &Tiles<Route>, bare: &Desk) -> Option<String> {
    // Where the world without the say holds a secretary of the person's own, the act is the same.
    if let Some(own) = pinned(bare.tiles()) {
        return Some(focused_on(bare.tiles(), own.id()));
    }
    let a = pinned(said)?.id();
    let on = tiles::act(said.clone(), Act::Focus(a));
    let b = free(bare.tiles());
    let to = b.get(focus(&on)?.min(b.len().checked_sub(1)?))?;
    Some(focused_on(bare.tiles(), to.id()))
}

/// The address a focused secretary drawn alone is hidden to, where it is.
fn hidden(desk: &Desk) -> Option<String> {
    match (
        desk.tiles().focused().page().pin(),
        desk.drawing(ROOMS[0]).pinned,
    ) {
        (Pin::Pinned, Pinned::Drawn { without, .. }) => Some(without),
        (Pin::Pinned | Pin::Free, Pinned::Drawn { .. } | Pinned::Behind { .. } | Pinned::None) => {
            None
        }
    }
}

/// What a step did to the shares: kept comparable, or set beside the secretary, which no tree
/// without it has: the desk keeps that row for every node of the same free kinds, named here.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Shares {
    Comparable,
    Reached(Vec<Kind>),
}

/// The kinds of the free children of the node whose child holding `before` stands right before
/// its child holding `after`: the desk's key for the row a resize there sets.
fn row_kinds(n: &Node<Leaf<Route>>, before: u64, after: u64) -> Option<Vec<Kind>> {
    let holds = |k: u64| {
        n.parts()
            .position(|p| p.tree.leaves().iter().any(|l| l.id().key() == k))
    };
    match (holds(before), holds(after)) {
        (Some(i), Some(j)) if j == i + 1 => Some(
            n.parts()
                .filter(|p| !p.holds_pinned())
                .map(|p| p.tree.focused().page().kind())
                .collect(),
        ),
        (Some(i), Some(j)) if i == j => n.parts().nth(i).and_then(|p| match &p.tree {
            Tree::Node(c) => row_kinds(c, before, after),
            Tree::Leaf(_) => None,
        }),
        _ => None,
    }
}

/// The leaves, by their routes, of each child of each node whose free children are of a row's
/// kinds: what a row of shares set beside the secretary reaches in that tree.
type Row = Vec<Vec<String>>;

fn rows(t: &Tiles<Route>, reached: &[Vec<Kind>]) -> Vec<Row> {
    fn walk(n: &Node<Leaf<Route>>, reached: &[Vec<Kind>], out: &mut Vec<Row>) {
        let kinds: Vec<Kind> = n
            .parts()
            .filter(|p| !p.holds_pinned())
            .map(|p| p.tree.focused().page().kind())
            .collect();
        if reached.contains(&kinds) {
            out.push(
                n.parts()
                    .map(|p| p.tree.leaves().iter().map(|l| l.page().href()).collect())
                    .collect(),
            );
        }
        for p in n.parts() {
            if let Tree::Node(c) = &p.tree {
                walk(c, reached, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(t.root(), reached, &mut out);
    out
}

/// A crumb of `said`'s drawing at the room at `w` taken, and the same page focused in `bare`.
fn crumb(said: &mut Desk, bare: &mut Desk, i: Index, w: Index) {
    let room = ROOMS[w.index(ROOMS.len())];
    let crumbs: Vec<_> = said
        .drawing(room)
        .rows
        .into_iter()
        .flat_map(|r| r.crumbs)
        .collect();
    if crumbs.is_empty() {
        return;
    }
    let c = &crumbs[i.index(crumbs.len())];
    let to_b = match Route::parse(&c.href) {
        Route::Aside { .. } => before_aside(said.tiles(), bare),
        page => {
            let named: Vec<_> = free(bare.tiles())
                .into_iter()
                .filter(|l| *l.page() == page)
                .collect();
            match named.as_slice() {
                [l] => Some(focused_on(bare.tiles(), l.id())),
                _ => None,
            }
        }
    };
    if let Some(to_b) = to_b {
        said.go(&c.to);
        bare.go(&to_b);
    }
}

/// The divider between two of the person's pages, the one at `i`, set in both worlds.
fn resize(said: &mut Desk, bare: &mut Desk, i: Index, weights: [u32; 2]) {
    let lines = dividers(bare);
    if lines.is_empty() {
        return;
    }
    let (before, after, _) = &lines[i.index(lines.len())];
    if let (Some((sb, sa)), Some((bb, ba))) =
        (keys_of(said, before, after), keys_of(bare, before, after))
    {
        let shares = weights.map(|w| share(w).expect("a weight of 1 to 3"));
        said.resize(sb, sa, shares);
        bare.resize(bb, ba, shares);
    }
}

/// The divider beside the secretary set, in the world that holds it alone.
fn resize_aside(said: &mut Desk, weights: [u32; 2]) -> Shares {
    let d = said.drawing(WIDE);
    let aside = |k: u64| {
        d.tiles
            .iter()
            .any(|t| t.key == k && matches!(Route::parse(&t.href), Route::Aside { .. }))
    };
    let Some(l) = d
        .dividers
        .iter()
        .find(|l| aside(l.before) || aside(l.after))
    else {
        return Shares::Comparable;
    };
    let (b4, af) = (
        u32::try_from(l.before).unwrap_or(0),
        u32::try_from(l.after).unwrap_or(0),
    );
    let Some(kinds) = row_kinds(said.tiles().root(), l.before, l.after) else {
        return Shares::Comparable;
    };
    said.resize(
        b4,
        af,
        weights.map(|w| share(w).expect("a weight of 1 to 3")),
    );
    Shares::Reached(kinds)
}

/// One step in both worlds: `said` with the say, `bare` with none.
fn both(
    said: &mut Desk,
    bare: &mut Desk,
    history: &mut Vec<(String, String)>,
    step: &Step,
) -> Shares {
    let mut shares = Shares::Comparable;
    match step {
        Step::Open(i, h) => {
            if let (Some(s), Some(b)) = (at(&free(said.tiles()), *i), at(&free(bare.tiles()), *i)) {
                let (ks, kb) = (key(s), key(b));
                said.open(ks, h);
                bare.open(kb, h);
            }
        }
        Step::OpenAny(i, h) => {
            if let (Some(s), Some(b)) = (at(&every(said.tiles()), *i), at(&every(bare.tiles()), *i))
            {
                let (ks, kb) = (key(s), key(b));
                said.open(ks, h);
                bare.open(kb, h);
            }
        }
        Step::OpenInAside(h) => {
            let Some(a) = pinned(said.tiles()).map(key) else {
                return shares;
            };
            let on = tiles::act(
                said.tiles().clone(),
                Act::Focus(
                    pinned(said.tiles()).map_or_else(|| said.tiles().focused().id(), Leaf::id),
                ),
            );
            let b = free(bare.tiles());
            if let Some(from) = focus(&on).and_then(|k| b.get(k.min(b.len().saturating_sub(1)))) {
                let kb = key(from);
                said.open(a, h);
                bare.open(kb, h);
            }
        }
        Step::Focus(i) => {
            if let (Some(s), Some(b)) = (at(&free(said.tiles()), *i), at(&free(bare.tiles()), *i)) {
                let (to_s, to_b) = (
                    focused_on(said.tiles(), s.id()),
                    focused_on(bare.tiles(), b.id()),
                );
                said.go(&to_s);
                bare.go(&to_b);
            }
        }
        Step::FocusAside => {
            if let (Some(a), Some(to_b)) = (
                pinned(said.tiles()).map(Leaf::id),
                before_aside(said.tiles(), bare),
            ) {
                said.go(&focused_on(said.tiles(), a));
                bare.go(&to_b);
            }
        }
        Step::Show => {
            if let (Pinned::Behind { to }, Some(to_b)) = (
                said.drawing(ROOMS[0]).pinned,
                before_aside(said.tiles(), bare),
            ) {
                said.go(&to);
                bare.go(&to_b);
            }
        }
        Step::Hide => {
            for desk in [&mut *said, &mut *bare] {
                if let Some(without) = hidden(desk) {
                    desk.go(&without);
                }
            }
        }
        Step::Crumb(i, w) => crumb(said, bare, *i, *w),
        Step::Visit(i) => {
            if let (Some(s), Some(b)) = (at(&every(said.tiles()), *i), at(&every(bare.tiles()), *i))
            {
                let (to_s, to_b) = (
                    focused_on(said.tiles(), s.id()),
                    focused_on(bare.tiles(), b.id()),
                );
                said.go(&to_s);
                bare.go(&to_b);
            }
        }
        Step::Back(i) => {
            let (s, b) = history[i.index(history.len())].clone();
            said.go(&s);
            bare.go(&b);
        }
        Step::Typed(s) => {
            said.go(s);
            bare.go(s);
        }
        Step::Resize(i, a, b) => resize(said, bare, *i, [*a, *b]),
        Step::ResizeAside(a, b) => shares = resize_aside(said, [*a, *b]),
        Step::Say(s) => tell(said, s),
    }
    history.push((said.address(), bare.address()));
    shares
}

/// Erasure: over any acts of a person interleaved with any says, what the person sees of their own
/// pages is what they see with the secretary never said; under `Held`, with the focus on a page of
/// theirs, the secretary said is the focused workspace's end; a say leaves a focused secretary,
/// and its page, where they stand; a desk that reads the address cold, told the same, holds the
/// same tree. Crumbs, Show and Hide are the host's acts on the addresses the drawing names. The
/// law reaches a held secretary in at least a fifth of its steps.
#[test]
fn the_secretary_is_erased_by_its_absence() {
    let config = ProptestConfig {
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
        )),
        ..ProptestConfig::default()
    };
    let total = config.cases;
    let mut runner = proptest::test_runner::TestRunner::new(config);
    let (held_steps, all_steps) = (std::cell::Cell::new(0u32), std::cell::Cell::new(0u32));
    let result = runner.run(
        &(typed(), prop::collection::vec(step(), 1..14)),
        |(first, steps)| {
            let mut said = Desk::new().expect("the destinations are a home");
            let mut bare = Desk::new().expect("the destinations are a home");
            let mut say = Say::Absent;
            // The rows of shares a resize beside the secretary set, by their free kinds.
            let mut reached: Vec<Vec<Kind>> = Vec::new();
            said.go(&first);
            bare.go(&first);
            let mut history = vec![(said.address(), bare.address())];
            for s in &steps {
                let was = said.tiles().focused().clone();
                if let Shares::Reached(kinds) = both(&mut said, &mut bare, &mut history, s) {
                    reached.push(kinds);
                }
                if let Step::Say(now) = s {
                    say = now.clone();
                    if was.page().pin() == Pin::Pinned {
                        prop_assert_eq!(
                            said.tiles().focused(),
                            &was,
                            "a say moved a focused secretary"
                        );
                    }
                }
                all_steps.set(all_steps.get() + 1);
                if let (Say::Held(about), Pin::Free) = (&say, said.tiles().focused().page().pin()) {
                    held_steps.set(held_steps.get() + 1);
                    prop_assert_eq!(
                        pinned(said.tiles()).map(Leaf::page),
                        Some(&Route::Aside {
                            about: about.clone()
                        }),
                        "the say's secretary stands at the end"
                    );
                }
                let (mut a, mut b) = (seen(&said), seen(&bare));
                for (seen, desk) in [(&mut a, &said), (&mut b, &bare)] {
                    let nodes = rows(desk.tiles(), &reached);
                    let set = |d: &(String, String, [u32; 2])| {
                        nodes.iter().any(|n| {
                            n.windows(2)
                                .any(|w| w[0].contains(&d.0) && w[1].contains(&d.1))
                        })
                    };
                    seen.4.retain(|d| !set(d));
                }
                prop_assert_eq!(a, b, "after {:?}", s);
                prop_assert!(said.tiles().check().is_ok());
                let mut cold = Desk::new().expect("the destinations are a home");
                tell(&mut cold, &say);
                cold.go(&said.address());
                prop_assert_eq!(
                    tiles::print(cold.tiles()),
                    tiles::print(said.tiles()),
                    "a cold read of {}",
                    said.address()
                );
            }
            Ok(())
        },
    );
    result.expect("the secretary is erased by its absence");
    let (held, all) = (held_steps.get(), all_steps.get());
    eprintln!("erasure: {held} of {all} steps with a held secretary, over {total} runs");
    assert!(held * 5 >= all, "{held} of {all}");
}

/// A bare secretary address stands beside the page it is about, in that page's workspace, and its
/// Hide lands on that page; the whole conversation's lands on its workspace's home.
#[test]
fn a_bare_secretary_hides_to_the_page_it_is_about() {
    for (bare, workspace, lands) in [
        ("#/aside/todo%3Alife%2Fa", "#/", "#/todo/a"),
        ("#/aside/mail%3Aabcdef12", "#/mail", "#/mail/abcdef12"),
        ("#/aside", "#/", "#/"),
    ] {
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(bare);
        assert_eq!(desk.workspace(), workspace, "{bare}: its workspace");
        assert_eq!(desk.address(), bare, "{bare}: read and printed back");
        let without = hidden(&desk).unwrap_or_else(|| panic!("{bare}: its Hide"));
        desk.go(&without);
        assert_eq!(desk.address(), lands, "{bare}: where its Hide lands");
    }
}
