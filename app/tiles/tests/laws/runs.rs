//! Generated well-formed homes, runs of acts, measures (zeros and `u32::MAX` among them), and
//! viewports from 0 px to a 2,560 px desktop.

use std::num::NonZeroU8;

use proptest::prelude::*;
use tiles::{
    Act, Axis, Beside, Dir, Fit, Leaf, Measures, Node, Part, Policy, Px, RootRow, Share, Tiles,
    Tree, Viewport, act, follow,
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

/// What a generated run draws: the generator the laws had before stage 2 (weights one to three, no
/// put), or the wider one (any weight, and the put).
#[derive(Debug, Clone, Copy)]
pub enum Mix {
    Master,
    Wide,
}

/// A weight of one to three, and with `Mix::Wide` any weight sometimes.
pub fn share(mix: Mix) -> BoxedStrategy<Share> {
    let weight = match mix {
        Mix::Master => (1u8..=3).boxed(),
        Mix::Wide => prop_oneof![6 => 1u8..=3, 3 => 4u8..=12, 1 => 13u8..=255].boxed(),
    };
    weight
        .prop_map(|w| Share::of(w).expect("a range from one"))
        .boxed()
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
fn workspace(budget: usize, mix: Mix) -> BoxedStrategy<Tree<Pg>> {
    let leaf = free().prop_map(Tree::Leaf);
    if budget < 2 {
        return leaf.boxed();
    }
    let split = (2..=budget).prop_flat_map(move |k| {
        let parts: Vec<_> = (0..k)
            .map(|i| budget / k + usize::from(i < budget % k))
            .map(|b| (share(mix), workspace(b, mix)).prop_map(|(s, t)| Part::new(s, t)))
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

/// The default policy mostly; a bound of one to eight and either rule beside it otherwise.
pub fn policy() -> impl Strategy<Value = Policy> {
    let beside = prop_oneof![Just(Beside::Replace), Just(Beside::Insert)];
    let most = (1u8..=8).prop_map(|n| NonZeroU8::new(n).expect("one to eight"));
    let steps = prop_oneof![
        3 => Just(Share::THREE),
        1 => (1u8..=6).prop_map(|w| Share::of(w).expect("one to six")),
    ];
    prop_oneof![
        1 => Just(Policy::default()),
        2 => (most, beside, steps).prop_map(|(most, beside, steps)| {
            Policy::default().with_most(most).with_beside(beside).with_steps(steps)
        }),
    ]
}

/// A home and the policy its tiles grow by.
pub type Home = (Node<Pg>, Policy);

/// A root of one to six workspaces of at most the policy's `most` tiles, a pinned end in some,
/// mostly a `Never` node across as an application's would be.
pub fn home() -> impl Strategy<Value = Home> {
    home_of(Mix::Wide)
}

pub fn home_of(mix: Mix) -> impl Strategy<Value = Home> {
    policy().prop_flat_map(move |policy| {
        let ws = (
            workspace(usize::from(policy.most().get()), mix),
            proptest::option::weighted(0.3, 0u8..4),
        )
            .prop_map(|(t, pin)| match pin {
                Some(n) => pinned_end(t, n),
                None => t,
            });
        let root_fit = prop_oneof![4 => Just(Fit::Never), 1 => Just(Fit::Fits)];
        node(
            prop::collection::vec((share(mix), ws).prop_map(|(s, t)| Part::new(s, t)), 1..7),
            root_fit,
        )
        .prop_map(move |n| (n, policy))
    })
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
        1 => (one(24), one(44), one(72), one(320), root_row()).prop_map(|(divider, row, crumb, tall, root_row)| {
            Measures { divider, row, crumb, tall, root_row }
        }),
    ]
}

fn root_row() -> impl Strategy<Value = RootRow> {
    prop_oneof![
        1 => Just(RootRow::Top),
        4 => prop_oneof![8 => Just(Px(992)), 1 => Just(Px(0)), 1 => Just(Px(u32::MAX))].prop_map(RootRow::FootUnder),
    ]
}

/// An act, with `Focus` naming a leaf by its place among the tree's leaves.
#[derive(Debug, Clone)]
pub enum Step {
    Open(Pg),
    Follow(Pg),
    Below(Pg),
    Close,
    Focus(prop::sample::Index),
    Move(Dir),
    /// The leaf at the first index put beside the leaf at the second.
    Put(prop::sample::Index, prop::sample::Index, Dir),
    /// The leaf at the index and the one after it in the tree's order: often the two either side
    /// of a divider, and otherwise a pair a resize leaves alone.
    Resize(prop::sample::Index, [Share; 2]),
    Hold(Pg),
    Release,
}

pub fn step_of(mix: Mix) -> BoxedStrategy<Step> {
    let dir = prop_oneof![
        Just(Dir::Left),
        Just(Dir::Right),
        Just(Dir::Up),
        Just(Dir::Down)
    ];
    let put = match mix {
        Mix::Master => 0,
        Mix::Wide => 2,
    };
    prop_oneof![
        2 => page().prop_map(Step::Open),
        2 => page().prop_map(Step::Follow),
        1 => page().prop_map(Step::Below),
        2 => Just(Step::Close),
        2 => any::<prop::sample::Index>().prop_map(Step::Focus),
        2 => dir.clone().prop_map(Step::Move),
        put => (any::<prop::sample::Index>(), any::<prop::sample::Index>(), dir).prop_map(|(i, j, d)| Step::Put(i, j, d)),
        2 => (any::<prop::sample::Index>(), share(mix), share(mix)).prop_map(|(i, a, b)| Step::Resize(i, [a, b])),
        1 => page().prop_map(Step::Hold),
        1 => Just(Step::Release),
    ].boxed()
}

pub fn ids(tiles: &Tiles<Pg>) -> Vec<tiles::Id> {
    tiles
        .root()
        .parts()
        .flat_map(|p| p.tree.leaves())
        .map(Leaf::id)
        .collect()
}

/// What a run applies: an act, or a page followed, which `follow` opens with its chain.
#[derive(Debug, Clone)]
pub enum Made {
    Act(Act<Pg>),
    Follow(Pg),
}

pub fn made(tiles: &Tiles<Pg>, step: &Step) -> Made {
    match step {
        Step::Follow(p) => Made::Follow(p.clone()),
        Step::Open(_)
        | Step::Below(_)
        | Step::Close
        | Step::Focus(_)
        | Step::Move(_)
        | Step::Put(..)
        | Step::Resize(..)
        | Step::Hold(_)
        | Step::Release => Made::Act(the_act(tiles, step)),
    }
}

fn the_act(tiles: &Tiles<Pg>, step: &Step) -> Act<Pg> {
    match step {
        Step::Open(p) | Step::Follow(p) => Act::Open(p.clone()),
        Step::Below(p) => Act::Below(p.clone()),
        Step::Close => Act::Close,
        Step::Focus(i) => {
            let ids = ids(tiles);
            Act::Focus(ids[i.index(ids.len())])
        }
        Step::Move(d) => Act::Move(*d),
        Step::Put(i, j, edge) => {
            let ids = ids(tiles);
            Act::Put {
                leaf: ids[i.index(ids.len())],
                beside: ids[j.index(ids.len())],
                edge: *edge,
            }
        }
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

pub fn apply(t: Tiles<Pg>, m: Made) -> Tiles<Pg> {
    match m {
        Made::Act(a) => act(t, a),
        Made::Follow(p) => follow(t, p),
    }
}

/// A run: a home, the measures, then acts, each with the viewport the person saw after it.
pub type Run = (Home, Measures, Viewport, Vec<(Step, Viewport)>);

pub fn run() -> impl Strategy<Value = Run> {
    run_of(Mix::Wide)
}

pub fn run_of(mix: Mix) -> impl Strategy<Value = Run> {
    (
        home_of(mix),
        measures(),
        viewport(),
        prop::collection::vec((step_of(mix), viewport()), 0..14),
    )
}

/// A state before an act, the act, the state after it, and the viewport it was seen on.
pub type State = (Tiles<Pg>, Option<Made>, Tiles<Pg>, Viewport);

/// Every state of the run with the viewport it was seen on, and the act that led to it.
pub fn states((home, policy): Home, first: Viewport, steps: &[(Step, Viewport)]) -> Vec<State> {
    let start = Tiles::new(home, policy).expect("the generated home is well formed");
    let mut out = vec![(start.clone(), None, start.clone(), first)];
    let mut now = start;
    for (s, at) in steps {
        let a = made(&now, s);
        let next = apply(now.clone(), a.clone());
        out.push((now, Some(a), next.clone(), *at));
        now = next;
    }
    out
}
