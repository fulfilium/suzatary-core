use std::collections::BTreeMap;

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
    let inbox = |rows: &[(&str, usize, bool)]| {
        let open = |open: bool| {
            if open {
                json!({"kind": "open"})
            } else {
                json!({"kind": "completed", "at": "2026-11-10T04:00:00"})
            }
        };
        answered(
            inbox::Hold::Rows(Watched::Proposals),
            &rows_of(rows.iter().map(|(c, i, o)| (*c, *i, open(*o)))),
        )
    };
    let asked = |m: &inbox::Model| -> Vec<Value> {
        inbox::Inbox::subscriptions(m)
            .iter()
            .map(|(_, source)| printed(source))
            .filter(|s| s["type"] == "Watch" && s["local"]["type"] == "Cards")
            .map(|s| json!([s["local"]["conversation"], s["local"]["indices"]]))
            .collect()
    };
    let (mut m, _) = inbox::Inbox::init(inbox::Flags {
        context: context(true),
        open: String::new(),
        now: 1_790_000_000_000,
    });
    inbox::Inbox::update(
        &mut m,
        inbox(&[("a", 2, true), ("a", 5, false), ("b", 0, true)]),
    );
    assert_eq!(asked(&m), vec![json!(["a", [2]]), json!(["b", [0]])]);
    inbox::Inbox::update(
        &mut m,
        inbox(&[("a", 2, false), ("a", 7, true), ("b", 0, true)]),
    );
    assert_eq!(
        asked(&m),
        vec![json!(["a", [2, 7]]), json!(["b", [0]])],
        "grown, never shrunk"
    );
}

/// A card is read while its row is drawn, and stops being read once the row goes, though its
/// conversation's cards are still read for a tap that settles it.
#[test]
fn a_card_that_leaves_is_no_longer_read() {
    use proptest::strategy::ValueTree as _;
    use viewtree::Program;
    let rows = |ids: &[usize]| {
        answered(
            inbox::Hold::Rows(Watched::Proposals),
            &rows_of(ids.iter().map(|i| ("a", *i, json!({"kind": "open"})))),
        )
    };
    let row = proposal(2)
        .new_tree(&mut proptest::test_runner::TestRunner::deterministic())
        .expect("a row")
        .current();
    let cards_read = |m: &inbox::Model| {
        inbox::Inbox::subscriptions(m)
            .iter()
            .filter(|(slot, _)| matches!(slot, Slot::Held(inbox::Hold::Card(_))))
            .count()
    };
    let (mut m, _) = inbox::Inbox::init(inbox::Flags {
        context: context(true),
        open: String::new(),
        now: 1_790_000_000_000,
    });
    inbox::Inbox::update(&mut m, rows(&[2]));
    inbox::Inbox::update(
        &mut m,
        answered(
            inbox::Hold::Rows(Watched::Cards("a".to_owned())),
            &json!({"title": "A", "proposals": [row]}),
        ),
    );
    assert_eq!(cards_read(&m), 1, "a drawn card is read");
    inbox::Inbox::update(&mut m, rows(&[]));
    assert_eq!(cards_read(&m), 0, "a card whose row went is not");
}

/// A failed ask of a grown set keeps every card the standing answer drew, and says the problem
/// only on the rows it has no card for.
#[test]
fn a_failed_ask_keeps_the_cards_that_stood() {
    use proptest::strategy::ValueTree as _;
    use viewtree::Program;
    let open = |rows: &[usize]| {
        answered(
            inbox::Hold::Rows(Watched::Proposals),
            &rows_of(rows.iter().map(|i| ("a", *i, json!({"kind": "open"})))),
        )
    };
    let cards = |reading: Reading<Answer>| {
        inbox::Msg::Host(Box::new(Input::Held {
            ask: inbox::Hold::Rows(Watched::Cards("a".to_owned())),
            reading,
            behind: None,
        }))
    };
    let row = proposal(2)
        .new_tree(&mut proptest::test_runner::TestRunner::deterministic())
        .expect("a row")
        .current();
    let (mut m, _) = inbox::Inbox::init(inbox::Flags {
        context: context(true),
        open: String::new(),
        now: 1_790_000_000_000,
    });
    inbox::Inbox::update(&mut m, open(&[2]));
    inbox::Inbox::update(
        &mut m,
        answered(
            inbox::Hold::Rows(Watched::Cards("a".to_owned())),
            &json!({"title": "A", "proposals": [row]}),
        ),
    );
    inbox::Inbox::update(&mut m, open(&[2, 7]));
    inbox::Inbox::update(
        &mut m,
        cards(Reading::Error {
            e: "the box did not answer".to_owned(),
        }),
    );
    let drawn = format!("{:?}", viewtree::dom::fresh(&inbox::Inbox::view(&m)));
    assert!(
        drawn.contains("card/"),
        "the card that stood is still drawn: {drawn}"
    );
    assert_eq!(
        drawn.matches("decision-unresolved").count(),
        1,
        "one row says the problem"
    );
    assert!(drawn.contains("the box did not answer"));
}

