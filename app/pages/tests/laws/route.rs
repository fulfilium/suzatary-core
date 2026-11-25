//! The app's route as the layout's page: every hash a person has keeps its meaning. The hashes are
//! every route's print, drawn over every variant, and strings near them. `hashes.json` is the list
//! this parser and `web/src/shell/route.ts`'s both read to the same print (`web/test/laws.test.ts`).

use proptest::prelude::*;
use suzatary_pages::desk::{Area, Desk, Drawing, Scroller, share};
use suzatary_pages::route::Route;
use tiles::{Address, Home, Policy, Px, Viewport};

/// The end of `r`'s `home` chain, followed at most the default policy's `most` steps.
fn end(r: &Route) -> Route {
    let mut at = r.clone();
    for _ in 0..Policy::default().most().get() {
        let home = at.home();
        if home == at {
            break;
        }
        at = home;
    }
    at
}

/// The leaves the desk's tree holds.
fn leaves(desk: &Desk) -> usize {
    desk.tiles()
        .root()
        .parts()
        .map(|p| p.tree.leaves().len())
        .sum()
}

/// A word with bytes that need escaping, and some that are malformed escapes.
fn word() -> impl Strategy<Value = String> {
    let piece = "[a-z0-9]{0,3}|%2F|%E2%80%94|%ZZ|%|%\\+1|%-1|\\+|thread|memory|new|-|,|\\.|~|\\(|\\||!|'|\\*|/|\\?|&|=|—";
    prop::collection::vec(piece, 0..4).prop_map(|p| p.concat())
}

/// Every variant, by `variant`'s exhaustive match.
fn route() -> impl Strategy<Value = Route> {
    let w = word;
    let words = || prop::collection::vec(word(), 0..3);
    prop_oneof![
        Just(Route::Todos),
        (w(), w()).prop_map(|(text, title)| Route::Add { text, title }),
        w().prop_map(|id| Route::Todo { id }),
        w().prop_map(|open| Route::Typst { open }),
        prop::option::of(w()).prop_map(|open| Route::Inbox { open }),
        (w(), w()).prop_map(|(query, account)| Route::Mail { query, account }),
        w().prop_map(|id| Route::Message { id }),
        w().prop_map(|id| Route::Thread { id }),
        Just(Route::About),
        Just(Route::Settings),
        Just(Route::Briefings),
        w().prop_map(|date| Route::Briefing { date }),
        w().prop_map(|date| Route::Calendar { date }),
        (words(), words(), any::<bool>()).prop_map(|(refs, topics, focus)| Route::Secretary {
            refs,
            topics,
            focus
        }),
        Just(Route::Memory),
        Just(Route::NewTopic),
        Just(Route::Conversations),
        (
            w(),
            words(),
            any::<bool>(),
            prop::option::of(0..=(1u64 << 53) - 1)
        )
            .prop_map(|(id, refs, focus, at)| Route::Conversation {
                id,
                refs,
                focus,
                at
            }),
        w().prop_map(|about| Route::Aside { about }),
        w().prop_map(|hash| Route::Unknown { hash }),
    ]
}

/// A variant added to `Route` stops this match from compiling: the reminder to draw it in `route`.
#[test]
fn every_variant_is_drawn() {
    let variant = |r: &Route| match r {
        Route::Todos
        | Route::Add { .. }
        | Route::Todo { .. }
        | Route::Typst { .. }
        | Route::Inbox { .. }
        | Route::Mail { .. }
        | Route::Message { .. }
        | Route::Thread { .. }
        | Route::About
        | Route::Settings
        | Route::Briefings
        | Route::Briefing { .. }
        | Route::Calendar { .. }
        | Route::Secretary { .. }
        | Route::Memory
        | Route::NewTopic
        | Route::Conversations
        | Route::Conversation { .. }
        | Route::Aside { .. }
        | Route::Unknown { .. } => (),
    };
    Route::destinations().iter().for_each(variant);
}

/// A route's print, the same print cut or extended, or a string of its own.
fn hash() -> impl Strategy<Value = String> {
    prop_oneof![
        route().prop_map(|r| r.href()),
        (route(), word()).prop_map(|(r, w)| format!("{}{w}", r.href())),
        (route(), 0usize..12).prop_map(|(r, n)| r.href().chars().take(n).collect()),
        word(),
    ]
}

