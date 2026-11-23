//! The device section offers a new key wherever this device holds a key row, read or not, and
//! nowhere else; and the code from the box only to a key nobody trusts yet. A copy kept from before
//! the last change of form is said there once, quietly, asking nothing of the owner.

use suzatary_pages::runtime::{Answer, Context, Gate, Input, Reach, Text};
use suzatary_pages::settings::{Flags, Hold, Model, Msg, Settings};
use viewtree::kit::Reading;
use viewtree::{Attr, Kids, Node, Program};

fn model(signing: &serde_json::Value) -> Model {
    model_with(signing, &[])
}

fn model_with(signing: &serde_json::Value, earlier: &[&str]) -> Model {
    let (mut model, _) = Settings::init(Flags {
        context: Context {
            gate: Gate::Writes,
            reach: Reach::Writes,
            offsets: Vec::new(),
            keys: suzatary_pages::runtime::Keys::Touch,
            opens: suzatary_pages::runtime::Opens::Over,
            categories: vec!["home".to_owned(), "work".to_owned()],
            clock: None,
        },
    });
    let held = |ask, body: serde_json::Value| {
        Msg::Host(Box::new(Input::Held {
            ask,
            reading: Reading::Val {
                v: Answer::Done {
                    body: Text::printed(&body),
                },
            },
            behind: None,
        }))
    };
    let overview = serde_json::json!({ "groups": [], "ask": "default", "offers": [] });
    let device = serde_json::json!({
        "persist": { "type": "Granted" },
        "usage": { "bytes": 0.0, "count": 0 },
        "budget": 1.0,
        "signing": signing,
        "earlier": earlier
    });
    Settings::update(&mut model, held(Hold::Overview, overview));
    Settings::update(&mut model, held(Hold::Device, device));
    model
}

fn words<M>(n: &Node<M>) -> String {
    match n {
        Node::Text((), t) => t.clone(),
        Node::Foreign(..) => String::new(),
        Node::Block((), b) => b.fallback.clone(),
        Node::Lazy(l) => words(&l.draw()),
        Node::Element(e) => kids(&e.kids).map(words).collect(),
    }
}

fn kids<M>(k: &Kids<M>) -> impl Iterator<Item = &Node<M>> {
    k.nodes()
}

fn offered(signing: &serde_json::Value, act: &str) -> Vec<String> {
    classed(&model(signing), act)
}

fn classed(model: &Model, act: &str) -> Vec<String> {
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
            Node::Text(..) | Node::Foreign(..) | Node::Block(..) => {}
        }
    }
    let mut out = Vec::new();
    walk(&Settings::view(model), act, &mut out);
    out
}

#[test]
fn a_copy_from_before_the_update_is_said_once_quietly_and_none_is_said_without_one() {
    let signing = serde_json::json!({ "type": "Reads" });
    let said = classed(
        &model_with(&signing, &["topics", "topic-aliases"]),
        "settingsearlier",
    );
    assert_eq!(said.len(), 1, "one sentence for every copy");
    assert!(
        said[0]
            .starts_with("A copy of the topics and the topic aliases from before the last update")
    );
    assert!(said[0].contains("was not carried over: if one is missing, make it again"));
    assert!(
        !said[0].contains("nothing needs doing"),
        "a write in the copy is never sent"
    );
    assert!(classed(&model_with(&signing, &[]), "settingsearlier").is_empty());
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
