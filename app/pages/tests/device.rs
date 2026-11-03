//! The device section offers a new key wherever this device holds a key row, read or not, and
//! nowhere else; and the code from the box only to a key nobody trusts yet.

use suzatary_pages::runtime::{Answer, Context, Gate, Input};
use suzatary_pages::settings::{Flags, Hold, Model, Msg, Settings};
use viewtree::kit::Reading;
use viewtree::{Attr, Kids, Node, Program};

/// The page once the box's overview and this device are held.
fn model(signing: &serde_json::Value) -> Model {
    let (mut model, _) = Settings::init(Flags {
        context: Context {
            gate: Gate::Writes,
            offsets: Vec::new(),
        },
    });
    let held = |ask, body: serde_json::Value| {
        Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val {
                v: Answer::Done { body },
            },
            behind: None,
        }))
    };
    let overview = serde_json::json!({ "groups": [], "ask": "default", "offers": [] });
    let device = serde_json::json!({
        "persist": { "type": "Granted" },
        "usage": { "bytes": 0.0, "count": 0 },
        "budget": 1.0,
        "signing": signing
    });
    Settings::update(&mut model, held(Hold::Overview, overview));
    Settings::update(&mut model, held(Hold::Device, device));
    model
}

fn words<M>(n: &Node<M>) -> String {
    match n {
        Node::Text((), t) => t.clone(),
        Node::Foreign(..) => String::new(),
        Node::Lazy(l) => words(&l.draw()),
        Node::Element(e) => kids(&e.kids).map(words).collect(),
    }
}

fn kids<M>(k: &Kids<M>) -> Box<dyn Iterator<Item = &Node<M>> + '_> {
    match k {
        Kids::Plain(xs) => Box::new(xs.iter()),
        Kids::Keyed(xs) => Box::new(xs.iter().map(|(_, n)| n)),
    }
}

/// The words of every element of the page whose class names `act`.
fn offered(signing: &serde_json::Value, act: &str) -> Vec<String> {
    fn walk<M>(n: &Node<M>, act: &str, out: &mut Vec<String>) {
        match n {
            Node::Element(e) => {
                let classed = e
                    .attrs
                    .iter()
                    .any(|(a, v)| *a == Attr::Class && v.split(' ').any(|c| c == act));
                if classed {
                    out.push(words(n));
                }
                kids(&e.kids).for_each(|k| walk(k, act, out));
            }
            Node::Lazy(l) => walk(&l.draw(), act, out),
            Node::Text(..) | Node::Foreign(..) => {}
        }
    }
    let model = model(signing);
    let mut out = Vec::new();
    walk(&Settings::view(&model), act, &mut out);
    out
}

#[test]
fn an_unreadable_key_row_is_offered_a_new_key_and_a_device_with_no_key_row_is_not() {
    let renews = |s: serde_json::Value| offered(&s, "act-renewkey");
    let key = "ab".repeat(32);
    assert_eq!(
        renews(
            serde_json::json!({ "type": "Unreadable", "e": "it is not a key this browser made" })
        ),
        ["Use a new key on this device"]
    );
    assert_eq!(
        renews(serde_json::json!({ "type": "Keyed", "key": key, "standing": "untrusted" })),
        ["Use a new key on this device"]
    );
    for s in [
        serde_json::json!({ "type": "Unheard" }),
        serde_json::json!({ "type": "Reads" }),
        serde_json::json!({ "type": "Unsupported", "e": "this browser keeps no keys" }),
    ] {
        assert_eq!(renews(s.clone()), Vec::<String>::new(), "{s}");
    }
}

#[test]
fn an_untrusted_key_is_offered_the_code_from_the_box_and_a_trusted_or_revoked_one_is_not() {
    let key = "ab".repeat(32);
    let keyed = |standing| serde_json::json!({ "type": "Keyed", "key": key, "standing": standing });
    assert_eq!(
        offered(&keyed("untrusted"), "act-pair"),
        ["Enter the code from the box"]
    );
    assert_eq!(offered(&keyed("trusted"), "act-pair"), Vec::<String>::new());
    assert_eq!(offered(&keyed("revoked"), "act-pair"), Vec::<String>::new());
}