/// Any room, down to nothing tall: height takes no tile away.
fn viewport() -> impl Strategy<Value = Viewport> {
    (200u32..2400, 0u32..1600).prop_map(|(w, h)| Viewport { w: Px(w), h: Px(h) })
}

/// A list's least width, a divider and a column's (`tokens.typ`): the room two tiles need.
const BESIDE: u32 = 320 + 24 + 480;
/// A list's least width, a divider and the secretary's.
const ASIDE: u32 = 320 + 24 + 352;

#[derive(serde::Deserialize)]
struct Pair {
    hash: String,
    href: String,
}

fn shared() -> Vec<Pair> {
    serde_json::from_str(include_str!("../hashes.json")).expect("hashes.json is a list of pairs")
}

/// Each hash of the shared list prints as its `href` (and TypeScript's parser reads the same list).
#[test]
fn the_shared_hashes_print_as_listed() {
    let wrong: Vec<(String, String, String)> = shared()
        .into_iter()
        .filter_map(|p| {
            let got = Route::parse(&p.hash).href();
            (got != p.href).then_some((p.hash, p.href, got))
        })
        .collect();
    assert_eq!(wrong, Vec::new(), "(hash, listed, printed)");
}

/// The generated half of `hashes.json`: `cargo test -p suzatary-pages --test laws -- --ignored
/// --nocapture draw_shared_hashes` prints it from a fixed seed. A string that escapes a byte past
/// ASCII or a letter is left to the laws above: the published tree refuses a file of many, and the
/// hand-written cases take such a decode through both parsers.
#[test]
#[ignore = "prints the generated half of hashes.json"]
fn draw_shared_hashes() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &[7; 32]);
    let mut runner = TestRunner::new_with_rng(Config::default(), rng);
    let pairs: Vec<serde_json::Value> = (0..200)
        .map(|_| {
            let h = hash().new_tree(&mut runner).expect("a hash").current();
            (Route::parse(&h).href(), h)
        })
        .filter(|(href, h)| !spells(h) && !spells(href))
        .map(|(href, h)| serde_json::json!({ "href": href, "hash": h }))
        .collect();
    println!("{}", serde_json::to_string_pretty(&pairs).expect("JSON"));
}

/// Whether `s` spells a byte past ASCII, or a letter or digit, as a `%` escape.
fn spells(s: &str) -> bool {
    s.as_bytes().windows(3).any(|w| {
        let digit = |c: &u8| char::from(*c).to_digit(16);
        w[0] == b'%'
            && matches!((digit(&w[1]), digit(&w[2])), (Some(hi), Some(lo))
                if hi * 16 + lo >= 0x80 || char::from_u32(hi * 16 + lo).is_some_and(|c| c.is_ascii_alphanumeric()))
    })
}

/// A hash with a `~` before `|` or `-` is read as an address with a tree, damaged or not, and
/// stands for its head when the tree does not read: no route this app prints holds one but a
/// message id that spells `~|` or `~-`, which a hash cannot tell from a tree cut short.
fn bare(h: &str) -> bool {
    !(h.contains("~|") || h.contains("~-"))
}