/// What the person or the box does while a secret card stands: a reading or an event of the
/// page, a tap of the card and the box's answer to it.
#[derive(Debug, Clone)]
enum Standing {
    Msg(inbox::Msg),
    /// The rows, holding the card at a place or not at all.
    Rows(Value, Option<usize>),
    /// The card's body in a state, or a failed reading of it.
    Body(Option<&'static str>),
    Ui(card::Ev),
    /// The box's answer to the card's tap: done, refused, or none.
    Answer(Option<bool>),
    /// This tab's tap of the card, asked from the card in a state, and the box's word on it: on
    /// its way (`None`), taken, refused or lost; or no tap at all.
    Intent(Option<(&'static str, Option<&'static str>)>),
    /// The login's reach: writes or reads.
    Reach(bool),
}

const FIELD: &str = "secret/s/0";

/// The states the secret card's body is read in.
const STATES: [&str; 5] = ["standing", "failed", "refused", "unknown", "sent"];

fn standing() -> impl Strategy<Value = Standing> {
    // The rows, the card's intents and the reach are steps of their own, which the law follows.
    let own = |m: &inbox::Msg| {
        matches!(m, inbox::Msg::Host(i) if matches!(**i,
        Input::Held { ask: inbox::Hold::Rows(Watched::Proposals) | inbox::Hold::Intents, .. } | Input::Context(_)))
    };
    prop_oneof![
        inbox_msg()
            .prop_filter("the law's own steps", move |m| !own(m))
            .prop_map(Standing::Msg),
        (inbox_read(), proptest::option::weighted(0.8, 0usize..5))
            .prop_map(|(rows, at)| Standing::Rows(rows, at)),
        proptest::option::weighted(0.8, prop::sample::select(STATES.to_vec()))
            .prop_map(Standing::Body),
        proptest::option::of(any::<bool>()).prop_map(Standing::Answer),
        proptest::option::weighted(
            0.8,
            (
                prop::sample::select(STATES.to_vec()),
                proptest::option::of(prop::sample::select(vec!["Taken", "Refused", "Lost"])),
            ),
        )
        .prop_map(Standing::Intent),
        any::<bool>().prop_map(Standing::Reach),
        prop_oneof![
            Just(card::Ev::Open(card::Step::Saying {
                verb: "dismiss".to_owned()
            })),
            Just(card::Ev::Open(card::Step::Confirming {
                verb: "store".to_owned()
            })),
            Just(card::Ev::Close),
            Just(card::Ev::Typed("not yet".to_owned())),
            any::<bool>().prop_map(card::Ev::Details),
            Just(card::Ev::Yes(0)),
            Just(card::Ev::Dismiss),
        ]
        .prop_map(Standing::Ui),
    ]
}

/// The secret card `s/0` in `state`, which asks a value.
fn secret_card(state: &str) -> Value {
    use proptest::strategy::ValueTree as _;
    let mut secret = proposal(0)
        .new_tree(&mut proptest::test_runner::TestRunner::deterministic())
        .expect("a row")
        .current();
    secret["state"] = json!(state);
    secret["act"] =
        json!({"kind": "secret", "label": "the calendar's link", "found": null, "link": null});
    secret["card"]["answers"] = json!([{"verb": "store", "label": "Store", "done": "Stored", "asks": "value", "input": "The link"}]);
    secret["card"]["by"] = json!({"kind": "secretary"});
    secret["card"]["problem"] = json!("");
    secret["card"]["link"] = json!(null);
    secret
}

/// The first element of `seen` (depth first) that `is` takes.
fn found<'a>(
    seen: &'a [viewtree::dom::Seen],
    is: &dyn Fn(&BTreeMap<String, String>) -> bool,
) -> Option<&'a BTreeMap<String, String>> {
    seen.iter().find_map(|s| match s {
        viewtree::dom::Seen::Element { attrs, kids, .. } => {
            if is(attrs) {
                Some(attrs)
            } else {
                found(kids, is)
            }
        }
        viewtree::dom::Seen::Text(_)
        | viewtree::dom::Seen::Foreign { .. }
        | viewtree::dom::Seen::Block { .. } => None,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Law (the secret ask rests on it): whatever the order of readings (rows, card bodies in any
    /// state, prices, a chain's problem coming and going, a failed ask, the reach), of the
    /// person's steps (taps and Dismiss among them), of the card's own tap on its way, taken,
    /// refused or lost, and of the box's answers, a secret card's field is drawn while the card
    /// stands answerable to a writer, once for each such stay, and never made another card's.
    #[test]
    fn a_standing_secret_card_keeps_its_field(
        writes: bool,
        steps in proptest::collection::vec(standing(), 0..24),
    ) {
        use viewtree::Program;
        let key = card::Key { conversation: "s".to_owned(), index: 0 };
        let rows = |others: &Value, at: Option<usize>| {
            let mut rows = others.clone();
            let list = rows["payload"]["proposals"].as_array_mut().expect("rows");
            let secret = json!({"id": "ps", "asked": "proposal:s/0", "serves": null, "headline": "", "by": {"kind": "secretary"},
                "why": "", "state": {"kind": "open"}, "value": null, "at": "2026-10-05T04:00:00"});
            if let Some(at) = at {
                list.insert(at.min(list.len()), secret);
            }
            answered(inbox::Hold::Rows(Watched::Proposals), &rows)
        };
        let (mut m, _) = inbox::Inbox::init(inbox::Flags { context: context(writes), open: String::new(), now: 1_790_000_000_000 });
        inbox::Inbox::update(&mut m, rows(&json!({"payload": {"at": "", "proposals": [], "failed": []}, "held": []}), Some(0)));
        let (mut painter, first) = viewtree::Painter::start(inbox::Inbox::view(&m));
        let mut dom = viewtree::dom::Dom::default();
        dom.apply(&first);
        let (mut field, mut drawn, mut reach, mut state) = (None, 0, writes, None);
        for step in steps {
            match &step {
                Standing::Reach(writes) => reach = *writes,
                Standing::Body(Some(read)) => state = Some(*read),
                _ => {}
            }
            let msg = match step {
                Standing::Msg(msg) => msg,
                Standing::Rows(others, at) => rows(&others, at),
                Standing::Body(Some(state)) => answered(inbox::Hold::Rows(Watched::Cards("s".to_owned())), &json!({"title": "s", "proposals": [secret_card(state)]})),
                Standing::Body(None) => inbox::Msg::Host(Box::new(Input::Held {
                    ask: inbox::Hold::Rows(Watched::Cards("s".to_owned())),
                    reading: Reading::Error { e: "the box did not answer".to_owned() },
                    behind: None,
                })),
                Standing::Ui(ev) => inbox::Msg::Card(key.clone(), ev),
                Standing::Answer(answer) => inbox::Msg::Host(Box::new(Input::Answered {
                    ask: card::Asking::Act {
                        key: key.clone(),
                        then: card::Then { after: card::After::Toast { done: "Stored".to_owned() }, yes: true },
                        signed: String::new(),
                    },
                    answer: match answer {
                        Some(true) => Answer::Done { body: Text::printed(&json!({"proposals": [secret_card("sent")]})) },
                        Some(false) => Answer::Refused { status: 409, words: "refused".to_owned() },
                        None => Answer::Refused { status: 0, words: "no answer".to_owned() },
                    },
                })),
                Standing::Reach(writes) => inbox::Msg::Host(Box::new(Input::Context(context(writes)))),
                Standing::Intent(intent) => {
                    let asked: Vec<Value> = intent.into_iter().map(|(state, said)| json!({
                        "act": {"type": "Answer", "at": {"conversation": "s", "index": 0, "from": format!("{state}:0:")},
                            "answer": {"type": "Tap", "note": "", "always": false}},
                        "answer": said.map(|said| json!({"type": said, "e": "the box's word"})),
                        "retracted": false,
                    })).collect();
                    answered(inbox::Hold::Intents, &json!({ "asked": asked }))
                }
            };
            inbox::Inbox::update(&mut m, msg);
            let patches = painter.paint(inbox::Inbox::view(&m));
            for patch in &patches {
                if let viewtree::Patch::Set { id, name, value } = patch && name == "data-field" {
                    if value == FIELD {
                        drawn += 1;
                        field = Some(*id);
                    } else {
                        prop_assert_ne!(Some(*id), field, "the field became {}", value);
                    }
                }
            }
            prop_assert!(drawn <= 1, "the field was drawn {} times in one stay", drawn);
            dom.apply(&patches);
            let page = dom.page();
            let drawn_here = |what: &str| found(&page, &|a| a.get("data-field").is_some_and(|f| f == what)).is_some();
            let stands = drawn_here("card/s/0") && reach && state.is_some_and(|s| ["standing", "failed", "refused"].contains(&s));
            prop_assert!(!stands || drawn_here(FIELD), "a standing card a writer reads lost its field");
            if !drawn_here(FIELD) {
                (field, drawn) = (None, 0);
            }
        }
    }
}

fn answered(ask: inbox::Hold, body: &Value) -> inbox::Msg {
    let v = Answer::Done {
        body: Text::printed(body),
    };
    inbox::Msg::Host(Box::new(Input::Held {
        ask,
        reading: Reading::Val { v },
        behind: None,
    }))
}

/// The Inbox's rows: one secretary proposal per (conversation, index, state).
fn rows_of<'a>(rows: impl IntoIterator<Item = (&'a str, usize, Value)>) -> Value {
    let proposals: Vec<Value> = rows.into_iter().enumerate().map(|(n, (c, i, state))| json!({"id": format!("p{n}"),
        "asked": format!("proposal:{c}/{i}"), "serves": null, "headline": "", "by": {"kind": "secretary"},
        "why": "", "state": state, "value": null, "at": "2026-11-10T04:00:00"})).collect();
    json!({"payload": {"at": "", "proposals": proposals, "failed": []}, "held": []})
}
