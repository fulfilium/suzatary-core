use crate::cards::*;
use crate::common::*;

fn inbox_msg() -> impl Strategy<Value = inbox::Msg> {
    let host = |i: Input<inbox::Hold, inbox::Ask>| inbox::Msg::Host(Box::new(i));
    prop_oneof![
        watched().prop_flat_map(move |(w, v)| held(inbox::Hold::Rows(w), Just(v))).prop_map(host),
        held(inbox::Hold::Rows(Watched::Decisions("life".to_owned())), Just(json!({"at": "2026-10-05T04:00:00", "heads": [], "entries": [], "failed": [], "next": null}))).prop_map(host),
        held(inbox::Hold::Intents, Just(json!({"asked": [{"act": {"type": "Answer", "at": {"conversation": "setup", "index": 0, "from": "standing:0:"}, "answer": {"type": "Dismiss", "reason": "no"}}, "answer": null, "retracted": false}]}))).prop_map(host),
        card_ev().prop_map(|(k, ev)| inbox::Msg::Card(k, ev)),
        any::<bool>().prop_map(inbox::Msg::Setup),
        any::<bool>().prop_map(inbox::Msg::Settled),
        any::<bool>().prop_map(move |w| host(Input::Context(context(w)))),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn inbox_laws(writes: bool, opened: bool, msgs in proptest::collection::vec(inbox_msg(), 0..12)) {
        page_laws::<inbox::Inbox>(
            || inbox::Flags { context: context(writes), open: if opened { "p0".to_owned() } else { String::new() }, now: 1_790_000_000_000 },
            &msgs,
        );
    }
}

/// Each card is asked of the box for the proposals the open rows name and no other, and an index
/// once asked stays asked, so a card tapped and settled is still read.
#[test]
fn a_card_is_asked_for_what_the_rows_name() {
    use viewtree::Program;
    let state = |open: bool| {
        if open {
            json!({"kind": "open"})
        } else {
            json!({"kind": "completed", "at": "2026-11-10T04:00:00"})
        }
    };
    let inbox = |rows: &[(&str, usize, bool)]| {
        let proposals: Vec<Value> = rows.iter().enumerate().map(|(n, (c, i, open))| json!({"id": format!("p{n}"),
            "asked": format!("proposal:{c}/{i}"), "serves": null, "headline": "", "by": {"kind": "secretary"},
            "why": "", "state": state(*open), "value": null, "at": "2026-11-10T04:00:00"})).collect();
        let body = json!({"payload": {"at": "", "proposals": proposals, "failed": []}, "held": []});
        inbox::Msg::Host(Box::new(Input::Held {
            ask: inbox::Hold::Rows(Watched::Proposals),
            reading: Reading::Val {
                v: Answer::Done { body },
            },
            behind: None,
        }))
    };
    let asked = |effects: Vec<Effect<inbox::Hold, inbox::Ask>>| -> Vec<Value> {
        effects
            .iter()
            .map(printed)
            .filter(|e| e["type"] == "Watch" && e["local"]["type"] == "Cards")
            .map(|e| json!([e["local"]["conversation"], e["local"]["indices"]]))
            .collect()
    };
    let (mut m, _) = inbox::Inbox::init(inbox::Flags {
        context: context(true),
        open: String::new(),
        now: 1_790_000_000_000,
    });
    let first = inbox::Inbox::update(
        &mut m,
        inbox(&[("a", 2, true), ("a", 5, false), ("b", 0, true)]),
    );
    assert_eq!(asked(first), vec![json!(["a", [2]]), json!(["b", [0]])]);
    let next = inbox::Inbox::update(
        &mut m,
        inbox(&[("a", 2, false), ("a", 7, true), ("b", 0, true)]),
    );
    assert_eq!(
        asked(next),
        vec![json!(["a", [2, 7]])],
        "grown, never shrunk"
    );
}