proptest! {
    /// `print` is a section of `parse`: a route parsed from any hash prints as a hash that parses
    /// back to it, so every link the app writes reads as the page it names.
    #[test]
    fn a_printed_route_parses_back(h in hash()) {
        let r = Route::parse(&h);
        prop_assert_eq!(Route::parse(&r.href()), r.clone());
        prop_assert_eq!(<Route as Address>::parse(&r.print()), Some(r));
    }

    /// The layout's address of a bare hash is that bare hash: its tree has the route focused,
    /// and prints as the route alone, so every hash a person has is its own address.
    #[test]
    fn a_bare_hash_is_its_own_address(h in hash().prop_filter("bare", |h| bare(h))) {
        let r = Route::parse(&h);
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&h);
        prop_assert_eq!(desk.tiles().focused().page(), &r);
        prop_assert_eq!(tiles::print(desk.tiles()), r.print());
    }

    /// A page opened from its workspace's home, the end of its `home` chain, the way a person
    /// reaches it (a conversation from the stream, a todo from the list), stands where its bare
    /// hash puts it, and the address is that bare hash.
    #[test]
    fn a_page_opened_from_its_workspaces_home_is_its_bare_hash(h in hash().prop_filter("bare", |h| bare(h)), at in viewport()) {
        let r = Route::parse(&h);
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&end(&r).href());
        let home = desk.drawing(at).tiles[0].key;
        desk.open(u32::try_from(home).expect("a small key"), &r.href());
        let mut cold = Desk::new().expect("the destinations are a home");
        cold.go(&r.href());
        prop_assert_eq!(desk.address(), r.href());
        let pages = |d: &Desk| d.tiles().root().focus.tree.leaves().into_iter().map(|l| l.page().clone()).collect::<Vec<_>>();
        prop_assert_eq!(pages(&desk), pages(&cold));
    }

    /// Every route's `home` chain ends at one of the bar's destinations within the default policy's `most` steps.
    #[test]
    fn every_chain_ends_at_a_destination(h in hash()) {
        let r = Route::parse(&h);
        prop_assert!(Route::destinations().contains(&end(&r)), "{:?} ends at {:?}", r, end(&r));
    }

    /// A bare hash's layout: a page drawn alone scrolls the document, and only a todo or the
    /// secretary beside the list it is opened from, and a conversation beside the conversations,
    /// make two tiles, exactly where the room holds both least widths and a divider, at any height. Alone, such a
    /// page stands under its workspace's crumb row; the two stand either side of one divider,
    /// named by the list.
    #[test]
    fn a_bare_hash_draws_its_home_and_its_page(h in hash().prop_filter("bare", |h| bare(h)), at in viewport()) {
        let r = Route::parse(&h);
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&h);
        let d = desk.drawing(at);
        // Every leaf of the tree is drawn or behind, once.
        let mut keys: Vec<u64> = d.tiles.iter().map(|t| t.key).chain(d.behind.iter().map(|b| b.key)).collect();
        let leaves = desk.tiles().root().parts().map(|p| p.tree.leaves().len()).sum::<usize>();
        keys.sort_unstable();
        keys.dedup();
        prop_assert_eq!(keys.len(), leaves);
        prop_assert_eq!(d.tiles.len() + d.behind.len(), leaves);
        // The list a bare hash opens its page beside, and the workspace both stand in.
        let from = if matches!(r, Route::Todo { .. }) {
            Some(("#/", 0, BESIDE))
        } else if matches!(r, Route::Aside { .. }) {
            Some(("#/", 0, ASIDE))
        } else if matches!(r, Route::Conversation { .. } | Route::NewTopic | Route::Memory) {
            Some(("#/conversations", 2, BESIDE))
        } else {
            None
        };
        let hrefs: Vec<&str> = d.tiles.iter().map(|t| t.href.as_str()).collect();
        let href = r.href();
        match from {
            Some((list, ws, least)) if at.w.0 >= least => {
                prop_assert_eq!(hrefs, vec![list, href.as_str()]);
                prop_assert_eq!(d.scroll, Scroller::Tiles);
                prop_assert!(d.rows.is_empty());
                prop_assert_eq!(d.dividers.len(), 1);
                prop_assert_eq!(d.dividers[0].before, d.tiles[0].key);
                prop_assert_eq!(&d.dividers[0].node, &vec![ws]);
                prop_assert!(d.tiles.iter().all(|t| t.rect.y == 0 && t.rect.h == at.h.0));
            }
            Some(_) | None => {
                prop_assert_eq!(hrefs, vec![href.as_str()]);
                prop_assert_eq!(d.scroll, Scroller::Document);
                prop_assert!(d.dividers.is_empty());
                let rows: Vec<Vec<usize>> = d.rows.iter().map(|r| r.node.clone()).collect();
                prop_assert_eq!(rows, from.map(|(_, ws, _)| vec![ws]).into_iter().collect::<Vec<_>>());
            }
        }
    }

    /// Links followed in drawn tiles, through the desk: each address the desk prints, read by a
    /// desk that never saw it, prints the same address and draws the same pages in the same
    /// rectangles, at two rooms; and the address before an open draws again what was drawn before
    /// it, which is Back.
    #[test]
    fn an_open_reads_back_from_its_address(first in hash(), opens in prop::collection::vec((any::<prop::sample::Index>(), link()), 1..6), at in viewport(), other in viewport()) {
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&first);
        for (i, h) in opens {
            let key = pick(&desk, at, i);
            let (was, drawn) = (desk.address(), placed(&desk, at));
            desk.open(key, &h);
            let address = desk.address();
            let mut cold = Desk::new().expect("the destinations are a home");
            cold.go(&address);
            prop_assert_eq!(cold.address(), address.clone());
            for room in [at, other] {
                prop_assert_eq!(placed(&cold, room), placed(&desk, room));
                // Every leaf drawn or behind, once; and the host keeps pages by the addresses behind.
                let d = desk.drawing(room);
                prop_assert_eq!(d.tiles.len() + d.behind.len(), leaves(&desk));
                let behind = |d: &Drawing| d.behind.iter().map(|b| b.href.clone()).collect::<Vec<_>>();
                prop_assert_eq!(behind(&cold.drawing(room)), behind(&d));
            }
            let mut back = Desk::new().expect("the destinations are a home");
            back.go(&address);
            back.go(&was);
            prop_assert_eq!(placed(&back, at), drawn);
        }
    }

    /// A resize at a divider, over any opens, rooms and resizes before it: the divider then
    /// stands at the stop asked, with its shares; the same tiles stand; and the same resize again
    /// changes nothing.
    #[test]
    fn a_resize_holds_the_stop_asked(first in hash(), opens in prop::collection::vec((any::<prop::sample::Index>(), link()), 0..5), resizes in prop::collection::vec((viewport(), any::<prop::sample::Index>(), any::<prop::sample::Index>()), 1..8)) {
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&first);
        for (i, h) in opens {
            let at = Viewport { w: Px(1920), h: Px(900) };
            let key = pick(&desk, at, i);
            desk.open(key, &h);
        }
        for (at, d, s) in resizes {
            let was = desk.drawing(at);
            let Some(line) = (!was.dividers.is_empty()).then(|| was.dividers[d.index(was.dividers.len())].clone()) else {
                continue;
            };
            let stop = line.stops[s.index(line.stops.len())];
            let shares = stop.shares.map(|w| share(w).expect("a drawing's weights are shares"));
            let (b, a) = (small(line.before), small(line.after));
            desk.resize(b, a, shares);
            let now = desk.drawing(at);
            let held = now.dividers.iter().find(|l| (l.before, l.after) == (line.before, line.after));
            prop_assert_eq!(held.and_then(|l| l.stops.get(l.now)).copied(), Some(stop));
            prop_assert_eq!(keys(&now), keys(&was));
            desk.resize(b, a, shares);
            prop_assert_eq!(desk.drawing(at), now);
        }
    }
}

/// A page a link opens: any hash, or often one of the pages that stand beside one another.
fn link() -> impl Strategy<Value = String> {
    prop_oneof![
        1 => hash(),
        2 => prop::sample::select(vec!["#/todo/a", "#/todo/b", "#/mail/m", "#/mail/thread/t", "#/secretary/c", "#/briefings/2026-01-02", "#/inbox"]).prop_map(str::to_owned),
    ]
}

/// The key of a drawn tile, as the host hands it to the desk.
fn small(key: u64) -> u32 {
    u32::try_from(key).expect("a test's keys are small")
}

fn pick(desk: &Desk, at: Viewport, i: prop::sample::Index) -> u32 {
    let keys = keys(&desk.drawing(at));
    small(keys[i.index(keys.len())].0)
}

fn keys(d: &Drawing) -> Vec<(u64, String)> {
    d.tiles.iter().map(|t| (t.key, t.href.clone())).collect()
}

/// What a person sees of a drawing: each tile's page and rectangle, and the scroller.
fn placed(desk: &Desk, at: Viewport) -> (Scroller, Vec<(String, Area)>) {
    let d = desk.drawing(at);
    (
        d.scroll,
        d.tiles.into_iter().map(|t| (t.href, t.rect)).collect(),
    )
}
