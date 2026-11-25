//! Generated well-formed homes, runs of acts, measures (zeros and `u32::MAX` among them), and
//! viewports from 0 px to a 2,560 px desktop.

use proptest::prelude::*;
use viewtree::tiles::{
    Act, Axis, Dir, Fit, Leaf, MOST, Measures, Node, Part, Px, Share, Tiles, Tree, Viewport, act,
};

use super::page::{Kind, MEASURES, Pg, pg};

pub fn page() -> impl Strategy<Value = Pg> {
    (
        prop_oneof![
            Just(Kind::List),
            Just(Kind::Column),
            Just(Kind::Side),
            Just(Kind::Slide)
        ],
        0u8..4,
    )
        .prop_map(|(k, n)| pg(k, n))
}

fn free() -> impl Strategy<Value = Pg> {
    (
        prop_oneof![Just(Kind::List), Just(Kind::Column), Just(Kind::Slide)],
        0u8..4,
    )
        .prop_map(|(k, n)| pg(k, n))
}

fn share() -> impl Strategy<Value = Share> {
    prop_oneof![Just(Share::One), Just(Share::Two), Just(Share::Three)]
}

fn axis() -> impl Strategy<Value = Axis> {
    prop_oneof![Just(Axis::Across), Just(Axis::Down)]
}

fn fit() -> impl Strategy<Value = Fit> {
    prop_oneof![3 => Just(Fit::Fits), 1 => Just(Fit::Never)]
}

fn node(
    parts: impl Strategy<Value = Vec<Part<Pg>>>,
    fit: impl Strategy<Value = Fit>,
) -> impl Strategy<Value = Node<Pg>> {
    (axis(), fit, parts, any::<prop::sample::Index>()).prop_map(|(axis, fit, parts, at)| {
        let at = at.index(parts.len());
        let mut parts = parts.into_iter();
        let first = parts.next().expect("a node is generated with a child");
        Node::new(axis, fit, first, parts.collect(), at)
    })
}

/// A workspace of at most `budget` free leaves, nodes of two children or more, any depth that
/// allows, both axes, both fits.
fn workspace(budget: usize) -> BoxedStrategy<Tree<Pg>> {
    let leaf = free().prop_map(Tree::Leaf);
    if budget < 2 {
        return leaf.boxed();
    }
    let split = (2..=budget).prop_flat_map(move |k| {
        let parts: Vec<_> = (0..k)
            .map(|i| budget / k + usize::from(i < budget % k))
            .map(|b| (share(), workspace(b)).prop_map(|(s, t)| Part::new(s, t)))
            .collect();
        node(parts, fit()).prop_map(Tree::Node)
    });
    prop_oneof![1 => leaf, 2 => split].boxed()
}

/// The workspace with its end, the last leaf through the last children, made pinned.
fn pinned_end(tree: Tree<Pg>, n: u8) -> Tree<Pg> {
    match tree {
        Tree::Leaf(_) => Tree::Leaf(pg(Kind::Side, n)),
        Tree::Node(mut n_) => {
            let end = n_.after.last_mut().unwrap_or(&mut n_.focus);
            let tree = std::mem::replace(&mut end.tree, Tree::Leaf(pg(Kind::List, 0)));
            end.tree = pinned_end(tree, n);
            Tree::Node(n_)
        }
    }
}

/// A root of one to six workspaces of at most `MOST` tiles, a pinned end in some, mostly a
/// `Never` node across as an application's would be.
pub fn home() -> impl Strategy<Value = Node<Pg>> {
    let ws =
        (workspace(MOST), proptest::option::weighted(0.3, 0u8..4)).prop_map(|(t, pin)| match pin {
            Some(n) => pinned_end(t, n),
            None => t,
        });
    let root_fit = prop_oneof![4 => Just(Fit::Never), 1 => Just(Fit::Fits)];
    node(
        prop::collection::vec((share(), ws).prop_map(|(s, t)| Part::new(s, t)), 1..7),
        root_fit,
    )
}

