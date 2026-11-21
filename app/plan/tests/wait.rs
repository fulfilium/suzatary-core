//! The wait rule's laws, over generated stores whose geneses share prefixes: a name by genesis
//! moves the one store the rule picks, or says why it moves none; a chain's name moves that chain
//! alone; and no followed store changes which of this deployment's own chains a name moves.

use prodrome_core::event::{Hash, TodoId};
use proptest::prelude::*;
use suzatary_base::chains::ChainName;
use suzatary_base::reference::{GenesisPrefix, GlobalTodo, Reference, TodoRef};
use suzatary_plan::wait::{Audience, Held, Kept, Missing, Wait, blocker};

/// Four geneses that share the prefix of eight `a`, each told apart by where its `b` stands,
/// and one that shares nothing.
fn genesis(index: usize) -> Hash {
    let text = if index < 4 {
        format!("{}b{}", "a".repeat(8 + index), "a".repeat(55 - index))
    } else {
        "c".repeat(64)
    };
    Hash::new(text).expect("hex")
}

fn name(index: usize) -> ChainName {
    ChainName::new(format!("s{index}")).expect("a slug")
}

fn id() -> TodoId {
    TodoId::new("x").expect("an id")
}

fn on(index: usize) -> Wait {
    Wait::On(TodoRef {
        chain: name(index),
        id: id(),
    })
}

fn kept() -> impl Strategy<Value = Kept> {
    prop_oneof![
        Just(Kept::Private),
        Just(Kept::Public),
        Just(Kept::Followed)
    ]
}

fn audience() -> impl Strategy<Value = Audience> {
    prop_oneof![Just(Audience::Deployment), Just(Audience::Anyone)]
}

/// Each store's genesis (an index into [`genesis`]) and whose it is.
fn stores() -> impl Strategy<Value = Vec<(usize, Kept)>> {
    proptest::collection::vec((0usize..5, kept()), 0..6)
}

fn held(stores: &[(usize, Kept)]) -> Held {
    stores
        .iter()
        .enumerate()
        .fold(Held::default(), |held, (i, (g, kept))| {
            held.with(name(i), genesis(*g), *kept)
        })
}

fn by_genesis(target: usize, len: usize) -> Reference {
    Reference::Global(GlobalTodo {
        genesis: GenesisPrefix::new(&genesis(target).as_str()[..len]).expect("a prefix"),
        id: id(),
    })
}

/// The rule written out apart from the code.
fn expected(stores: &[(usize, Kept)], written_in: Audience, prefix: &str) -> Wait {
    let own = |g: usize| stores.iter().any(|(h, k)| *h == g && *k != Kept::Followed);
    let mut started: Vec<usize> = stores
        .iter()
        .map(|(g, _)| *g)
        .filter(|g| genesis(*g).as_str().starts_with(prefix))
        .collect();
    started.sort_by_key(|g| genesis(*g));
    started.dedup();
    let owned: Vec<usize> = started.iter().copied().filter(|g| own(*g)).collect();
    let one = match (started.as_slice(), owned.as_slice()) {
        ([], _) => return Wait::Missing(Missing::Unheld),
        ([one], _) | (_, [one]) => *one,
        (several, _) => {
            return Wait::Missing(Missing::Ambiguous {
                geneses: several
                    .iter()
                    .map(|g| genesis(*g).as_str().to_owned())
                    .collect(),
            });
        }
    };
    let readable: Vec<(usize, Kept)> = stores
        .iter()
        .enumerate()
        .filter(|(_, (g, k))| {
            *g == one && (written_in == Audience::Deployment || *k != Kept::Private)
        })
        .map(|(i, (_, k))| (i, *k))
        .collect();
    let owned: Vec<usize> = readable
        .iter()
        .filter(|(_, k)| *k != Kept::Followed)
        .map(|(i, _)| *i)
        .collect();
    match (owned.as_slice(), readable.as_slice()) {
        (_, []) => Wait::Missing(Missing::Private),
        ([i], _) | (_, [(i, _)]) => on(*i),
        (_, several) => Wait::Missing(Missing::Replicas {
            stores: several.iter().map(|(i, _)| name(*i)).collect(),
        }),
    }
}

proptest! {
    /// A name by genesis: no held genesis starts the prefix, unheld; several do and not one alone
    /// is begun by an own chain, ambiguous with exactly those; else, among the stores that begin
    /// it that the writer's words may move (any where the deployment wrote, else only stores
    /// anyone reads): none, private; one own chain alone, or one store alone, it; else replicas.
    /// A chain's name is a label where anyone reads the writer's store, and otherwise moves that
    /// chain when it is held.
    #[test]
    fn a_wait_names_the_one_store_the_rule_picks_or_says_why_not(
        stores in stores(),
        written_in in audience(),
        target in 0usize..5,
        len in 8usize..=64,
        local in 0usize..8,
    ) {
        let held = held(&stores);
        let whole = genesis(target);
        let prefix = &whole.as_str()[..len];
        prop_assert_eq!(
            blocker(written_in, &by_genesis(target, len), &held),
            expected(&stores, written_in, prefix)
        );
        let chain = Reference::Todo(TodoRef { chain: name(local), id: id() });
        let expected = match written_in {
            Audience::Anyone => Wait::Label,
            Audience::Deployment if local < stores.len() => on(local),
            Audience::Deployment => Wait::Missing(Missing::Unheld),
        };
        prop_assert_eq!(blocker(written_in, &chain, &held), expected);
    }

    /// No followed store changes which of this deployment's own chains a name moves: where a name
    /// moves an own chain with no followed store held, it moves that chain beside any followed
    /// stores, whatever genesis they begin.
    #[test]
    fn no_followed_store_moves_a_name_off_an_own_chain(
        own in proptest::collection::vec(
            (0usize..5, prop_oneof![Just(Kept::Private), Just(Kept::Public)]),
            0..4,
        ),
        followed in proptest::collection::vec(0usize..5, 0..4),
        written_in in audience(),
        target in 0usize..5,
        len in 8usize..=64,
    ) {
        let name = by_genesis(target, len);
        let alone = blocker(written_in, &name, &held(&own));
        let beside: Vec<(usize, Kept)> = own
            .iter()
            .copied()
            .chain(followed.iter().map(|g| (*g, Kept::Followed)))
            .collect();
        if let Wait::On(_) = alone {
            prop_assert_eq!(blocker(written_in, &name, &held(&beside)), alone);
        }
    }
}
