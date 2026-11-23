//! The app's route as the layout's page: every hash a person has keeps its meaning. The hashes are
//! every route's print, drawn over every variant, and strings near them. `hashes.json` is the list
//! this parser and `web/src/shell/route.ts`'s both read to the same print (`web/test/laws.test.ts`).

use proptest::prelude::*;
use suzatary_pages::desk::{Desk, Scroller};
use suzatary_pages::route::Route;
use viewtree::tiles::{self, Page, Px, Viewport};

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
        | Route::Conversation { .. }
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

proptest! {
    /// `print` is a section of `parse`: a route parsed from any hash prints as a hash that parses
    /// back to it, so every link the app writes reads as the page it names.
    #[test]
    fn a_printed_route_parses_back(h in hash()) {
        let r = Route::parse(&h);
        prop_assert_eq!(Route::parse(&r.href()), r.clone());
        prop_assert_eq!(<Route as Page>::parse(&r.print()), Some(r));
    }

    /// The layout's address of a bare hash is that bare hash: its tree has the route focused,
    /// and prints as the route alone, so the address a person sees stays today's.
    #[test]
    fn a_bare_hash_is_its_own_address(h in hash()) {
        let r = Route::parse(&h);
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&h);
        prop_assert_eq!(desk.tiles().focused().page(), &r);
        prop_assert_eq!(tiles::print(desk.tiles()), r.print());
    }

    /// Today's layouts: a page drawn alone scrolls the document, and only a todo, beside the list
    /// it is opened from, makes two tiles, exactly where the room holds both least widths and a
    /// divider, at any height. The todo alone stands under the Todos workspace's crumb row; the
    /// two stand either side of one divider, named by the list.
    #[test]
    fn a_bare_hash_draws_todays_layout(h in hash(), at in viewport()) {
        let r = Route::parse(&h);
        let mut desk = Desk::new().expect("the destinations are a home");
        desk.go(&h);
        let d = desk.drawing(at);
        let todo = matches!(r, Route::Todo { .. });
        let hrefs: Vec<&str> = d.tiles.iter().map(|t| t.href.as_str()).collect();
        let href = r.href();
        if todo && at.w.0 >= BESIDE {
            prop_assert_eq!(hrefs, vec!["#/", href.as_str()]);
            prop_assert_eq!(d.scroll, Scroller::Tiles);
            prop_assert!(d.rows.is_empty());
            prop_assert_eq!(d.dividers.len(), 1);
            prop_assert_eq!(d.dividers[0].before, d.tiles[0].key);
            prop_assert_eq!(&d.dividers[0].node, &vec![0]);
            prop_assert!(d.tiles.iter().all(|t| t.rect.y == 0 && t.rect.h == at.h.0));
        } else {
            prop_assert_eq!(hrefs, vec![href.as_str()]);
            prop_assert_eq!(d.scroll, Scroller::Document);
            prop_assert!(d.dividers.is_empty());
            let rows: Vec<Vec<usize>> = d.rows.iter().map(|r| r.node.clone()).collect();
            prop_assert_eq!(rows, if todo { vec![vec![0]] } else { vec![] });
        }
    }
}