pub fn viewport() -> impl Strategy<Value = Viewport> {
    let v = |w, h| Viewport { w: Px(w), h: Px(h) };
    prop_oneof![
        1 => (0u32..=320, 0u32..=600).prop_map(move |(w, h)| v(w, h)),
        3 => (320u32..=480, 560u32..=932).prop_map(move |(w, h)| v(w, h)),
        2 => (560u32..=932, 320u32..=480).prop_map(move |(w, h)| v(w, h)),
        2 => (744u32..=1024, 1024u32..=1366).prop_map(move |(w, h)| v(w, h)),
        2 => (1024u32..=1366, 744u32..=1024).prop_map(move |(w, h)| v(w, h)),
        3 => (1024u32..=2560, 600u32..=1600).prop_map(move |(w, h)| v(w, h)),
    ]
}

/// The application's measures mostly; any of them zero or `u32::MAX` sometimes.
pub fn measures() -> impl Strategy<Value = Measures> {
    let one =
        |usual: u32| prop_oneof![8 => Just(Px(usual)), 1 => Just(Px(0)), 1 => Just(Px(u32::MAX))];
    prop_oneof![
        3 => Just(MEASURES),
        1 => (one(24), one(44), one(72), one(320), one(992)).prop_map(|(divider, row, crumb, tall, foot)| {
            Measures { divider, row, crumb, tall, foot }
        }),
    ]
}

/// An act, with `Focus` naming a leaf by its place among the tree's leaves.
#[derive(Debug, Clone)]
pub enum Step {
    Open(Pg),
    Below(Pg),
    Close,
    Focus(prop::sample::Index),
    Move(Dir),
    /// The leaf at the index and the one after it in the tree's order: often the two either side
    /// of a divider, and otherwise a pair a resize leaves alone.
    Resize(prop::sample::Index, [Share; 2]),
    Hold(Pg),
    Release,
}

pub fn step() -> impl Strategy<Value = Step> {
    let dir = prop_oneof![
        Just(Dir::Left),
        Just(Dir::Right),
        Just(Dir::Up),
        Just(Dir::Down)
    ];
    prop_oneof![
        4 => page().prop_map(Step::Open),
        1 => page().prop_map(Step::Below),
        2 => Just(Step::Close),
        2 => any::<prop::sample::Index>().prop_map(Step::Focus),
        2 => dir.prop_map(Step::Move),
        2 => (any::<prop::sample::Index>(), share(), share()).prop_map(|(i, a, b)| Step::Resize(i, [a, b])),
        1 => page().prop_map(Step::Hold),
        1 => Just(Step::Release),
    ]
}

pub fn ids(tiles: &Tiles<Pg>) -> Vec<viewtree::tiles::Id> {
    tiles
        .root()
        .parts()
        .flat_map(|p| p.tree.leaves())
        .map(Leaf::id)
        .collect()
}

pub fn the_act(tiles: &Tiles<Pg>, step: &Step) -> Act<Pg> {
    match step {
        Step::Open(p) => Act::Open(p.clone()),
        Step::Below(p) => Act::Below(p.clone()),
        Step::Close => Act::Close,
        Step::Focus(i) => {
            let ids = ids(tiles);
            Act::Focus(ids[i.index(ids.len())])
        }
        Step::Move(d) => Act::Move(*d),
        Step::Resize(i, shares) => {
            let ids = ids(tiles);
            let k = i.index(ids.len());
            Act::Resize {
                before: ids[k],
                after: ids[(k + 1) % ids.len()],
                shares: *shares,
            }
        }
        Step::Hold(p) => Act::Hold(p.clone()),
        Step::Release => Act::Release,
    }
}

/// A run: a home, the measures, then acts, each with the viewport the person saw after it.
pub type Run = (Node<Pg>, Measures, Viewport, Vec<(Step, Viewport)>);

pub fn run() -> impl Strategy<Value = Run> {
    (
        home(),
        measures(),
        viewport(),
        prop::collection::vec((step(), viewport()), 0..14),
    )
}

/// A state before an act, the act, the state after it, and the viewport it was seen on.
pub type State = (Tiles<Pg>, Option<Act<Pg>>, Tiles<Pg>, Viewport);

/// Every state of the run with the viewport it was seen on, and the act that led to it.
pub fn states(home: Node<Pg>, first: Viewport, steps: &[(Step, Viewport)]) -> Vec<State> {
    let start = Tiles::new(home).expect("the generated home is well formed");
    let mut out = vec![(start.clone(), None, start.clone(), first)];
    let mut now = start;
    for (s, at) in steps {
        let a = the_act(&now, s);
        let next = act(now.clone(), a.clone());
        out.push((now, Some(a), next.clone(), *at));
        now = next;
    }
    out
}
