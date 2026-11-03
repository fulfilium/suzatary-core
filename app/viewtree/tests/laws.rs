//! The painter's laws, over generated views and a DOM kept as data (`Dom`), which applies
//! patches the way `web/src/painter/painter.ts` does:
//!
//! 1. A patch applied to the painted DOM equals a fresh draw of the new view.
//! 2. Painting a view again patches nothing.
//! 3. The bytes read back as the patches they print.
//! 4. Reordering keyed rows makes and removes nothing: every row keeps its node.
//! 5. An event is answered by the handler the current view put on that node.

use proptest::prelude::*;
use viewtree::Attr::{Disabled, Value};
use viewtree::dom::{Dom, Shape, fresh};
use viewtree::node::{el, foreign, text};
use viewtree::patch::{self, ROOT};
use viewtree::{Node, On, Painter, Patch, Tag, view};

type Msg = u8;

const TAGS: [Tag; 4] = [Tag::Div, Tag::P, Tag::Section, Tag::Span];
const CLASSES: [&str; 3] = ["a", "b", "c"];

fn words(n: u8) -> String {
    ["", "one", "two", "three"][usize::from(n % 4)].to_owned()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "an input handler takes the field's words"
)]
fn input(s: String) -> Msg {
    u8::try_from(s.len()).unwrap_or(u8::MAX)
}

/// A generated view: elements of a few tags and classes, texts, foreign parts, and children by
/// position or by key, each key drawn from a small set so that rows are kept, moved and lost.
fn view() -> impl Strategy<Value = Node<Msg>> {
    let leaf = prop_oneof![
        (0u8..4).prop_map(|n| text(words(n))),
        (0u8..3, 0u8..3).prop_map(|(k, p)| foreign("card", format!("k{k}"), words(p))),
    ];
    leaf.prop_recursive(4, 40, 6, |inner| {
        (
            0usize..4,
            proptest::option::of(0usize..3),
            proptest::option::of(0u8..3),
            any::<bool>(),
            any::<bool>(),
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..5).prop_map(Kids::Plain),
                proptest::collection::vec((0u8..6, inner), 0..6).prop_map(Kids::Keyed),
            ],
        )
            .prop_map(|(tag, class, click, typed, disabled, kids)| {
                let e = el(TAGS[tag]).set(viewtree::Attr::Disabled, disabled);
                let e = match class {
                    Some(c) => e.set(viewtree::Attr::Class, CLASSES[c]),
                    None => e,
                };
                let e = match click {
                    Some(m) => e.click(m),
                    None => e,
                };
                let e = if typed { e.input(input) } else { e };
                match kids {
                    Kids::Plain(xs) => e.child(xs).node(),
                    Kids::Keyed(xs) => e
                        .keyed(xs.into_iter().map(|(k, x)| (format!("r{k}"), x)))
                        .node(),
                }
            })
    })
}

#[derive(Debug, Clone)]
enum Kids {
    Plain(Vec<Node<Msg>>),
    Keyed(Vec<(u8, Node<Msg>)>),
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Law 1, over a run of views: after each, the patched DOM is the fresh draw of that view.
    #[test]
    fn a_patch_applied_equals_a_fresh_draw(first in view(), views in proptest::collection::vec(view(), 0..5)) {
        let (mut painter, ps) = Painter::start(first);
        let mut dom = Dom::default();
        dom.apply(&ps);
        for v in views {
            dom.apply(&painter.paint(v.clone()));
            prop_assert_eq!(dom.page(), fresh(&v));
        }
    }

    /// Law 2: a view painted twice patches nothing the second time.
    #[test]
    fn painting_a_view_again_patches_nothing(a in view(), b in view()) {
        let (mut painter, _) = Painter::start(a);
        painter.paint(b.clone());
        prop_assert_eq!(painter.paint(b), Vec::<Patch>::new());
    }

    /// Law 3: the bytes are the patches.
    #[test]
    fn the_bytes_read_back_as_the_patches(a in view(), b in view()) {
        let (mut painter, first) = Painter::start(a);
        for ps in [first, painter.paint(b)] {
            prop_assert_eq!(patch::decode(&patch::encode(&ps)), Ok(ps));
        }
    }

    /// Law 5: whatever was painted before, a node's event is its current handler's message.
    #[test]
    fn an_event_is_answered_by_the_current_view(a in view(), b in view()) {
        let (mut painter, first) = Painter::start(a);
        let mut dom = Dom::default();
        dom.apply(&first);
        dom.apply(&painter.paint(b));
        let mut ids = Vec::new();
        dom.ids(ROOT, &mut ids);
        for id in ids {
            if let Shape::Element { on, .. } = &dom.nodes[&id] {
                prop_assert_eq!(painter.message(id, On::Click, String::new()).is_some(), on.contains(&On::Click.code()));
                prop_assert_eq!(painter.message(id, On::Input, "four".to_owned()).is_some(), on.contains(&On::Input.code()));
                if on.contains(&On::Input.code()) {
                    prop_assert_eq!(painter.message(id, On::Input, "four".to_owned()), Some(4));
                }
            }
        }
    }
}

fn row(k: usize) -> (String, Node<Msg>) {
    (format!("row{k}"), view! { P "row" [format!("row {k}")] })
}

/// Law 4: any permutation of keyed rows moves nodes and makes or removes none, and moves no
/// more rows than are out of their relative order.
#[test]
fn reordering_keyed_rows_keeps_every_node() {
    let order = [3usize, 0, 4, 1, 2, 5, 7, 6];
    let list = |ks: &[usize]| el::<Msg>(Tag::Div).keyed(ks.iter().map(|&k| row(k))).node();
    let (mut painter, _) = Painter::start(list(&[0, 1, 2, 3, 4, 5, 6, 7]));
    let ps = painter.paint(list(&order));
    assert!(ps.iter().all(|p| matches!(p, Patch::Move { .. })), "{ps:?}");
    // 0 1 2 5 6 (or 7) stay in order: three rows move.
    assert_eq!(ps.len(), 3, "{ps:?}");
}

/// One keystroke in a field of a page of 200 rows changes one attribute: the patch is a few
/// bytes whatever the page's size.
#[test]
fn a_keystroke_patches_what_it_changed() {
    let page = |typed: &str| -> Node<Msg> {
        view! { Div [
            view! { Div => (0..200).map(row) },
            view! { Input {Value => typed} @input(input) },
            view! { Button {Disabled => typed.is_empty()} },
        ] }
    };
    let (mut painter, _) = Painter::start(page(""));
    let ps = painter.paint(page("B"));
    assert_eq!(ps.len(), 2, "{ps:?}");
    assert!(patch::encode(&ps).len() < 24);
}
